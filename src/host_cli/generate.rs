//! `rin generate`: validate `rin.yml` + `agents/` and write the team's directory in `$RIN_HOME`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::json;

use crate::compose;
use crate::files::{random_hex, write_if_changed, write_private, write_shared};
use crate::rin_bail;
use crate::shared::config::{self, Authentication, Config, VERSION};
use crate::team::{self, Team, Tokens, read_tokens};

pub const DOCKERFILE: &str = include_str!("../../images/Dockerfile");

/// Built-in providers: Rin's id, Bifrost's name, and the `keys.env` variable.
const BUILT_IN: [(&str, &str, &str); 4] = [
    ("anthropic", "anthropic", "ANTHROPIC_API_KEY"),
    ("openai", "openai", "OPENAI_API_KEY"),
    ("google", "gemini", "GEMINI_API_KEY"),
    ("ollama", "ollama", "OLLAMA_API_KEY"),
];

/// Where Bifrost reaches the built-in `ollama`: Ollama Cloud.
const OLLAMA_CLOUD_URL: &str = "https://ollama.com";

/// Where an API-key agent finds its virtual key: every variable its harness may read.
const AGENT_KEY_VARS: [&str; 3] = ["ANTHROPIC_API_KEY", "OPENAI_API_KEY", "GEMINI_API_KEY"];

#[derive(Debug)]
pub struct Generated {
    pub team: Team,
    pub warnings: Vec<String>,
}

/// Writes the team for `file` into `home` (`$RIN_HOME`). Images build `rin` from `git_rev`.
pub fn generate(file: &Path, instructions: &Path, home: &Path, git_rev: &str) -> Result<Generated> {
    let text =
        fs::read_to_string(file).with_context(|| format!("cannot read {}", file.display()))?;
    let config = config::from_yaml(&text).with_context(|| file.display().to_string())?;
    if config.version != VERSION {
        rin_bail!(
            "{}: version {} does not match this rin ({VERSION}); install rin {} or change `version:`",
            file.display(),
            config.version,
            config.version
        );
    }
    let sources = check_instructions(&config, instructions)?;

    check_names(home, &config)?;

    let team = Team::at(home, &config.name);
    let new_home = !home.exists();
    fs::create_dir_all(&team.dir)
        .with_context(|| format!("cannot create {}", team.dir.display()))?;
    // Only lock down what Rin made: $RIN_HOME may be a directory you use for more.
    if new_home {
        crate::files::set_mode(home, 0o700)?;
    }
    crate::files::set_mode(&home.join("teams"), 0o700)?;
    let mut warnings = Vec::new();
    // Your provider keys: whoever made the file, only you may read it.
    let keys_env = home.join("keys.env");
    if keys_env.is_file()
        && let Err(e) = crate::files::set_mode(&keys_env, 0o600)
    {
        warnings.push(format!("{e:#}: make it readable by you only"));
    }

    let source = fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    write_if_changed(
        &team.dir.join("source"),
        format!("{}\n", source.display()).as_bytes(),
    )?;
    let rin_json = format!("{}\n", serde_json::to_string_pretty(&config)?);
    write_if_changed(&team.rin_json(), rin_json.as_bytes())?;
    write_instructions(&team, &sources)?;
    write_if_changed(&team.dir.join("images/Dockerfile"), DOCKERFILE.as_bytes())?;

    let tokens = tokens(&team, &config)?;
    let tokens_json = format!("{}\n", serde_json::to_string_pretty(&tokens)?);
    write_private(&team.tokens_file(), tokens_json.as_bytes())?;

    let virtual_keys = if config.uses_api_keys() {
        if let Some(problem) = missing_keys(&keys_env, &config) {
            warnings.push(format!(
                "{problem}: add them, then run `rin generate` again (only Bifrost reads them)"
            ));
        }
        write_bifrost(&team, &config, &defined_keys(&keys_env))?
    } else {
        BTreeMap::new()
    };
    write_env_files(&team, &config, &tokens, &virtual_keys)?;

    let hub_files_hash = compose::files_hash(&[rin_json.as_bytes(), tokens_json.as_bytes()]);
    let compose = compose::render(&compose::Inputs {
        config: &config,
        git_rev,
        hub_files_hash: &hub_files_hash,
    });
    write_if_changed(&team.compose_file(), compose.as_bytes())?;

    team::set_current(home, &config.name)?;
    Ok(Generated { team, warnings })
}

/// Every agent has `<dir>/<id>/INSTRUCTIONS.md`, and every directory is an agent.
fn check_instructions(config: &Config, dir: &Path) -> Result<Vec<(String, PathBuf)>> {
    if !dir.is_dir() {
        rin_bail!("instructions directory {} not found", dir.display());
    }
    let mut problems = Vec::new();
    let mut entries: Vec<_> = fs::read_dir(dir)
        .with_context(|| format!("cannot read {}", dir.display()))?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with('.') && entry.path().is_dir() && config.agent(&name).is_none() {
            problems.push(format!(
                "{}: no agent with id {name:?} in rin.yml",
                entry.path().display()
            ));
        }
    }
    let mut sources = Vec::new();
    for agent in &config.agents {
        let agent_dir = dir.join(&agent.id);
        let file = agent_dir.join("INSTRUCTIONS.md");
        if !agent_dir.is_dir() {
            problems.push(format!(
                "agent {:?} has no directory {}",
                agent.id,
                agent_dir.display()
            ));
        } else if !file.is_file() {
            problems.push(format!(
                "{}: INSTRUCTIONS.md is missing",
                agent_dir.display()
            ));
        } else {
            sources.push((agent.id.clone(), file));
        }
    }
    if !problems.is_empty() {
        rin_bail!(
            "instructions don't match rin.yml:\n  {}",
            problems.join("\n  ")
        );
    }
    Ok(sources)
}

/// Copies `agents/<id>/INSTRUCTIONS.md` so the team keeps working if the project moves.
fn write_instructions(team: &Team, sources: &[(String, PathBuf)]) -> Result<()> {
    let root = team.dir.join("instructions");
    for (id, file) in sources {
        let text = fs::read(file).with_context(|| format!("cannot read {}", file.display()))?;
        write_shared(&root.join(id).join("INSTRUCTIONS.md"), &text)?;
    }
    remove_others(&root, sources.iter().map(|(id, _)| id.clone()))
}

/// Removes entries of `dir` not in `keep`.
fn remove_others(dir: &Path, keep: impl IntoIterator<Item = String>) -> Result<()> {
    let keep: Vec<String> = keep.into_iter().collect();
    let Ok(entries) = fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries.filter_map(|e| e.ok()) {
        if !keep.contains(&entry.file_name().to_string_lossy().into_owned()) {
            let path = entry.path();
            let result = if path.is_dir() {
                fs::remove_dir_all(&path)
            } else {
                fs::remove_file(&path)
            };
            result.with_context(|| format!("cannot remove {}", path.display()))?;
        }
    }
    Ok(())
}

/// Refuses a team whose Docker names another team in `home` already uses
/// (team `a` agent `b-c` and team `a-b` agent `c` are both `rin-a-b-c`).
fn check_names(home: &Path, config: &Config) -> Result<()> {
    let ours = compose::docker_names(config);
    let Ok(entries) = fs::read_dir(home.join("teams")) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == config.name {
            continue;
        }
        // A team directory Rin can't read (another version, half deleted) has no say.
        let Ok(other) = Team::at(home, &name).config() else {
            continue;
        };
        let theirs = compose::docker_names(&other);
        if let Some((kind, clash)) = ours.iter().find(|n| theirs.contains(n)) {
            rin_bail!(
                "team {:?} would use the Docker {kind} name {clash:?}, which team {name:?} already uses: rename the team or an agent",
                config.name
            );
        }
    }
    Ok(())
}

/// Keeps existing tokens; makes new ones for new agents (or all, if `tokens.json` was deleted).
fn tokens(team: &Team, config: &Config) -> Result<Tokens> {
    let old = read_tokens(&team.tokens_file())?.unwrap_or_default();
    let mut tokens = Tokens::new();
    for id in std::iter::once("user").chain(config.agents.iter().map(|a| a.id.as_str())) {
        let token = match old.get(id) {
            Some(token) => token.clone(),
            None => random_hex()?,
        };
        tokens.insert(id.to_string(), token);
    }
    Ok(tokens)
}

/// `env/<id>.env`: the agent's own token, plus its Bifrost virtual key if it uses an API key.
fn write_env_files(
    team: &Team,
    config: &Config,
    tokens: &Tokens,
    virtual_keys: &BTreeMap<String, String>,
) -> Result<()> {
    let dir = team.dir.join("env");
    for agent in &config.agents {
        let mut text = format!("RIN_TOKEN={}\n", tokens[&agent.id]);
        if let Some(key) = virtual_keys.get(&agent.id) {
            for var in AGENT_KEY_VARS {
                text.push_str(&format!("{var}={key}\n"));
            }
        }
        write_private(&dir.join(format!("{}.env", agent.id)), text.as_bytes())?;
    }
    remove_others(&dir, config.agents.iter().map(|a| format!("{}.env", a.id)))?;
    crate::files::set_mode(&dir, 0o700)
}

/// The variables `keys.env` sets to something. Only the names are read.
fn defined_keys(keys_env: &Path) -> Vec<String> {
    let text = fs::read_to_string(keys_env).unwrap_or_default();
    text.lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.trim_start_matches("export ").split_once('='))
        // `KEY=""` sets nothing either, to Compose.
        .filter(|(_, value)| !matches!(value.trim(), "" | "\"\"" | "''"))
        .map(|(name, _)| name.trim().to_string())
        .collect()
}

/// The `keys.env` variable a provider's key is in; `None` for a keyless custom provider.
fn key_var<'a>(config: &'a Config, provider: &str) -> Option<&'a str> {
    match BUILT_IN.iter().find(|(id, _, _)| *id == provider) {
        Some((_, _, var)) => Some(var),
        None => config.custom_provider(provider)?.api_key_env.as_deref(),
    }
}

/// The keys the team's API-key agents need that `keys.env` doesn't set, as an
/// error for you; `None` if nothing is missing. A team that only uses keyless
/// providers needs no `keys.env` at all.
pub fn missing_keys(keys_env: &Path, config: &Config) -> Option<String> {
    let defined = defined_keys(keys_env);
    let mut missing: Vec<&str> = config
        .agents
        .iter()
        .filter_map(|a| key_var(config, a.provider()?))
        .filter(|var| !defined.iter().any(|d| d == var))
        .collect();
    missing.sort();
    missing.dedup();
    if missing.is_empty() {
        None
    } else if keys_env.is_file() {
        Some(format!(
            "{} does not set {}",
            keys_env.display(),
            missing.join(", ")
        ))
    } else {
        Some(format!(
            "{} does not exist; your API-key agents need {}",
            keys_env.display(),
            missing.join(", ")
        ))
    }
}

/// Bifrost's `providers`: the built-in ones `keys.env` has a key for, and the
/// custom ones that need none or have theirs.
fn bifrost_providers(
    config: &Config,
    defined: &[String],
) -> serde_json::Map<String, serde_json::Value> {
    let has = |var: &str| defined.iter().any(|d| d == var);
    let mut providers = serde_json::Map::new();
    for (_, name, var) in BUILT_IN.iter().filter(|(_, _, var)| has(var)) {
        let mut key =
            json!({ "name": name, "value": format!("env.{var}"), "models": ["*"], "weight": 1.0 });
        if *name == "ollama" {
            key["ollama_key_config"] = json!({ "url": OLLAMA_CLOUD_URL });
        }
        providers.insert(name.to_string(), json!({ "keys": [key] }));
    }
    for provider in &config.llm_providers {
        let name = provider.gateway_name();
        let keys = match &provider.api_key_env {
            Some(var) if has(var) => vec![
                json!({ "name": name, "value": format!("env.{var}"), "models": ["*"], "weight": 1.0 }),
            ],
            Some(_) => continue,
            None => vec![],
        };
        // `baseUrl` is what an OpenAI client takes (`…/v1`); Bifrost would add `/v1` again.
        let chat = "/chat/completions";
        providers.insert(
            name,
            json!({
                "keys": keys,
                // A server on your machine or network has a private address, which
                // Bifrost refuses unless allowed; you chose this URL yourself.
                "network_config": { "base_url": provider.base_url.trim_end_matches('/'), "allow_private_network": true },
                "custom_provider_config": {
                    "base_provider_type": "openai",
                    "is_key_less": !provider.api_key_required,
                    "allowed_requests": { "chat_completion": true, "chat_completion_stream": true },
                    "request_path_overrides": { "chat_completion": chat, "chat_completion_stream": chat },
                },
            }),
        );
    }
    providers
}

fn env_name(id: &str) -> String {
    format!("BIFROST_VK_{}", id.to_ascii_uppercase().replace('-', "_"))
}

/// Writes `bifrost/config.json` (no secrets: values are `env.` references) and
/// `bifrost/bifrost.env` (admin login and one virtual key per API-key agent,
/// kept across runs). Returns agent id → virtual key.
fn write_bifrost(
    team: &Team,
    config: &Config,
    defined: &[String],
) -> Result<BTreeMap<String, String>> {
    let dir = team.dir.join("bifrost");
    let env_file = dir.join("bifrost.env");
    let old: BTreeMap<String, String> = fs::read_to_string(&env_file)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let keep_or_new = |name: &str, prefix: &str| -> Result<String> {
        match old.get(name) {
            Some(value) if value.starts_with(prefix) => Ok(value.clone()),
            _ => Ok(format!("{prefix}{}", random_hex()?)),
        }
    };

    let mut env = BTreeMap::new();
    env.insert("BIFROST_ADMIN_USER".to_string(), "admin".to_string());
    env.insert(
        "BIFROST_ADMIN_PASSWORD".to_string(),
        keep_or_new("BIFROST_ADMIN_PASSWORD", "")?,
    );
    let mut virtual_keys = BTreeMap::new();
    let mut vk_config = Vec::new();
    for agent in config
        .agents
        .iter()
        .filter(|a| a.authentication == Authentication::ApiKey)
    {
        let name = env_name(&agent.id);
        let key = keep_or_new(&name, "sk-bf-")?;
        vk_config.push(json!({
            "id": format!("rin-{}", agent.id),
            "name": agent.id,
            "value": format!("env.{name}"),
            "is_active": true,
            "allow_all_providers": true,
        }));
        env.insert(name, key.clone());
        virtual_keys.insert(agent.id.clone(), key);
    }

    let provider_config = bifrost_providers(config, defined);
    let bifrost_config = json!({
        "$schema": "https://www.getbifrost.ai/schema",
        "client": { "enforce_auth_on_inference": true },
        "providers": provider_config,
        "governance": {
            "auth_config": {
                "is_enabled": true,
                "admin_username": "env.BIFROST_ADMIN_USER",
                "admin_password": "env.BIFROST_ADMIN_PASSWORD",
            },
            "virtual_keys": vk_config,
        },
    });
    write_shared(
        &dir.join("config.json"),
        format!("{}\n", serde_json::to_string_pretty(&bifrost_config)?).as_bytes(),
    )?;
    let text: String = env.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
    write_private(&env_file, text.as_bytes())?;
    crate::files::set_mode(&dir, 0o700)?;
    Ok(virtual_keys)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const EXAMPLE: &str = include_str!("../../examples/team/rin.yml");

    fn example() -> String {
        let pinned = config::from_yaml(EXAMPLE).unwrap().version;
        EXAMPLE.replacen(
            &format!("version: {pinned}"),
            &format!("version: {VERSION}"),
            1,
        )
    }

    /// A project directory (copy of examples/team, edited) and an empty `$RIN_HOME`.
    struct Fixture {
        root: PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Fixture {
            let root =
                std::env::temp_dir().join(format!("rin-generate-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            for id in ["rin", "backend", "frontend", "devops"] {
                let dir = root.join("project/agents").join(id);
                fs::create_dir_all(&dir).unwrap();
                fs::write(dir.join("INSTRUCTIONS.md"), format!("# {id}\n")).unwrap();
            }
            fs::write(root.join("project/rin.yml"), example()).unwrap();
            fs::create_dir_all(root.join("home")).unwrap();
            fs::write(
                root.join("home/keys.env"),
                "# mine\nANTHROPIC_API_KEY=sk-ant-secret\nOPENAI_API_KEY=\n",
            )
            .unwrap();
            Fixture { root }
        }

        fn home(&self) -> PathBuf {
            self.root.join("home")
        }

        fn team(&self) -> PathBuf {
            self.home().join("teams/my-team")
        }

        fn edit(&self, from: &str, to: &str) {
            let path = self.root.join("project/rin.yml");
            let text = fs::read_to_string(&path).unwrap();
            assert!(text.contains(from), "no {from:?}");
            fs::write(path, text.replacen(from, to, 1)).unwrap();
        }

        fn generate(&self) -> Result<Generated> {
            generate(
                &self.root.join("project/rin.yml"),
                &self.root.join("project/agents"),
                &self.home(),
                "v0.1.0",
            )
        }

        fn read(&self, path: &str) -> String {
            fs::read_to_string(self.team().join(path)).unwrap()
        }

        /// Every file in the team directory → its bytes.
        fn snapshot(&self) -> BTreeMap<PathBuf, Vec<u8>> {
            fn walk(dir: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
                for entry in fs::read_dir(dir).unwrap().flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        walk(&path, out)
                    } else {
                        out.insert(path.clone(), fs::read(&path).unwrap());
                    }
                }
            }
            let mut out = BTreeMap::new();
            walk(&self.team(), &mut out);
            out
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn writes_the_team_directory() {
        let f = Fixture::new("c1");
        let generated = f.generate().unwrap();
        assert!(generated.warnings.is_empty(), "{:?}", generated.warnings);
        for path in [
            "source",
            "docker-compose.yml",
            "rin.json",
            "images/Dockerfile",
            "tokens.json",
            "instructions/rin/INSTRUCTIONS.md",
            "instructions/backend/INSTRUCTIONS.md",
            "instructions/frontend/INSTRUCTIONS.md",
            "instructions/devops/INSTRUCTIONS.md",
            "env/rin.env",
            "env/backend.env",
            "env/frontend.env",
            "env/devops.env",
            "bifrost/config.json",
            "bifrost/bifrost.env",
        ] {
            assert!(f.team().join(path).is_file(), "missing {path}");
        }
        assert_eq!(
            fs::read_to_string(f.home().join("current")).unwrap(),
            "my-team\n"
        );
        assert!(f.read("source").trim_end().ends_with("project/rin.yml"));
        assert_eq!(
            f.read("instructions/backend/INSTRUCTIONS.md"),
            "# backend\n"
        );

        let rin_json: serde_json::Value = serde_json::from_str(&f.read("rin.json")).unwrap();
        assert_eq!(
            rin_json["messaging"],
            json!({"timeout": "30m", "maxHops": 5})
        );

        // Secrets are yours only.
        use std::os::unix::fs::PermissionsExt;
        for path in ["tokens.json", "env/devops.env", "bifrost/bifrost.env"] {
            let mode = fs::metadata(f.team().join(path))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600, "{path}");
        }
    }

    #[test]
    fn files_read_inside_containers_are_readable_by_their_users() {
        // With a umask like 077 these would be yours only, and bind mounts keep
        // that: the agents (UID 1000) and Bifrost could not read them.
        let f = Fixture::new("shared");
        f.generate().unwrap();
        use std::os::unix::fs::PermissionsExt;
        let shared = [
            "instructions/backend/INSTRUCTIONS.md",
            "bifrost/config.json",
        ];
        for path in shared {
            crate::files::set_mode(&f.team().join(path), 0o600).unwrap();
        }
        f.generate().unwrap();
        for path in shared {
            let mode = fs::metadata(f.team().join(path))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o644, "{path}");
        }
    }

    #[test]
    fn generating_again_changes_nothing() {
        let f = Fixture::new("c3");
        f.generate().unwrap();
        let before = f.snapshot();
        f.generate().unwrap();
        assert_eq!(before, f.snapshot());
    }

    #[test]
    fn deleting_tokens_rotates_them_all() {
        let f = Fixture::new("c4");
        f.generate().unwrap();
        let old: Tokens = serde_json::from_str(&f.read("tokens.json")).unwrap();
        let old_vk = f.read("bifrost/bifrost.env");
        fs::remove_file(f.team().join("tokens.json")).unwrap();
        f.generate().unwrap();
        let new: Tokens = serde_json::from_str(&f.read("tokens.json")).unwrap();
        assert_eq!(
            old.keys().collect::<Vec<_>>(),
            ["backend", "devops", "frontend", "rin", "user"]
        );
        assert_eq!(
            old.keys().collect::<Vec<_>>(),
            new.keys().collect::<Vec<_>>()
        );
        for (id, token) in &old {
            assert_ne!(token, &new[id], "{id}");
            assert!(id == "user" || f.read(&format!("env/{id}.env")).contains(&new[id]));
        }
        // Bifrost's virtual keys are not tokens: they stay.
        assert_eq!(old_vk, f.read("bifrost/bifrost.env"));
    }

    #[test]
    fn the_hub_is_recreated_when_its_files_change() {
        // The hub reads rin.json and tokens.json only at start; Compose
        // recreates it on `rin up` only if its service definition changes.
        let f = Fixture::new("hub-files");
        let label = |f: &Fixture| {
            let compose = f.read("docker-compose.yml");
            compose
                .lines()
                .find(|l| l.contains("rin.files:"))
                .unwrap()
                .to_string()
        };
        f.generate().unwrap();
        let first = label(&f);
        f.generate().unwrap();
        assert_eq!(first, label(&f));
        f.edit("maxHops: 5", "maxHops: 2");
        f.generate().unwrap();
        let second = label(&f);
        assert_ne!(first, second);
        fs::remove_file(f.team().join("tokens.json")).unwrap();
        f.generate().unwrap();
        assert_ne!(second, label(&f));
    }

    #[test]
    fn refuses_names_another_team_uses() {
        let f = Fixture::new("names");
        f.generate().unwrap();
        // Team "my" with agent "team-backend" would be container rin-my-team-backend.
        f.edit("name: my-team", "name: my");
        f.edit("id: backend", "id: team-backend");
        let agents = f.root.join("project/agents");
        fs::rename(agents.join("backend"), agents.join("team-backend")).unwrap();
        let error = f.generate().unwrap_err().to_string();
        assert!(
            error.contains(r#"name "rin-my-team-backend", which team "my-team" already uses"#),
            "{error}"
        );
        assert!(!f.home().join("teams/my").exists());
    }

    #[test]
    fn compose_file_holds_no_secrets() {
        let f = Fixture::new("c5");
        f.generate().unwrap();
        let compose = f.read("docker-compose.yml");
        let tokens: Tokens = serde_json::from_str(&f.read("tokens.json")).unwrap();
        let bifrost_env = f.read("bifrost/bifrost.env");
        let secrets = tokens.values().cloned().chain(
            bifrost_env
                .lines()
                .filter_map(|l| l.split_once('='))
                .map(|(_, v)| v.to_string()),
        );
        for secret in secrets.filter(|s| s != "admin") {
            assert!(!compose.contains(&secret));
            assert!(!f.read("bifrost/config.json").contains(&secret));
        }
        assert!(
            !compose.contains("sk-ant-secret")
                && !f.read("bifrost/config.json").contains("sk-ant-secret")
        );
        for id in ["rin", "backend", "frontend", "devops"] {
            let env = f.read(&format!("env/{id}.env"));
            let rin_tokens: Vec<&str> = env
                .lines()
                .filter_map(|l| l.strip_prefix("RIN_TOKEN="))
                .collect();
            assert_eq!(rin_tokens, [tokens[id].as_str()], "{id}");
            for (other, token) in &tokens {
                if other != id {
                    assert!(!env.contains(token.as_str()), "{id} has {other}'s token");
                }
            }
        }
        // Only the API-key agent gets a virtual key; it is in Bifrost's env, as a reference in config.json.
        assert!(
            f.read("env/devops.env")
                .contains("ANTHROPIC_API_KEY=sk-bf-")
        );
        assert!(!f.read("env/backend.env").contains("sk-bf-"));
        assert!(!f.read("env/frontend.env").contains("sk-bf-"));
        let config: serde_json::Value =
            serde_json::from_str(&f.read("bifrost/config.json")).unwrap();
        assert_eq!(
            config["governance"]["virtual_keys"][0]["value"],
            "env.BIFROST_VK_DEVOPS"
        );
        assert_eq!(
            config["providers"]
                .as_object()
                .unwrap()
                .keys()
                .collect::<Vec<_>>(),
            ["anthropic"]
        );
    }

    #[test]
    fn compose_file_shape() {
        let f = Fixture::new("compose");
        f.generate().unwrap();
        let compose = f.read("docker-compose.yml");
        for line in [
            "name: rin-my-team\n",
            "      - \"127.0.0.1:10302:7000\"\n",
            "      - rin-my-team-backend-home:/home/agent\n",
            "      - ./instructions/rin/INSTRUCTIONS.md:/home/agent/.claude/CLAUDE.md:ro\n",
            "      - ./instructions/backend/INSTRUCTIONS.md:/home/agent/.codex/AGENTS.md:ro\n",
            "      - ./instructions/frontend/INSTRUCTIONS.md:/home/agent/.gemini/GEMINI.md:ro\n",
            "      - ./instructions/devops/INSTRUCTIONS.md:/home/agent/.pi/agent/AGENTS.md:ro\n",
            "      - ./tokens.json:/etc/rin/tokens.json:ro\n",
            "    hostname: backend\n",
            "    cpus: 1\n",
            "    mem_limit: \"2g\"\n",
            "        RIN_GIT_REV: v0.1.0\n",
            "  rin-my-team-hub:\n    name: rin-my-team-hub\n",
            "  bifrost:\n",
            // A missing keys.env must not stop Compose from loading the project.
            "      - { path: ../../keys.env, required: false }\n",
        ] {
            assert!(compose.contains(line), "no {line:?} in\n{compose}");
        }
        assert_eq!(
            compose.matches("ports:").count(),
            1,
            "only the hub publishes a port"
        );
        assert_eq!(
            compose.matches("./tokens.json:").count(),
            1,
            "only the hub mounts tokens.json"
        );
        assert!(!compose.contains("docker.sock"));
    }

    #[test]
    fn leaves_an_existing_rin_home_mode_alone() {
        let f = Fixture::new("home-mode");
        use std::os::unix::fs::PermissionsExt;
        crate::files::set_mode(&f.home(), 0o755).unwrap();
        f.generate().unwrap();
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&f.home()), 0o755);
        assert_eq!(mode(&f.home().join("teams")), 0o700);
        // The fixture wrote keys.env with the default umask.
        assert_eq!(mode(&f.home().join("keys.env")), 0o600);
    }

    #[test]
    fn no_bifrost_without_api_keys() {
        let f = Fixture::new("c6");
        f.edit(
            "harness: pi\n    authentication: apiKey",
            "harness: claude-code\n    authentication: subscription",
        );
        f.generate().unwrap();
        let compose = f.read("docker-compose.yml");
        assert!(!compose.contains("bifrost"), "{compose}");
        assert!(!f.read("env/devops.env").contains("sk-bf-"));
    }

    #[test]
    fn instructions_must_match_agents() {
        let f = Fixture::new("instructions");
        fs::create_dir_all(f.root.join("project/agents/qa")).unwrap();
        let e = format!("{:#}", f.generate().unwrap_err());
        assert!(e.contains("agents/qa: no agent with id \"qa\""), "{e}");
        fs::remove_dir_all(f.root.join("project/agents/qa")).unwrap();

        fs::remove_file(f.root.join("project/agents/frontend/INSTRUCTIONS.md")).unwrap();
        let e = format!("{:#}", f.generate().unwrap_err());
        assert!(
            e.contains("agents/frontend: INSTRUCTIONS.md is missing"),
            "{e}"
        );

        fs::remove_dir_all(f.root.join("project/agents/frontend")).unwrap();
        let e = format!("{:#}", f.generate().unwrap_err());
        assert!(e.contains("agent \"frontend\" has no directory"), "{e}");
        assert!(!f.team().exists(), "nothing written for a rejected config");
    }

    #[test]
    fn refuses_another_version() {
        let f = Fixture::new("version");
        f.edit(&format!("version: {VERSION}"), "version: 0.0.9");
        let e = format!("{:#}", f.generate().unwrap_err());
        assert!(e.contains("0.0.9") && e.contains(VERSION), "{e}");
    }

    #[test]
    fn warns_without_provider_keys() {
        let f = Fixture::new("nokeys");
        fs::remove_file(f.home().join("keys.env")).unwrap();
        let generated = f.generate().unwrap();
        assert_eq!(generated.warnings.len(), 1);
    }

    const PROVIDERS: &str = "
llmProviders:
  - id: groq
    type: openai
    baseUrl: https://api.groq.com/openai/v1/
    apiKeyRequired: true
    apiKeyEnv: GROQ_API_KEY
  - id: local
    type: openai
    baseUrl: http://host.docker.internal:8000/v1
    apiKeyRequired: false
";

    /// The fixture with custom providers, devops on `model` and `keys` as keys.env.
    fn with_providers(name: &str, model: &str, keys: &str) -> Fixture {
        let f = Fixture::new(name);
        f.edit(
            "model: anthropic/claude-sonnet-5-5",
            &format!("model: {model}"),
        );
        let path = f.root.join("project/rin.yml");
        let text = fs::read_to_string(&path).unwrap();
        fs::write(&path, text + PROVIDERS).unwrap();
        fs::write(f.home().join("keys.env"), keys).unwrap();
        f
    }

    fn bifrost_providers(f: &Fixture) -> serde_json::Value {
        let config: serde_json::Value =
            serde_json::from_str(&f.read("bifrost/config.json")).unwrap();
        config["providers"].clone()
    }

    #[test]
    fn bifrost_gets_ollama_cloud_and_custom_providers() {
        let f = with_providers(
            "custom",
            "groq/llama-4",
            "GROQ_API_KEY=gsk-secret\nOLLAMA_API_KEY=ollama-secret\n",
        );
        let generated = f.generate().unwrap();
        assert!(generated.warnings.is_empty(), "{:?}", generated.warnings);
        let providers = bifrost_providers(&f);
        assert_eq!(
            providers.as_object().unwrap().keys().collect::<Vec<_>>(),
            ["ollama", "rin-groq", "rin-local"]
        );
        assert_eq!(
            providers["ollama"]["keys"][0],
            json!({ "name": "ollama", "value": "env.OLLAMA_API_KEY", "models": ["*"], "weight": 1.0,
                    "ollama_key_config": { "url": "https://ollama.com" } })
        );
        let groq = &providers["rin-groq"];
        assert_eq!(groq["keys"][0]["value"], "env.GROQ_API_KEY");
        assert_eq!(
            groq["network_config"]["base_url"],
            "https://api.groq.com/openai/v1"
        );
        assert_eq!(
            groq["custom_provider_config"]["base_provider_type"],
            "openai"
        );
        assert_eq!(groq["custom_provider_config"]["is_key_less"], false);
        assert_eq!(
            groq["custom_provider_config"]["request_path_overrides"]["chat_completion"],
            "/chat/completions"
        );
        assert_eq!(
            groq["custom_provider_config"]["allowed_requests"],
            json!({ "chat_completion": true, "chat_completion_stream": true })
        );
        let local = &providers["rin-local"];
        assert_eq!(local["keys"], json!([]));
        assert_eq!(local["custom_provider_config"]["is_key_less"], true);
        assert_eq!(local["network_config"]["allow_private_network"], true);
        for secret in ["gsk-secret", "ollama-secret"] {
            assert!(!f.read("bifrost/config.json").contains(secret));
            assert!(!f.read("docker-compose.yml").contains(secret));
        }
        assert!(
            f.read("docker-compose.yml")
                .contains("    extra_hosts:\n      - host.docker.internal:host-gateway\n")
        );
    }

    #[test]
    fn no_extra_hosts_without_custom_providers() {
        let f = Fixture::new("no-extra-hosts");
        f.generate().unwrap();
        assert!(!f.read("docker-compose.yml").contains("extra_hosts"));
    }

    #[test]
    fn keyless_providers_need_no_keys_env() {
        let f = with_providers("keyless", "local/qwen3", "");
        fs::remove_file(f.home().join("keys.env")).unwrap();
        let generated = f.generate().unwrap();
        assert!(generated.warnings.is_empty(), "{:?}", generated.warnings);
        assert!(f.read("env/devops.env").contains("OPENAI_API_KEY=sk-bf-"));
        assert_eq!(
            bifrost_providers(&f)
                .as_object()
                .unwrap()
                .keys()
                .collect::<Vec<_>>(),
            ["rin-local"]
        );
    }

    #[test]
    fn missing_keys_names_what_agents_need() {
        let f = with_providers("missing", "groq/llama-4", "OLLAMA_API_KEY=x\n");
        let generated = f.generate().unwrap();
        let keys_env = f.home().join("keys.env");
        let config =
            config::from_yaml(&fs::read_to_string(f.root.join("project/rin.yml")).unwrap())
                .unwrap();
        let problem = missing_keys(&keys_env, &config).unwrap();
        assert!(
            problem.ends_with("keys.env does not set GROQ_API_KEY"),
            "{problem}"
        );
        assert_eq!(generated.warnings.len(), 1);
        assert!(
            generated.warnings[0].starts_with(&problem),
            "{:?}",
            generated.warnings
        );
        // A provider without its key is left out of Bifrost.
        assert!(bifrost_providers(&f).get("rin-groq").is_none());

        // Subscription agents need no key; the API-key one names its provider's.
        fs::remove_file(&keys_env).unwrap();
        let problem = missing_keys(&keys_env, &config).unwrap();
        assert!(
            problem.ends_with("keys.env does not exist; your API-key agents need GROQ_API_KEY"),
            "{problem}"
        );
        fs::write(&keys_env, "GROQ_API_KEY=\"\"\n").unwrap();
        assert!(missing_keys(&keys_env, &config).is_some());
        fs::write(&keys_env, "export GROQ_API_KEY=gsk\n").unwrap();
        assert_eq!(missing_keys(&keys_env, &config), None);

        // Every harness's key, each named once.
        let mut config = config;
        for agent in &mut config.agents {
            agent.authentication = Authentication::ApiKey;
        }
        config.agents[3].model = "ollama/qwen3".into();
        config.agents.push(config.agents[0].clone());
        let problem = missing_keys(&keys_env, &config).unwrap();
        assert!(
            problem.ends_with(
                "does not set ANTHROPIC_API_KEY, GEMINI_API_KEY, OLLAMA_API_KEY, OPENAI_API_KEY"
            ),
            "{problem}"
        );
    }

    #[test]
    fn moving_an_agent_out_removes_its_files() {
        let f = Fixture::new("remove");
        f.generate().unwrap();
        let text = fs::read_to_string(f.root.join("project/rin.yml")).unwrap();
        // Cuts frontend (subscription) and devops (API key), the last two agents.
        let cut = text.find("  - id: frontend").unwrap();
        fs::write(f.root.join("project/rin.yml"), &text[..cut]).unwrap();
        for id in ["frontend", "devops"] {
            fs::remove_dir_all(f.root.join("project/agents").join(id)).unwrap();
        }
        f.generate().unwrap();
        let tokens: Tokens = serde_json::from_str(&f.read("tokens.json")).unwrap();
        for id in ["frontend", "devops"] {
            assert!(!f.team().join(format!("env/{id}.env")).exists(), "{id}");
            assert!(!f.team().join("instructions").join(id).exists(), "{id}");
            assert!(!tokens.contains_key(id), "{id}");
        }
    }
}
