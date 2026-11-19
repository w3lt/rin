//! The team config: `rin.yml` as typed data, checked against the embedded JSON Schema.
//!
//! The same type is written out as `rin.json` (defaults filled in) by `rin generate`
//! and read back by the hub.

#![cfg_attr(
    not(all(feature = "host", feature = "hub", feature = "agent")),
    allow(dead_code)
)]

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// `rin-schema.json`, embedded so every `rin` checks configs the same way.
pub const SCHEMA: &str = include_str!("../rin-schema.json");

/// The version of this `rin` binary; a team's `version:` must match it exactly.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub const NOT_A_RIN_CONFIG: &str = r#"not a Rin config: "rin" must be "tohsaka""#;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub rin: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub messaging: Messaging,
    pub agents: Vec<Agent>,
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

fn default_timeout() -> String {
    "30m".into()
}

fn default_max_hops() -> u32 {
    5
}

fn default_max_sessions() -> u32 {
    3
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Agent {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: AgentType,
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
            anyhow::bail!(
                "messaging.timeout: {} is longer than {MAX_MINUTES}m (a year)",
                self.messaging.timeout
            );
        }
        let mut seen = std::collections::HashSet::new();
        for agent in &self.agents {
            if !seen.insert(agent.id.as_str()) {
                anyhow::bail!("duplicate agent id: {:?}", agent.id);
            }
        }
        Ok(())
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
        anyhow::bail!(NOT_A_RIN_CONFIG);
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
        anyhow::bail!("invalid config:\n{}", problems.join("\n"));
    }
    let config: Config = serde_json::from_value(value)?;
    config.check()?;
    Ok(config)
}

#[cfg(all(test, feature = "host"))]
mod tests {
    use super::*;

    const EXAMPLE: &str = include_str!("../examples/team/rin.yml");

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
        let config = from_yaml(&edit("messaging:\n  timeout: 30m     # how long a sender waits for a reply\n  maxHops: 5       # longest chain of agent → agent messages\n", "")).unwrap();
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
        assert_eq!(sessions, [3, 3, 3]);
        assert_eq!(json["agents"][0]["resources"]["cpus"], serde_json::json!(2));
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
        assert!(err(&yaml).contains("/agents/1/authentication"));
    }

    #[test]
    fn rejects_bad_resources() {
        assert!(
            err(&edit(
                "cpus: 2, ram: 4g }\n\n  - id: backend",
                "cpus: 2, ram: 4GB }\n\n  - id: backend"
            ))
            .contains("/agents/0/resources/ram")
        );
        assert!(
            err(&edit(
                "cpus: 2, ram: 4g }\n\n  - id: backend",
                "cpus: 0, ram: 4g }\n\n  - id: backend"
            ))
            .contains("/agents/0/resources/cpus")
        );
        assert!(
            err(&edit(
                "cpus: 2, ram: 4g }\n\n  - id: backend",
                "ram: 4g }\n\n  - id: backend"
            ))
            .contains("/agents/0/resources")
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
        assert!(err(&edit("    model: gpt-5-codex\n", "")).contains("/agents/2"));
        assert!(err(&edit("rin: tohsaka", "rin: tohsaka\nfoo: 1")).contains("foo"));
        assert!(err(&edit("maxHops: 5", "maxHops: 5\n  foo: 1")).contains("foo"));
        assert!(
            err(&edit(
                "model: gpt-5-codex",
                "model: gpt-5-codex\n    foo: 1"
            ))
            .contains("foo")
        );
        assert!(err(&edit("ram: 4g }", "ram: 4g, gpu: 1 }")).contains("gpu"));
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
}
