//! Commands you run on your machine; they act on the current team (or `--team`).

use std::collections::HashMap;
use std::os::unix::process::CommandExt;
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::client::Client;
use crate::display;
use crate::team::{self, Team};

/// `docker compose -p rin-<name> -f $RIN_HOME/teams/<name>/docker-compose.yml`
pub fn compose(team: &Team) -> Command {
    let mut command = Command::new("docker");
    command.arg("compose").arg("-p").arg(team.project()).arg("-f").arg(team.compose_file());
    command
}

fn run(mut command: Command) -> Result<()> {
    let status = command.status().context("cannot run docker (is Docker installed and running?)")?;
    if !status.success() {
        bail!("docker compose failed ({status})");
    }
    Ok(())
}

/// The client you use on your machine: your token, the published hub.
pub fn client(team: &Team) -> Result<Client> {
    let tokens = team.tokens()?;
    let token = tokens.get("user").context("tokens.json has no token for you: run `rin generate`")?;
    Ok(Client::new(team::HOST_HUB_URL, token.as_str()))
}

/// Service → state (`running`, `exited`, …) from `docker compose ps`.
fn states(team: &Team) -> Result<HashMap<String, String>> {
    let output = compose(team)
        .args(["ps", "--all", "--format", "json"])
        .output()
        .context("cannot run docker (is Docker installed and running?)")?;
    if !output.status.success() {
        bail!("docker compose ps failed: {}", String::from_utf8_lossy(&output.stderr).trim());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    // Newer Compose prints one JSON object per line, older ones an array.
    let rows: Vec<serde_json::Value> = match serde_json::from_str::<Vec<serde_json::Value>>(text.trim()) {
        Ok(rows) => rows,
        Err(_) => text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect(),
    };
    Ok(rows
        .iter()
        .filter_map(|r| Some((r.get("Service")?.as_str()?.to_string(), r.get("State")?.as_str()?.to_string())))
        .collect())
}

/// Other Rin teams Docker has running (from `docker compose ls`).
fn running_teams(except: &Team) -> Vec<String> {
    let Ok(output) = Command::new("docker").args(["compose", "ls", "--format", "json"]).output() else { return vec![] };
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap_or_default();
    rows.iter()
        .filter(|r| r.get("Status").and_then(|s| s.as_str()).is_some_and(|s| s.starts_with("running")))
        .filter_map(|r| r.get("Name")?.as_str()?.strip_prefix("rin-").map(str::to_string))
        .filter(|name| *name != except.name)
        .collect()
}

pub fn up(team: &Team) -> Result<()> {
    let already_up = states(team)?.get("hub").is_some_and(|s| s == "running");
    if !already_up && std::net::TcpListener::bind(("127.0.0.1", team::HUB_PORT)).is_err() {
        bail!("{}", port_taken(team));
    }
    if let Err(e) = run({
        let mut c = compose(team);
        c.args(["up", "-d"]);
        c
    }) {
        if !already_up {
            // Leave nothing half-started behind.
            let mut down = compose(team);
            down.arg("down");
            let _ = down.status();
        }
        bail!("{e}\nteam {} is not up; {}", team.name, port_hint(team));
    }
    Ok(())
}

fn port_taken(team: &Team) -> String {
    let others = running_teams(team);
    let by = match others.as_slice() {
        [] => String::new(),
        names => format!(" (team {} is up)", names.join(", ")),
    };
    format!(
        "cannot start team {}: port {} on 127.0.0.1 is already in use{by}. Only one Rin team can be up at a time: run `rin down --team <name>` first",
        team.name,
        team::HUB_PORT
    )
}

fn port_hint(team: &Team) -> String {
    format!("if the error is about port {}, another Rin team or program is using it", team::HUB_PORT)
        + &match running_teams(team).as_slice() {
            [] => String::new(),
            names => format!(" (team {} is up)", names.join(", ")),
        }
}

pub fn down(team: &Team) -> Result<()> {
    let mut command = compose(team);
    command.arg("down");
    run(command)
}

/// Replaces this process with a shell in the agent's container.
pub fn shell(team: &Team, id: &str) -> Result<()> {
    if team.config()?.agent(id).is_none() {
        bail!("unknown agent: {id:?}");
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
    println!("team {} (rin {})", team.name, config.version);
    let mut rows = vec![["ID", "LABEL", "TYPE", "HARNESS", "STATE", "SESSIONS", "QUEUED"].map(String::from).to_vec()];
    for agent in &config.agents {
        let state = match states.get(&agent.id).map(String::as_str) {
            Some("exited") => "stopped".to_string(),
            Some(state) => state.to_string(),
            None => "down".to_string(),
        };
        let row = hub.as_ref().ok().and_then(|h| h.agents.iter().find(|a| a.id == agent.id));
        let (sessions, queued) = match row {
            Some(a) => (format!("{}/{}", a.sessions, a.max_sessions), a.queued.to_string()),
            None => ("-".into(), "-".into()),
        };
        rows.push(vec![
            agent.id.clone(),
            agent.label.clone(),
            agent.kind.as_str().into(),
            agent.harness.as_str().into(),
            state,
            sessions,
            queued,
        ]);
    }
    print!("{}", display::table(&rows));
    match &hub {
        Ok(hub) => hub.warnings.iter().for_each(|w| println!("{w}")),
        Err(e) => println!("hub: {e}"),
    }
    if config.uses_api_keys() {
        let bifrost = states.get("bifrost").map_or("down", String::as_str);
        println!("bifrost: {bifrost}");
    }
    Ok(())
}

pub async fn log(team: &Team, follow: bool, agent: Option<&str>) -> Result<()> {
    if let Some(id) = agent
        && team.config()?.agent(id).is_none()
    {
        bail!("unknown agent: {id:?}");
    }
    let client = client(team)?;
    let mut after = 0;
    loop {
        for event in client.log(agent, after).await? {
            print!("{}", display::event(&event));
            after = event.id;
        }
        if !follow {
            return Ok(());
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
