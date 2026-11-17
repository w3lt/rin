//! What the hub, runtimes and clients send each other.

#![cfg_attr(not(all(feature = "host", feature = "hub", feature = "agent")), allow(dead_code))]

use serde::{Deserialize, Serialize};

/// `POST /messages` (sender → hub). Who is sending comes from the token, never the body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendRequest {
    pub to: String,
    pub text: String,
    #[serde(default = "one")]
    pub hops: u32,
}

fn one() -> u32 {
    1
}

/// `POST /run` (hub → receiver's runtime).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRequest {
    pub from: String,
    pub from_label: String,
    pub text: String,
    pub hops: u32,
    /// What is left of the sender's timeout; the runtime kills the session after it.
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RunResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// `GET /agents`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentsResponse {
    pub agents: Vec<AgentRow>,
    /// Current deadlock warnings.
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRow {
    pub id: String,
    pub label: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub harness: String,
    /// Delegated sessions running now.
    pub sessions: usize,
    pub max_sessions: usize,
    pub queued: usize,
    /// The caller.
    #[serde(default)]
    pub you: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Message,
    Reply,
    Error,
    Warning,
}

/// One row of `rin log`, as served by `GET /log`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub id: i64,
    /// Unix time in milliseconds.
    pub ts: i64,
    pub kind: Kind,
    /// The message this event belongs to (its `message` event id); none for warnings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub msg: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hops: Option<u32>,
    pub text: String,
}
