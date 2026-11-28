//! On every start, `rin runtime` writes three things into the harness's own
//! config in the home volume: Rin's MCP server, its tool timeout, and the
//! default model. Only those keys are touched; `rin.yml` wins on each start.
//! Codex's `config.toml` is written out again as a whole when one of them
//! changes: your other keys keep their values, but not comments or layout.

use crate::rin_bail;
use crate::rinerrln;
use crate::shared::config::{Harness, custom_provider_name, split_model};
use anyhow::{Context, Result};
use serde_json::{Map, Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub struct AgentSettings<'a> {
    pub harness: Harness,
    pub model: &'a str,
    /// The team's `messaging.timeout`.
    pub timeout: Duration,
    /// Bifrost's URL, for API-key agents.
    pub bifrost_url: Option<&'a str>,
}

/// How long a harness lets the `rin` MCP tool run: a little above the team's
/// timeout, so a slow reply ends with Rin's own `timeout` error.
pub fn tool_timeout(timeout: Duration) -> Duration {
    timeout + Duration::from_secs(60)
}

/// What Codex must pass to `rin mcp` (it strips the rest of its environment);
/// `RIN_SESSION` lets the runtime kill the server with its session.
const MCP_ENV: [&str; 4] = ["RIN_TOKEN", "RIN_HUB_URL", "RIN_HOPS", "RIN_SESSION"];

pub fn apply(home: &Path, settings: &AgentSettings) -> Result<Vec<PathBuf>> {
    let tool_timeout = tool_timeout(settings.timeout);
    let ms = tool_timeout.as_millis() as u64;
    let secs = tool_timeout.as_secs();
    let mut written = Vec::new();
    let mut json = |path: PathBuf, edit: &dyn Fn(&mut Map<String, Value>)| -> Result<()> {
        if merge_json(&path, edit)? {
            written.push(path);
        }
        Ok(())
    };

    match settings.harness {
        Harness::ClaudeCode => {
            json(home.join(".claude/settings.json"), &|s| {
                s.insert("model".into(), json!(settings.model));
                object(s, "env").insert("MCP_TOOL_TIMEOUT".into(), json!(ms.to_string()));
            })?;
            json(home.join(".claude.json"), &|s| {
                object(s, "mcpServers").insert(
                    "rin".into(),
                    json!({ "type": "stdio", "command": "rin", "args": ["mcp"], "env": {}, "timeout": ms }),
                );
            })?;
        }
        Harness::Codex => {
            let path = home.join(".codex/config.toml");
            if merge_codex(&path, settings, secs)? {
                written.push(path);
            }
        }
        Harness::Pi => {
            let provider = split_model(settings.model).map(|(provider, _)| provider);
            let pi_model = PiModel::of(settings.model);
            let extra_provider = (!pi_model.builtin)
                .then_some(pi_model.provider.as_deref())
                .flatten();
            let model_id = &pi_model.id;
            json(home.join(".pi/agent/settings.json"), &|s| {
                s.insert("defaultModel".into(), json!(model_id));
                if let Some(provider) = &pi_model.provider {
                    s.insert("defaultProvider".into(), json!(provider));
                }
            })?;
            if let Some(bifrost_url) = settings.bifrost_url {
                let route = match provider {
                    Some("anthropic") => Some(("anthropic", format!("{bifrost_url}/anthropic"))),
                    Some("openai") => Some(("openai", format!("{bifrost_url}/openai/v1"))),
                    Some("google") => Some(("google", format!("{bifrost_url}/genai"))),
                    _ => None,
                };
                match (extra_provider, route) {
                    (Some(name), _) => json(home.join(".pi/agent/models.json"), &|s| {
                        let entry = object(object(s, "providers"), name);
                        entry.insert("baseUrl".into(), json!(format!("{bifrost_url}/openai/v1")));
                        entry.insert("api".into(), json!("openai-completions"));
                        // The name of the variable holding the agent's virtual key.
                        entry.insert("apiKey".into(), json!("$OPENAI_API_KEY"));
                        let models = array(entry, "models");
                        if !models.iter().any(|m| m.get("id") == Some(&json!(model_id))) {
                            models.push(json!({ "id": model_id }));
                        }
                    })?,
                    (None, Some((provider, url))) => {
                        json(home.join(".pi/agent/models.json"), &|s| {
                            object(object(s, "providers"), provider)
                                .insert("baseUrl".into(), json!(url));
                        })?
                    }
                    // Without a route pi would send its Bifrost key straight to the provider.
                    (None, None) => rinerrln!(
                        "runtime: pi model {:?} can't be routed through Bifrost: write it as <provider>/<model>",
                        settings.model
                    ),
                }
            }
        }
        Harness::Antigravity => {
            json(home.join(".gemini/config/mcp_config.json"), &|s| {
                object(s, "mcpServers").insert(
                    "rin".into(),
                    json!({ "command": "rin", "args": ["mcp"], "env": {}, "timeoutSeconds": secs }),
                );
            })?;
            json(home.join(".gemini/antigravity-cli/settings.json"), &|s| {
                s.insert("model".into(), json!(settings.model));
                if settings.bifrost_url.is_some() {
                    s.insert("modelProvider".into(), json!("gemini"));
                } else if s.get("modelProvider") == Some(&json!("gemini")) {
                    // Switched back to a subscription: stop using the API key.
                    s.remove("modelProvider");
                }
            })?;
        }
    }
    Ok(written)
}

/// A `rin.yml` pi model as pi knows it.
pub struct PiModel {
    pub provider: Option<String>,
    pub id: String,
    /// A provider Rin adds to pi (Ollama or a custom one), rather than one of pi's own.
    pub builtin: bool,
}

impl PiModel {
    /// pi's own providers keep their models and only get Bifrost's URL. Ollama
    /// and custom providers become pi providers of their own, speaking chat
    /// completions to Bifrost, which wants `<its name>/<model>` as the model id.
    pub fn of(model: &str) -> PiModel {
        let Some((provider, name)) = split_model(model) else {
            return PiModel {
                provider: None,
                id: model.to_string(),
                builtin: true,
            };
        };
        let added = match provider {
            "anthropic" | "openai" | "google" => None,
            "ollama" => Some("ollama".to_string()),
            id => Some(custom_provider_name(id)),
        };
        match added {
            Some(pi_provider) => PiModel {
                id: format!("{pi_provider}/{name}"),
                provider: Some(pi_provider),
                builtin: false,
            },
            None => PiModel {
                provider: Some(provider.to_string()),
                id: name.to_string(),
                builtin: true,
            },
        }
    }

    /// What `pi --model` takes: `<provider>/<id>`, so an id with a `/` in it
    /// still names this provider's model.
    pub fn cli(&self) -> String {
        match &self.provider {
            Some(provider) => format!("{provider}/{}", self.id),
            None => self.id.clone(),
        }
    }
}

/// The object at `key`, made one if missing or not an object.
fn object<'a>(map: &'a mut Map<String, Value>, key: &str) -> &'a mut Map<String, Value> {
    let slot = map.entry(key.to_string()).or_insert_with(|| json!({}));
    if !slot.is_object() {
        *slot = json!({});
    }
    slot.as_object_mut().expect("just made an object")
}

/// The array at `key`, made one if missing or not an array.
fn array<'a>(map: &'a mut Map<String, Value>, key: &str) -> &'a mut Vec<Value> {
    let slot = map.entry(key.to_string()).or_insert_with(|| json!([]));
    if !slot.is_array() {
        *slot = json!([]);
    }
    slot.as_array_mut().expect("just made an array")
}

/// Applies `edit` to the JSON object in `path` (created if missing); writes
/// only when something changed. Returns whether it wrote.
fn merge_json(path: &Path, edit: &dyn Fn(&mut Map<String, Value>)) -> Result<bool> {
    let text = read_or_empty(path)?;
    let original: Value = if text.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(&text)
            .with_context(|| format!("{} is not valid JSON; fix or delete it", path.display()))?
    };
    let Value::Object(mut map) = original.clone() else {
        rin_bail!("{} is not a JSON object; fix or delete it", path.display());
    };
    edit(&mut map);
    let updated = Value::Object(map);
    if updated == original {
        return Ok(false);
    }
    write_atomic(
        path,
        format!("{}\n", serde_json::to_string_pretty(&updated)?).as_bytes(),
    )?;
    Ok(true)
}

fn merge_codex(path: &Path, settings: &AgentSettings, tool_timeout_secs: u64) -> Result<bool> {
    let text = read_or_empty(path)?;
    let original: toml::Table = toml::from_str(&text)
        .with_context(|| format!("{} is not valid TOML; fix or delete it", path.display()))?;
    let mut table = original.clone();
    table.insert("model".into(), toml::Value::String(settings.model.into()));

    let mut rin = toml::Table::new();
    rin.insert("command".into(), "rin".into());
    rin.insert("args".into(), toml::Value::Array(vec!["mcp".into()]));
    rin.insert(
        "env_vars".into(),
        toml::Value::Array(MCP_ENV.iter().map(|v| (*v).into()).collect()),
    );
    rin.insert(
        "tool_timeout_sec".into(),
        toml::Value::Integer(tool_timeout_secs as i64),
    );
    toml_table(&mut table, "mcp_servers").insert("rin".into(), toml::Value::Table(rin));

    match settings.bifrost_url {
        Some(bifrost) => {
            table.insert("model_provider".into(), "bifrost".into());
            let mut provider = toml::Table::new();
            provider.insert("name".into(), "Bifrost".into());
            provider.insert("base_url".into(), format!("{bifrost}/openai/v1").into());
            provider.insert("env_key".into(), "OPENAI_API_KEY".into());
            provider.insert("wire_api".into(), "responses".into());
            toml_table(&mut table, "model_providers")
                .insert("bifrost".into(), toml::Value::Table(provider));
        }
        None => {
            // Switched back to a subscription: stop routing through Bifrost.
            if table.get("model_provider").and_then(toml::Value::as_str) == Some("bifrost") {
                table.remove("model_provider");
            }
        }
    }
    if table == original {
        return Ok(false);
    }
    write_atomic(path, toml::to_string(&table)?.as_bytes())?;
    Ok(true)
}

fn toml_table<'a>(table: &'a mut toml::Table, key: &str) -> &'a mut toml::Table {
    let slot = table
        .entry(key.to_string())
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    if !slot.is_table() {
        *slot = toml::Value::Table(toml::Table::new());
    }
    slot.as_table_mut().expect("just made a table")
}

fn read_or_empty(path: &Path) -> Result<String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e).with_context(|| format!("cannot read {}", path.display())),
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    let tmp = path.with_extension("rin-tmp");
    fs::write(&tmp, bytes).with_context(|| format!("cannot write {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("cannot write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_home(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rin-settings-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn read_json(path: PathBuf) -> Value {
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
    }

    fn settings(
        harness: Harness,
        model: &str,
        bifrost: Option<&'static str>,
    ) -> AgentSettings<'static> {
        AgentSettings {
            harness,
            model: Box::leak(model.to_string().into_boxed_str()),
            timeout: Duration::from_secs(1800),
            bifrost_url: bifrost,
        }
    }

    #[test]
    fn claude_code_keeps_your_keys_and_resets_rins() {
        let home = temp_home("claude");
        let s = settings(Harness::ClaudeCode, "claude-opus-5-5", None);
        apply(&home, &s).unwrap();
        let config = read_json(home.join(".claude/settings.json"));
        assert_eq!(config["model"], "claude-opus-5-5");
        assert_eq!(config["env"]["MCP_TOOL_TIMEOUT"], "1860000");
        let state = read_json(home.join(".claude.json"));
        assert_eq!(state["mcpServers"]["rin"]["command"], "rin");
        assert_eq!(state["mcpServers"]["rin"]["timeout"], 1_860_000);

        // L9: you add a key and change the model; a restart keeps yours and resets Rin's.
        fs::write(
            home.join(".claude/settings.json"),
            r#"{"foo": 1, "model": "other", "env": {"X": "y"}}"#,
        )
        .unwrap();
        apply(&home, &s).unwrap();
        let config = read_json(home.join(".claude/settings.json"));
        assert_eq!(config["foo"], 1);
        assert_eq!(config["env"]["X"], "y");
        assert_eq!(config["model"], "claude-opus-5-5");

        // Nothing to change: nothing written.
        assert!(apply(&home, &s).unwrap().is_empty());
    }

    #[test]
    fn broken_files_are_reported_not_overwritten() {
        let home = temp_home("broken");
        fs::create_dir_all(home.join(".claude")).unwrap();
        fs::write(home.join(".claude/settings.json"), "{ nope").unwrap();
        let err = apply(&home, &settings(Harness::ClaudeCode, "m", None)).unwrap_err();
        assert!(err.to_string().contains("not valid JSON"), "{err}");
        assert_eq!(
            fs::read_to_string(home.join(".claude/settings.json")).unwrap(),
            "{ nope"
        );
    }

    #[test]
    fn codex_gets_model_mcp_and_gateway() {
        let home = temp_home("codex");
        fs::create_dir_all(home.join(".codex")).unwrap();
        fs::write(
            home.join(".codex/config.toml"),
            "approval_policy = \"never\"\nmodel = \"x\"\n",
        )
        .unwrap();
        apply(
            &home,
            &settings(Harness::Codex, "gpt-5-codex", Some("http://bifrost:8080")),
        )
        .unwrap();
        let config: toml::Table =
            toml::from_str(&fs::read_to_string(home.join(".codex/config.toml")).unwrap()).unwrap();
        assert_eq!(config["approval_policy"].as_str(), Some("never"));
        assert_eq!(config["model"].as_str(), Some("gpt-5-codex"));
        let rin = &config["mcp_servers"]["rin"];
        assert_eq!(rin["command"].as_str(), Some("rin"));
        assert_eq!(rin["tool_timeout_sec"].as_integer(), Some(1860));
        assert_eq!(rin["env_vars"].as_array().unwrap().len(), 4);
        assert_eq!(config["model_provider"].as_str(), Some("bifrost"));
        assert_eq!(
            config["model_providers"]["bifrost"]["base_url"].as_str(),
            Some("http://bifrost:8080/openai/v1")
        );

        apply(&home, &settings(Harness::Codex, "gpt-5-codex", None)).unwrap();
        let config: toml::Table =
            toml::from_str(&fs::read_to_string(home.join(".codex/config.toml")).unwrap()).unwrap();
        assert!(config.get("model_provider").is_none());
    }

    #[test]
    fn pi_splits_provider_and_routes_it() {
        let home = temp_home("pi");
        apply(
            &home,
            &settings(
                Harness::Pi,
                "anthropic/claude-sonnet-5-5",
                Some("http://bifrost:8080"),
            ),
        )
        .unwrap();
        let config = read_json(home.join(".pi/agent/settings.json"));
        assert_eq!(config["defaultProvider"], "anthropic");
        assert_eq!(config["defaultModel"], "claude-sonnet-5-5");
        let models = read_json(home.join(".pi/agent/models.json"));
        assert_eq!(
            models["providers"]["anthropic"]["baseUrl"],
            "http://bifrost:8080/anthropic"
        );
        // pi's own providers' models are named as written.
        let cli = PiModel::of("anthropic/claude-sonnet-5-5").cli();
        assert_eq!(cli, "anthropic/claude-sonnet-5-5");
        assert_eq!(PiModel::of("bare").cli(), "bare");
    }

    #[test]
    fn pi_adds_ollama_and_custom_providers_on_bifrost() {
        for (model, name) in [
            ("ollama/qwen3-coder:480b", "ollama"),
            ("groq/openai/gpt-oss-120b", "rin-groq"),
        ] {
            let home = temp_home(name);
            let s = settings(Harness::Pi, model, Some("http://bifrost:8080"));
            apply(&home, &s).unwrap();
            let pi_model = format!("{name}/{}", model.split_once('/').unwrap().1);
            let config = read_json(home.join(".pi/agent/settings.json"));
            assert_eq!(config["defaultProvider"], name);
            assert_eq!(config["defaultModel"], pi_model.as_str());
            let models = read_json(home.join(".pi/agent/models.json"));
            let entry = &models["providers"][name];
            assert_eq!(entry["baseUrl"], "http://bifrost:8080/openai/v1");
            assert_eq!(entry["api"], "openai-completions");
            assert_eq!(entry["apiKey"], "$OPENAI_API_KEY");
            assert_eq!(entry["models"], json!([{ "id": pi_model }]));
            assert_eq!(PiModel::of(model).cli(), format!("{name}/{pi_model}"));

            // Models you added are kept, and Rin's is not added twice.
            let mut edited = models.clone();
            edited["providers"][name]["models"] = json!([{ "id": "mine" }, { "id": pi_model }]);
            fs::write(home.join(".pi/agent/models.json"), edited.to_string()).unwrap();
            assert!(apply(&home, &s).unwrap().is_empty());
        }
    }

    #[test]
    fn antigravity_gets_mcp_and_model() {
        let home = temp_home("agy");
        apply(
            &home,
            &settings(
                Harness::Antigravity,
                "gemini-3-pro",
                Some("http://bifrost:8080"),
            ),
        )
        .unwrap();
        let mcp = read_json(home.join(".gemini/config/mcp_config.json"));
        assert_eq!(mcp["mcpServers"]["rin"]["timeoutSeconds"], 1860);
        let config = read_json(home.join(".gemini/antigravity-cli/settings.json"));
        assert_eq!(config["model"], "gemini-3-pro");
        assert_eq!(config["modelProvider"], "gemini");

        apply(&home, &settings(Harness::Antigravity, "gemini-3-pro", None)).unwrap();
        let config = read_json(home.join(".gemini/antigravity-cli/settings.json"));
        assert!(config.get("modelProvider").is_none(), "{config}");
    }
}
