//! Talking to the hub: `rin send`, `rin agents`, `rin log`, and `rin mcp`.

#![cfg_attr(not(all(feature = "host", feature = "hub", feature = "agent")), allow(dead_code))]

use anyhow::{Result, anyhow, bail};
use serde::de::DeserializeOwned;

use crate::proto::{AgentsResponse, Event, SendRequest};

pub struct Client {
    url: String,
    token: String,
    http: reqwest::Client,
}

impl Client {
    pub fn new(url: impl Into<String>, token: impl Into<String>) -> Client {
        Client { url: url.into().trim_end_matches('/').to_string(), token: token.into(), http: reqwest::Client::new() }
    }

    /// Inside an agent container: `RIN_TOKEN` and `RIN_HUB_URL`.
    pub fn from_agent_env() -> Option<Client> {
        let token = env_var("RIN_TOKEN")?;
        let url = env_var("RIN_HUB_URL").unwrap_or_else(|| "http://hub:7000".to_string());
        Some(Client::new(url, token))
    }

    /// Sends a message and waits for the reply (no client-side timeout:
    /// the hub enforces the team's timeout).
    pub async fn send(&self, to: &str, text: &str, hops: u32) -> Result<String> {
        let request = SendRequest { to: to.to_string(), text: text.to_string(), hops };
        let response = self.http.post(format!("{}/messages", self.url)).bearer_auth(&self.token).json(&request).send().await;
        let body: serde_json::Value = self.read(response).await?;
        body.get("reply")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| anyhow!("the hub sent no reply"))
    }

    pub async fn agents(&self) -> Result<AgentsResponse> {
        let response = self.http.get(format!("{}/agents", self.url)).bearer_auth(&self.token).send().await;
        self.read(response).await
    }

    pub async fn log(&self, agent: Option<&str>, after: i64) -> Result<Vec<Event>> {
        let mut url = format!("{}/log?after={after}", self.url);
        if let Some(agent) = agent {
            // Ids are [a-z0-9-], so they need no escaping; anything else is no agent.
            if agent.is_empty() || !agent.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
                bail!("unknown agent: {agent:?}");
            }
            url.push_str(&format!("&agent={agent}"));
        }
        let response = self.http.get(url).bearer_auth(&self.token).send().await;
        #[derive(serde::Deserialize)]
        struct Log {
            events: Vec<Event>,
        }
        Ok(self.read::<Log>(response).await?.events)
    }

    async fn read<T: DeserializeOwned>(&self, response: reqwest::Result<reqwest::Response>) -> Result<T> {
        let response = response.map_err(|e| anyhow!("cannot reach the hub at {}: {}", self.url, root_cause(&e)))?;
        let status = response.status();
        let text = response.text().await.map_err(|e| anyhow!("bad answer from the hub ({status}): {e}"))?;
        let body: serde_json::Value = match serde_json::from_str(&text) {
            Ok(body) => body,
            // Errors raised before the hub's handlers (body too large, ...) are plain text.
            Err(_) if !status.is_success() => bail!("the hub answered {status}: {}", text.trim()),
            Err(e) => bail!("bad answer from the hub ({status}): {e}"),
        };
        if !status.is_success() {
            match body.get("error").and_then(serde_json::Value::as_str) {
                Some(error) => bail!("{error}"),
                None => bail!("the hub answered {status}"),
            }
        }
        Ok(serde_json::from_value(body)?)
    }
}

fn root_cause(e: &dyn std::error::Error) -> String {
    let mut cause = e;
    while let Some(next) = cause.source() {
        cause = next;
    }
    cause.to_string()
}

/// This session's hop count: `RIN_HOPS`, unset (0) for sessions you start yourself.
pub fn current_hops() -> u32 {
    env_var("RIN_HOPS").and_then(|h| h.trim().parse().ok()).unwrap_or(0)
}

/// An environment variable, or else the nearest ancestor process's value.
///
/// Harnesses may start MCP servers and tools with a trimmed environment; the
/// session itself (started by `rin runtime`, or from `rin shell`) still has
/// `RIN_HOPS` / `RIN_TOKEN`, so look up the process tree (Linux only).
pub fn env_var(name: &str) -> Option<String> {
    if let Ok(value) = std::env::var(name) {
        return Some(value);
    }
    #[cfg(target_os = "linux")]
    {
        let mut pid = parent_of("self")?;
        while pid > 1 {
            if let Some(value) = environ_of(pid, name) {
                return Some(value);
            }
            pid = parent_of(&pid.to_string())?;
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn parent_of(pid: &str) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // pid (comm) state ppid …; comm may contain spaces and parentheses.
    stat.rsplit_once(')')?.1.split_whitespace().nth(1)?.parse().ok()
}

#[cfg(target_os = "linux")]
fn environ_of(pid: u32, name: &str) -> Option<String> {
    let environ = std::fs::read(format!("/proc/{pid}/environ")).ok()?;
    let prefix = format!("{name}=");
    environ
        .split(|b| *b == 0)
        .find_map(|entry| entry.strip_prefix(prefix.as_bytes()))
        .map(|value| String::from_utf8_lossy(value).into_owned())
}
