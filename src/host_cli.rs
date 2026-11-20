//! Commands you run on your machine; they act on the current team (or `--team`).

mod generate;

use crate::client::Client;
use crate::files;
use crate::rin_bail;
use crate::rinerrln;
use crate::rinlogln;
use crate::team::{self, Team};
use crate::{display, shared::config};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

/// `docker compose -p rin-<name> -f $RIN_HOME/teams/<name>/docker-compose.yml`
pub fn compose(team: &Team) -> Command {
    let mut command = Command::new("docker");
    command
        .arg("compose")
        .arg("-p")
        .arg(team.project())
        .arg("-f")
        .arg(team.compose_file());
    command
}

fn run(mut command: Command) -> Result<()> {
    let status = command
        .status()
        .context("cannot run docker (is Docker installed and running?)")?;
    if !status.success() {
        rin_bail!("docker compose failed ({status})");
    }
    Ok(())
}

/// The client you use on your machine: your token, the published hub.
pub fn client(team: &Team) -> Result<Client> {
    let tokens = team.tokens()?;
    let token = tokens
        .get("user")
        .context("tokens.json has no token for you: run `rin generate`")?;
    Ok(Client::new(team::HOST_HUB_URL, token.as_str()))
}

/// Service → state (`running`, `exited`, …) from `docker compose ps`.
fn states(team: &Team) -> Result<HashMap<String, String>> {
    let output = compose(team)
        .args(["ps", "--all", "--format", "json"])
        .output()
        .context("cannot run docker (is Docker installed and running?)")?;
    if !output.status.success() {
        rin_bail!(
            "docker compose ps failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let text = String::from_utf8_lossy(&output.stdout);
    // Newer Compose prints one JSON object per line, older ones an array.
    let rows: Vec<serde_json::Value> =
        match serde_json::from_str::<Vec<serde_json::Value>>(text.trim()) {
            Ok(rows) => rows,
            Err(_) => text
                .lines()
                .filter_map(|l| serde_json::from_str(l).ok())
                .collect(),
        };
    Ok(rows
        .iter()
        .filter_map(|r| {
            Some((
                r.get("Service")?.as_str()?.to_string(),
                r.get("State")?.as_str()?.to_string(),
            ))
        })
        .collect())
}

/// Other Rin teams Docker has running (from `docker compose ls`).
fn running_teams(except: &Team) -> Vec<String> {
    let Ok(output) = Command::new("docker")
        .args(["compose", "ls", "--format", "json"])
        .output()
    else {
        return vec![];
    };
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap_or_default();
    rows.iter()
        .filter(|r| {
            r.get("Status")
                .and_then(|s| s.as_str())
                .is_some_and(|s| s.starts_with("running"))
        })
        .filter_map(|r| {
            r.get("Name")?
                .as_str()?
                .strip_prefix("rin-")
                .map(str::to_string)
        })
        .filter(|name| *name != except.name)
        .collect()
}

pub fn up(team: &Team) -> Result<()> {
    let states = states(team)?;
    let hub_up = states.get("hub").is_some_and(|s| s == "running");
    let any_up = states.values().any(|s| s == "running");
    if !hub_up && std::net::TcpListener::bind(("127.0.0.1", team::HUB_PORT)).is_err() {
        rin_bail!(
            "cannot start team {}: port {} on 127.0.0.1 is already in use{}. Only one Rin team can be up at a time: run `rin down --team <name>` first",
            team.name,
            team::HUB_PORT,
            others_up(team)
        );
    }
    // --build: an agent that switched harness, or a new RIN_GIT_REV, needs a new
    // image under the same tag; unchanged images come straight from the cache.
    // --remove-orphans: an agent taken out of rin.yml stops with the rest.
    let mut command = compose(team);
    command.args(["up", "-d", "--build", "--remove-orphans"]);
    if let Err(e) = run(command) {
        if !any_up {
            // Leave nothing half-started behind (but never stop what was already running).
            let mut down = compose(team);
            down.args(["down", "--remove-orphans"]);
            let _ = down.status();
        }
        let state = if any_up {
            "may not be fully up (`rin status` shows what runs)"
        } else {
            "is not up"
        };
        rin_bail!(
            "{e}\nteam {} {state}; if the error is about port {}, another Rin team or program is using it{}",
            team.name,
            team::HUB_PORT,
            others_up(team)
        );
    }
    // Last, so the build output doesn't scroll it away.
    let keys_env = team::rin_home()?.join("keys.env");
    if !keys_env.is_file() && team.config().is_ok_and(|c| c.uses_api_keys()) {
        rinerrln!(
            "rin: warning: {} is missing: API-key agents will fail. Add your provider keys, then run `rin generate` and `rin up`",
            keys_env.display()
        );
    }
    Ok(())
}

/// ` (team x is up)` when Docker runs other Rin teams.
fn others_up(team: &Team) -> String {
    match running_teams(team).as_slice() {
        [] => String::new(),
        names => format!(" (team {} is up)", names.join(", ")),
    }
}

pub fn down(team: &Team) -> Result<()> {
    let mut command = compose(team);
    command.args(["down", "--remove-orphans"]);
    run(command)
}

/// Replaces this process with a shell in the agent's container.
pub fn shell(team: &Team, id: &str) -> Result<()> {
    if team.config()?.agent(id).is_none() {
        rin_bail!("unknown agent: {id:?}");
    }
    let mut command = compose(team);
    command.args(["exec", "-u", "agent", id, "bash"]);
    let error = command.exec();
    Err(error).context("cannot run docker (is Docker installed and running?)")
}

pub async fn status(team: &Team) -> Result<()> {
    let config = team.config()?;
    let states = states(team)?;
    let hub = client(team)?.agents().await;
    let mut text = format!("team {} (rin {})\n", team.name, config.version);
    let mut rows = vec![
        [
            "ID", "LABEL", "TYPE", "HARNESS", "STATE", "SESSIONS", "QUEUED",
        ]
        .map(String::from)
        .to_vec(),
    ];
    for agent in &config.agents {
        let state = match states.get(&agent.id).map(String::as_str) {
            Some("exited") => "stopped".to_string(),
            Some(state) => state.to_string(),
            None => "down".to_string(),
        };
        let row = hub
            .as_ref()
            .ok()
            .and_then(|h| h.agents.iter().find(|a| a.id == agent.id));
        let (sessions, queued) = match row {
            Some(a) => (
                format!("{}/{}", a.sessions, a.max_sessions),
                a.queued.to_string(),
            ),
            None => ("-".into(), "-".into()),
        };
        rows.push(vec![
            agent.id.clone(),
            agent.label.clone(),
            agent.agent_type.as_str().into(),
            agent.harness.as_str().into(),
            state,
            sessions,
            queued,
        ]);
    }
    text.push_str(&display::table(&rows));
    match &hub {
        Ok(hub) => hub
            .warnings
            .iter()
            .for_each(|w| text.push_str(&format!("{w}\n"))),
        Err(e) => text.push_str(&format!("hub: {e}\n")),
    }
    if config.uses_api_keys() {
        let bifrost = states.get("bifrost").map_or("down", String::as_str);
        text.push_str(&format!("bifrost: {bifrost}\n"));
    }
    display::out(&text)?;
    Ok(())
}

pub async fn log(team: &Team, follow: bool, agent: Option<&str>) -> Result<()> {
    if let Some(id) = agent
        && team.config()?.agent(id).is_none()
    {
        rin_bail!("unknown agent: {id:?}");
    }
    let mut client = client(team)?;
    let mut after = 0;
    let mut lost = false;
    loop {
        match client.log(agent, after).await {
            Ok(page) => {
                if std::mem::take(&mut lost) {
                    rinerrln!("rin: following again");
                }
                let more = page.len() >= crate::proto::LOG_PAGE;
                for event in page {
                    display::out(&display::event(&event))?;
                    after = event.id;
                }
                if more {
                    continue;
                }
                if !follow {
                    return Ok(());
                }
            }
            Err(e) if follow => {
                if !std::mem::replace(&mut lost, true) {
                    rinerrln!("rin: {e:#} (still following)");
                }
                // The hub may come back with new tokens (`rin generate && rin up`).
                if let Ok(fresh) = self::client(team) {
                    client = fresh;
                }
            }
            Err(e) => return Err(e),
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

pub fn use_team(name: &str) -> Result<()> {
    let team = Team::resolve(Some(name))?;
    team::set_current(&team::rin_home()?, &team.name)?;
    println!("current team: {}", team.name);
    Ok(())
}

pub fn generate_config(config_file: PathBuf, instructions: PathBuf) -> Result<()> {
    // RIN_GIT_REV builds the images from another Rin revision (e.g. refs/heads/<branch>)
    // to try unreleased changes; releases build from the `v<version>` tag.
    let git_rev = std::env::var("RIN_GIT_REV")
        .ok()
        .filter(|rev| !rev.is_empty())
        .unwrap_or_else(|| format!("v{}", config::VERSION));
    let generated = generate::generate(&config_file, &instructions, &team::rin_home()?, &git_rev)?;
    for warning in &generated.warnings {
        rinerrln!("warning: {warning}");
    }
    println!(
        "team {} written to {} (now the current team)",
        generated.team.name,
        generated.team.dir.display()
    );
    Ok(())
}

pub fn init_project(name: Option<String>, path: PathBuf) -> Result<()> {
    let name = name.unwrap_or(config::Config::dummy_team_name());
    let path = path.join(&name);
    ensure_project_dir(&path)?;
    ensure_not_initialized(&path)?;

    let (config, instructions) = config::Config::init(name);

    let config_path = path.join("rin.yml");
    let yaml = format!(
        "{}\n{}",
        config::rin_yaml_schema_directive(),
        serde_saphyr::to_string(&config)?
    );
    files::write_if_changed(&config_path, yaml.as_bytes())?;

    for (agent, text) in config.agents.iter().zip(&instructions) {
        files::write_if_changed(
            &path.join("agents").join(&agent.id).join("INSTRUCTIONS.md"),
            text.as_bytes(),
        )?;
    }

    rinlogln!("Done! Happy hacking with Rin!");

    Ok(())
}

fn ensure_project_dir(path: &Path) -> Result<()> {
    if path.exists() && !path.is_dir() {
        rin_bail!("{} is not a directory", path.display());
    }

    fs::create_dir_all(&path).with_context(|| format!("Cannot create {}", path.display()))?;
    Ok(())
}

fn ensure_not_initialized(path: &Path) -> Result<()> {
    let rin_path = path.join("rin.yml");
    if rin_path.is_file() {
        rin_bail!("{} is already a Rin project", path.display());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Scratch {
            let dir = std::env::temp_dir().join(format!("rin-init-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn init_writes_rin_yml_and_instructions() {
        let s = Scratch::new("files");
        init_project(Some("my-team".into()), s.0.clone()).unwrap();
        let project = s.0.join("my-team");
        let (expected, instructions) = config::Config::init("my-team".into());

        let yaml = fs::read_to_string(project.join("rin.yml")).unwrap();
        assert!(yaml.starts_with(&format!("{}\n", config::rin_yaml_schema_directive())));
        assert_eq!(config::from_yaml(&yaml).unwrap(), expected);

        let mut folders: Vec<_> = fs::read_dir(project.join("agents"))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        folders.sort();
        let mut ids: Vec<_> = expected.agents.iter().map(|a| a.id.clone()).collect();
        ids.sort();
        assert_eq!(folders, ids);
        for (agent, text) in expected.agents.iter().zip(&instructions) {
            let file = project
                .join("agents")
                .join(&agent.id)
                .join("INSTRUCTIONS.md");
            assert_eq!(
                &fs::read_to_string(&file).unwrap(),
                text,
                "{}",
                file.display()
            );
        }
    }

    #[test]
    fn init_without_a_name_makes_rinteam() {
        let s = Scratch::new("default-name");
        init_project(None, s.0.clone()).unwrap();
        let yaml = fs::read_to_string(s.0.join("rinteam/rin.yml")).unwrap();
        assert_eq!(config::from_yaml(&yaml).unwrap().name, "rinteam");
    }

    #[test]
    fn init_refuses_a_file_in_the_way() {
        let s = Scratch::new("file");
        fs::write(s.0.join("my-team"), "mine").unwrap();
        let e = init_project(Some("my-team".into()), s.0.clone()).unwrap_err();
        assert!(format!("{e:#}").contains("is not a directory"), "{e:#}");
        assert_eq!(fs::read_to_string(s.0.join("my-team")).unwrap(), "mine");
    }

    #[test]
    fn rin_generate_accepts_what_init_writes() {
        let s = Scratch::new("generate");
        init_project(Some("my-team".into()), s.0.clone()).unwrap();
        let project = s.0.join("my-team");
        let generated = generate::generate(
            &project.join("rin.yml"),
            &project.join("agents"),
            &s.0.join("home"),
            config::VERSION,
        )
        .unwrap();
        assert_eq!(generated.team.name, "my-team");
    }
}
