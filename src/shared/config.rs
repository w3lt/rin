//! The team config: `rin.yml` as typed data, checked against the embedded JSON Schema.
//!
//! The same type is written out as `rin.json` (defaults filled in) by `rin generate`
//! and read back by the hub.

#![cfg_attr(
    not(all(feature = "host", feature = "hub", feature = "agent")),
    allow(dead_code)
)]

mod default_config;

use crate::rin_bail;
use default_config::{default_max_hops, default_max_sessions, default_timeout};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// `rin-schema.json`, embedded so every `rin` checks configs the same way.
pub const SCHEMA: &str = include_str!("../../rin-schema.json");

/// The version of this `rin` binary; a team's `version:` must match it exactly.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub const NOT_A_RIN_CONFIG: &str = r#"not a Rin config: "rin" must be "tohsaka""#;

#[derive(Debug, Clone, PartialEq)]
pub struct Tohsaka;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub rin: Tohsaka,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub messaging: Messaging,
    pub agents: Vec<Agent>,
}

impl Serialize for Tohsaka {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str("tohsaka")
    }
}

impl<'de> Deserialize<'de> for Tohsaka {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        match String::deserialize(deserializer)?.as_str() {
            "tohsaka" => Ok(Tohsaka),
            _ => Err(serde::de::Error::custom("expected 'tohsaka'")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Messaging {
    #[serde(default = "default_timeout")]
    pub timeout: String,
    #[serde(default = "default_max_hops")]
    pub max_hops: u32,
}

impl Default for Messaging {
    fn default() -> Self {
        Self {
            timeout: default_timeout(),
            max_hops: default_max_hops(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Agent {
    pub id: String,
    #[serde(rename = "type")]
    pub agent_type: AgentType,
    pub label: String,
    pub harness: Harness,
    pub authentication: Authentication,
    pub model: String,
    #[serde(default = "default_max_sessions")]
    pub max_sessions: u32,
    pub resources: Resources,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resources {
    /// Kept as written (`2` stays `2`, `0.5` stays `0.5`) so it reaches Compose unchanged.
    pub cpus: serde_json::Number,
    pub ram: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentType {
    Leader,
    Member,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Harness {
    ClaudeCode,
    Codex,
    Antigravity,
    Pi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Authentication {
    Subscription,
    ApiKey,
}

impl Agent {
    /// Starter agents for [`Config::init`], each with its `INSTRUCTIONS.md` text.
    fn dummy_leader() -> (Agent, String) {
        (
            Agent {
                id: "rinlead".to_string(),
                agent_type: AgentType::Leader,
                label: "Rin Lead".to_string(),
                harness: Harness::ClaudeCode,
                authentication: Authentication::Subscription,
                model: "claude-opus-5-5".to_string(),
                max_sessions: default_max_sessions(),
                resources: Resources {
                    cpus: serde_json::Number::from(1),
                    ram: "2g".to_string(),
                },
            },
            "# Rin — Chief Architect

You lead this team.
Break work down and delegate to the right teammate with the `send_message` tool; `list_agents` shows who is on the team and how busy they are.
Review what comes back before you report to the user."
                .to_string()
        )
    }

    fn dummy_backend() -> (Agent, String) {
        (
            Agent {
                id: "backend".to_string(),
                agent_type: AgentType::Member,
                label: "Backend Engineer".to_string(),
                harness: Harness::Codex,
                authentication: Authentication::Subscription,
                model: "gpt-6-luna".to_string(),
                max_sessions: default_max_sessions(),
                resources: Resources {
                    cpus: serde_json::Number::from(1),
                    ram: "2g".to_string(),
                },
            },
            "# Backend Engineer

You own APIs, data and services.

To message a teammate, run `rin send <id> \"<text>\"` in the shell: it waits and prints their reply.
`rin agents` lists the team."
                .to_string(),
        )
    }

    fn dummy_frontend() -> (Agent, String) {
        (
            Agent {
                id: "frontend".to_string(),
                agent_type: AgentType::Member,
                label: "Front-end Engineer".to_string(),
                harness: Harness::Antigravity,
                authentication: Authentication::Subscription,
                model: "gemini-3.8-flash".to_string(),
                max_sessions: default_max_sessions(),
                resources: Resources {
                    cpus: serde_json::Number::from(1),
                    ram: "2g".to_string(),
                },
            },
            "# Frontend Engineer

You own the user interface. Use `send_message` to ask teammates for what you
need; `list_agents` shows the team."
                .to_string(),
        )
    }

    fn dummy_devops() -> (Agent, String) {
        (
            Agent {
                id: "devops".to_string(),
                agent_type: AgentType::Member,
                label: "DevOps Engineer".to_string(),
                harness: Harness::Pi,
                authentication: Authentication::ApiKey,
                model: "anthropic/claude-sonnet-5-5".to_string(),
                max_sessions: default_max_sessions(),
                resources: Resources {
                    cpus: serde_json::Number::from(1),
                    ram: "2g".to_string(),
                },
            },
            "# DevOps Engineer

You own infrastructure, CI/CD and deployments.

To message a teammate, run `rin send <id> \"<text>\"` in the shell: it waits and prints their reply.
`rin agents` lists the team."
                .to_string(),
        )
    }
}

impl AgentType {
    pub fn as_str(self) -> &'static str {
        match self {
            AgentType::Leader => "leader",
            AgentType::Member => "member",
        }
    }
}

impl Harness {
    pub const ALL: [Harness; 4] = [
        Harness::ClaudeCode,
        Harness::Codex,
        Harness::Antigravity,
        Harness::Pi,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Harness::ClaudeCode => "claude-code",
            Harness::Codex => "codex",
            Harness::Antigravity => "antigravity",
            Harness::Pi => "pi",
        }
    }

    pub fn parse(s: &str) -> Option<Harness> {
        Harness::ALL.into_iter().find(|h| h.as_str() == s)
    }

    /// Where the agent's `INSTRUCTIONS.md` is mounted (read-only) in its container.
    pub fn instructions_path(self) -> &'static str {
        match self {
            Harness::ClaudeCode => "/home/agent/.claude/CLAUDE.md",
            Harness::Codex => "/home/agent/.codex/AGENTS.md",
            Harness::Pi => "/home/agent/.pi/agent/AGENTS.md",
            Harness::Antigravity => "/home/agent/.gemini/GEMINI.md",
        }
    }
}

impl Messaging {
    pub fn timeout(&self) -> Duration {
        parse_minutes(&self.timeout).unwrap_or(Duration::from_secs(30 * 60))
    }
}

/// The longest `timeout` Rin takes: a year, far beyond any task, and far from
/// overflowing a deadline.
pub const MAX_MINUTES: u64 = 365 * 24 * 60;

/// Parses `30m` style durations (whole minutes, as the schema allows), up to [`MAX_MINUTES`].
pub fn parse_minutes(s: &str) -> Option<Duration> {
    let minutes: u64 = s.strip_suffix('m')?.parse().ok()?;
    (1..=MAX_MINUTES)
        .contains(&minutes)
        .then(|| Duration::from_secs(minutes * 60))
}

/// `45s`, `3m`, `1h20m`.
pub fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    match secs {
        0..60 => format!("{secs}s"),
        60..3600 => format!("{}m", secs / 60),
        _ => format!("{}h{}m", secs / 3600, secs % 3600 / 60),
    }
}

impl Config {
    pub fn init(name: String) -> (Config, Vec<String>) {
        let (leader, leader_instructions) = Agent::dummy_leader();
        let (backend, backend_instruction) = Agent::dummy_backend();
        let (frontend, frontend_instruction) = Agent::dummy_frontend();
        let (devops, devops_instruction) = Agent::dummy_devops();
        (
            Config {
                rin: Tohsaka,
                name,
                version: VERSION.to_string(),
                messaging: Messaging::default(),
                agents: vec![leader, backend, frontend, devops],
            },
            vec![
                leader_instructions,
                backend_instruction,
                frontend_instruction,
                devops_instruction,
            ],
        )
    }

    pub fn agent(&self, id: &str) -> Option<&Agent> {
        self.agents.iter().find(|a| a.id == id)
    }

    pub fn uses_api_keys(&self) -> bool {
        self.agents
            .iter()
            .any(|a| a.authentication == Authentication::ApiKey)
    }

    /// Checks JSON Schema can't express.
    fn check(&self) -> anyhow::Result<()> {
        if parse_minutes(&self.messaging.timeout).is_none() {
            rin_bail!(
                "messaging.timeout: {} is longer than {MAX_MINUTES}m (a year)",
                self.messaging.timeout
            );
        }
        let mut seen = std::collections::HashSet::new();
        for agent in &self.agents {
            if !seen.insert(agent.id.as_str()) {
                rin_bail!("duplicate agent id: {:?}", agent.id);
            }
        }
        Ok(())
    }

    pub fn dummy_team_name() -> String {
        "rinteam".to_string()
    }
}

/// Parses a `rin.yml`.
#[cfg(feature = "host")]
pub fn from_yaml(text: &str) -> anyhow::Result<Config> {
    let value: serde_json::Value =
        serde_saphyr::from_str(text).map_err(|e| anyhow::anyhow!("invalid YAML: {e}"))?;
    from_value(value)
}

/// Reads a generated `rin.json`.
#[cfg(any(feature = "host", feature = "hub"))]
pub fn from_json_file(path: &std::path::Path) -> anyhow::Result<Config> {
    use anyhow::Context;
    let shown = path.display();
    let text = std::fs::read_to_string(path).with_context(|| format!("cannot read {shown}"))?;
    let value = serde_json::from_str(&text).with_context(|| format!("{shown} is not JSON"))?;
    from_value(value).with_context(|| shown.to_string())
}

/// Validates a config (from `rin.yml` or `rin.json`) and turns it into a [`Config`].
#[cfg(any(feature = "host", feature = "hub"))]
pub fn from_value(value: serde_json::Value) -> anyhow::Result<Config> {
    if value.get("rin").and_then(serde_json::Value::as_str) != Some("tohsaka") {
        rin_bail!("{NOT_A_RIN_CONFIG}");
    }
    // Compiled once: `rin generate` reads every other team's config too.
    static VALIDATOR: std::sync::LazyLock<jsonschema::Validator> = std::sync::LazyLock::new(|| {
        let schema: serde_json::Value =
            serde_json::from_str(SCHEMA).expect("embedded schema is JSON");
        jsonschema::validator_for(&schema).expect("embedded schema is valid")
    });
    let problems: Vec<String> = VALIDATOR
        .iter_errors(&value)
        .map(|e| {
            let path = e.instance_path().to_string();
            let path = if path.is_empty() {
                "/".to_string()
            } else {
                path
            };
            format!("  {path}: {e}")
        })
        .collect();
    if !problems.is_empty() {
        rin_bail!("invalid config:\n{}", problems.join("\n"));
    }
    let config: Config = serde_json::from_value(value)?;
    config.check()?;
    Ok(config)
}

#[cfg(feature = "host")]
pub fn rin_yaml_schema_directive() -> String {
    format!(
        "# yaml-language-server: $schema=https://raw.githubusercontent.com/w3lt/rin/refs/tags/v{}/rin-schema.json",
        VERSION
    ).to_string()
}

#[cfg(all(test, feature = "host"))]
mod tests {
    use super::*;

    const EXAMPLE: &str = include_str!("../../examples/team/rin.yml");

    fn err(yaml: &str) -> String {
        format!(
            "{:#}",
            from_yaml(yaml).expect_err("config should be rejected")
        )
    }

    /// The example team with one line replaced.
    fn edit(from: &str, to: &str) -> String {
        assert!(EXAMPLE.contains(from), "example has no {from:?}");
        EXAMPLE.replacen(from, to, 1)
    }

    #[test]
    fn example_parses_with_defaults_filled() {
        let config = from_yaml(&edit("messaging:\n  timeout: 30m\n  maxHops: 5\n", "")).unwrap();
        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(
            json["messaging"],
            serde_json::json!({"timeout": "30m", "maxHops": 5})
        );
        assert_eq!(
            serde_json::to_string(&config.messaging).unwrap(),
            r#"{"timeout":"30m","maxHops":5}"#
        );
        let sessions: Vec<_> = config.agents.iter().map(|a| a.max_sessions).collect();
        assert_eq!(sessions, [3, 3, 3, 3]);
        assert_eq!(json["agents"][0]["resources"]["cpus"], serde_json::json!(1));
        assert_eq!(config.messaging.timeout(), Duration::from_secs(1800));
    }

    #[test]
    fn rin_json_round_trips() {
        let config = from_yaml(EXAMPLE).unwrap();
        let back = from_value(serde_json::to_value(&config).unwrap()).unwrap();
        assert_eq!(config, back);
    }

    #[test]
    fn rejects_non_rin_configs() {
        assert_eq!(err(&edit("rin: tohsaka\n", "")), NOT_A_RIN_CONFIG);
        assert_eq!(err(&edit("rin: tohsaka", "rin: saber")), NOT_A_RIN_CONFIG);
        assert_eq!(err(""), NOT_A_RIN_CONFIG);
        assert_eq!(err("- 1\n"), NOT_A_RIN_CONFIG);
    }

    #[test]
    fn rejects_bad_versions() {
        let line = format!("version: {}", from_yaml(EXAMPLE).unwrap().version);
        assert!(err(&edit(&line, &format!("{line}-beta"))).contains("/version"));
        assert!(err(&edit(&line, "version: 1")).contains("/version"));
    }

    #[test]
    fn needs_exactly_one_leader() {
        assert!(err(&edit("type: leader", "type: member")).contains("/agents"));
        assert!(err(&edit("type: member", "type: leader")).contains("/agents"));
    }

    #[test]
    fn rejects_duplicate_ids() {
        assert!(
            err(&edit("id: frontend", "id: backend")).contains("duplicate agent id: \"backend\"")
        );
    }

    #[test]
    fn rejects_reserved_and_malformed_ids() {
        for id in ["hub", "user", "bifrost", "localhost", "Backend", "1x", ""] {
            let e = err(&edit("id: backend", &format!("id: \"{id}\"")));
            assert!(e.contains("/agents/1/id"), "{id}: {e}");
        }
    }

    #[test]
    fn pi_needs_an_api_key() {
        let yaml = edit(
            "harness: pi\n    authentication: apiKey",
            "harness: pi\n    authentication: subscription",
        );
        assert!(err(&yaml).contains("/agents/3/authentication"));
    }

    #[test]
    fn rejects_bad_resources() {
        let first = "      cpus: 1\n      ram: 2g\n\n  - id: backend";
        assert!(
            err(&edit(
                first,
                "      cpus: 1\n      ram: 2GB\n\n  - id: backend"
            ))
            .contains("/agents/0/resources/ram")
        );
        assert!(
            err(&edit(
                first,
                "      cpus: 0\n      ram: 2g\n\n  - id: backend"
            ))
            .contains("/agents/0/resources/cpus")
        );
        assert!(
            err(&edit(first, "      ram: 2g\n\n  - id: backend")).contains("/agents/0/resources")
        );
    }

    #[test]
    fn rejects_bad_messaging() {
        for (from, to) in [
            ("timeout: 30m", "timeout: 30s"),
            ("timeout: 30m", "timeout: 0m"),
            ("maxHops: 5", "maxHops: 0"),
            ("maxSessions: 3", "maxSessions: 0"),
        ] {
            let e = err(&edit(from, to));
            assert!(e.starts_with("invalid config:"), "{to}: {e}");
        }
    }

    #[test]
    fn rejects_missing_model_and_unknown_fields() {
        assert!(err(&edit("    model: gemini-3.8-flash\n", "")).contains("/agents/2"));
        assert!(err(&edit("rin: tohsaka", "rin: tohsaka\nfoo: 1")).contains("foo"));
        assert!(err(&edit("maxHops: 5", "maxHops: 5\n  foo: 1")).contains("foo"));
        assert!(
            err(&edit(
                "model: gemini-3.8-flash",
                "model: gemini-3.8-flash\n    foo: 1"
            ))
            .contains("foo")
        );
        assert!(err(&edit("ram: 2g\n", "ram: 2g\n      gpu: 1\n")).contains("gpu"));
    }

    #[test]
    fn parses_minutes() {
        assert_eq!(parse_minutes("1m"), Some(Duration::from_secs(60)));
        assert_eq!(parse_minutes("0m"), None);
        assert_eq!(parse_minutes("30s"), None);
        assert_eq!(
            parse_minutes("525600m"),
            Some(Duration::from_secs(525_600 * 60))
        );
        assert_eq!(parse_minutes("525601m"), None);
        assert_eq!(parse_minutes("300000000000000000000m"), None);
    }

    #[test]
    fn rejects_absurd_timeouts() {
        let e = err(&edit("timeout: 30m", "timeout: 300000000000000000m"));
        assert!(
            e.contains("messaging.timeout: 300000000000000000m is longer than 525600m"),
            "{e}"
        );
    }

    #[test]
    fn formats_durations() {
        assert_eq!(format_duration(Duration::from_secs(59)), "59s");
        assert_eq!(format_duration(Duration::from_secs(61)), "1m");
        assert_eq!(format_duration(Duration::from_secs(3700)), "1h1m");
    }

    #[test]
    fn tohsaka_is_the_only_rin() {
        assert_eq!(
            serde_json::to_value(Tohsaka).unwrap(),
            serde_json::json!("tohsaka")
        );
        assert_eq!(
            serde_json::from_value::<Tohsaka>(serde_json::json!("tohsaka")).unwrap(),
            Tohsaka
        );
        assert_eq!(
            serde_json::from_str::<Tohsaka>(r#""tohsaka""#).unwrap(),
            Tohsaka
        );
        assert!(serde_json::from_value::<Tohsaka>(serde_json::json!("saber")).is_err());
        assert!(serde_json::from_value::<Tohsaka>(serde_json::json!(1)).is_err());
    }

    #[test]
    fn init_makes_a_valid_team() {
        let (config, instructions) = Config::init("my-team".into());
        assert_eq!(config.rin, Tohsaka);
        assert_eq!(config.name, "my-team");
        assert_eq!(config.version, VERSION);
        assert_eq!(config.messaging, Messaging::default());
        assert_eq!(instructions.len(), config.agents.len());

        let back = from_value(serde_json::to_value(&config).unwrap()).unwrap();
        assert_eq!(back, config);
    }

    #[test]
    fn init_instructions_are_unindented_markdown() {
        let (config, instructions) = Config::init("my-team".into());
        for (agent, text) in config.agents.iter().zip(&instructions) {
            assert!(text.starts_with("# "), "{}: {text:?}", agent.id);
            for line in text.lines() {
                assert!(!line.starts_with(' '), "{}: indented {line:?}", agent.id);
            }
        }
    }

    #[test]
    fn dummy_team_name_is_a_valid_name() {
        let (config, _) = Config::init(Config::dummy_team_name());
        assert_eq!(
            from_value(serde_json::to_value(&config).unwrap()).unwrap(),
            config
        );
    }

    #[test]
    fn schema_directive_points_at_this_versions_schema() {
        let directive = rin_yaml_schema_directive();
        assert!(directive.starts_with("# yaml-language-server: $schema=https://"));
        assert!(directive.ends_with(&format!("/v{VERSION}/rin-schema.json")));
        assert!(!directive.contains('\n'));
    }
}
