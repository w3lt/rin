//! `rin mcp`: the stdio MCP server Claude Code, Codex and Antigravity start
//! per session. Exactly two tools; the sender and hop count are never parameters.

use std::sync::Arc;

use anyhow::{Result, anyhow};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::{ServerHandler, ServiceExt, schemars, tool, tool_handler, tool_router};
use serde::Deserialize;

use crate::client::{Client, current_hops};

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SendMessage {
    /// Id of the agent to message (see list_agents).
    pub to: String,
    /// The message. The receiver starts a fresh session with it and its final answer comes back to you.
    pub text: String,
}

#[derive(Clone)]
pub struct RinMcp {
    client: Arc<Client>,
    /// This session's `RIN_HOPS`; messages it sends are one hop further.
    hops: u32,
}

#[tool_router]
impl RinMcp {
    #[tool(
        description = "Send a message to another agent on your Rin team and wait for its final answer, which is returned. The receiver starts a fresh session for each message."
    )]
    async fn send_message(
        &self,
        Parameters(args): Parameters<SendMessage>,
    ) -> Result<String, String> {
        self.client
            .send(&args.to, &args.text, self.hops + 1)
            .await
            .map_err(|e| e.to_string())
    }

    #[tool(
        description = "List the agents on your Rin team: id, label, type, harness, delegated sessions running (n/max) and messages queued. You are marked \"you\"."
    )]
    async fn list_agents(&self) -> Result<String, String> {
        let response = self.client.agents().await.map_err(|e| e.to_string())?;
        Ok(crate::display::agents(&response))
    }
}

#[tool_handler(name = "rin")]
impl ServerHandler for RinMcp {}

pub async fn main() -> Result<()> {
    let client = Client::from_agent_env()
        .ok_or_else(|| anyhow!("RIN_TOKEN is not set: rin mcp runs inside an agent container"))?;
    let server = RinMcp {
        client: Arc::new(client),
        hops: current_hops(),
    };
    let running = server.serve(rmcp::transport::stdio()).await?;
    running.waiting().await?;
    Ok(())
}
