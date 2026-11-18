//! Rin: a team of AI agents on your machine, one Docker container per agent.

cfg_select! {
    feature = "host" => {
        mod cli;
        mod compose;
        mod files;
        mod generate;
        mod yaml;
        mod team;

        use std::path::PathBuf;
    }
}

cfg_select! {
    feature = "hub" => {
        mod hub;
    }
}

cfg_select! {
    feature = "agent" => {
        mod mcp;
        mod runtime;
    }
}

cfg_select! {
    any(feature = "hub", feature = "agent") => {
        mod signals;
    }
}

cfg_select! {
    all(test, feature = "host", feature = "hub", feature = "agent") => {
        mod tests;
    }
    _ => {}
}

mod client;
mod config;
mod display;
mod proto;

use anyhow::Result;
use clap::{Parser, Subcommand};

use client::{Client, current_hops};

#[derive(Parser)]
#[command(
    name = "rin",
    version,
    about = "Rin (Tohsaka) is all you need to deploy a team of AI agents. She is the best waifu btw"
)]
struct Cli {
    /// Act on this team instead of the current one.
    #[arg(long, global = true, value_name = "NAME")]
    team: Option<String>,
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    #[cfg(feature = "host")]
    #[command(flatten)]
    Host(HostCmd),

    #[cfg(feature = "hub")]
    #[command(flatten)]
    Hub(HubCmd),

    #[cfg(feature = "agent")]
    #[command(flatten)]
    Agent(AgentCmd),

    /// Message an agent and print its reply.
    Send {
        to: String,
        #[arg(required = true, num_args = 1..)]
        text: Vec<String>,
    },
    /// List the team's agents.
    Agents,
}

#[derive(Subcommand)]
enum HostCmd {
    /// Validate rin.yml and write the team's folder in $RIN_HOME; makes it the current team.
    Generate {
        /// Folder with one <agent id>/INSTRUCTIONS.md per agent.
        #[arg(long, value_name = "DIR")]
        instructions: PathBuf,
        /// The team config.
        #[arg(
            short = 'f',
            long = "file",
            default_value = "rin.yml",
            value_name = "FILE"
        )]
        file: PathBuf,
    },
    /// Switch the current team.
    Use { name: String },
    /// Start the team (builds images the first time).
    Up,
    /// Stop the team; homes and the log are kept.
    Down,
    /// Open a shell in an agent's container.
    Shell { id: String },
    /// Agents' state, sessions and queues, and deadlock warnings.
    Status,
    /// Message history.
    Log {
        /// Keep printing new messages.
        #[arg(short, long)]
        follow: bool,
        /// Only messages to or from this agent.
        id: Option<String>,
    },
}

#[derive(Subcommand)]
enum HubCmd {
    /// The hub server (hub container).
    #[command(hide = true)]
    Hub,
}

#[derive(Subcommand)]
enum AgentCmd {
    /// An agent container's main process.
    #[cfg(feature = "agent")]
    #[command(hide = true)]
    Runtime,
    /// Rin's MCP server, started by the harness.
    #[cfg(feature = "agent")]
    #[command(hide = true)]
    Mcp,
}

/// Inside an agent container `rin` speaks as that agent; on your machine, as you.
#[cfg_attr(not(feature = "host"), allow(unused_variables))]
fn sender(team: Option<&str>) -> Result<(Client, u32)> {
    if let Some(client) = Client::from_agent_env() {
        return Ok((client, current_hops() + 1));
    }
    #[cfg(feature = "host")]
    {
        let team = team::Team::resolve(team)?;
        Ok((cli::client(&team)?, 1))
    }
    #[cfg(not(feature = "host"))]
    anyhow::bail!("RIN_TOKEN is not set: run this inside an agent container")
}

async fn run(cli: Cli) -> Result<()> {
    #[cfg(feature = "host")]
    let team = || team::Team::resolve(cli.team.as_deref());
    match cli.command {
        #[cfg(feature = "host")]
        Cmd::Host(cmd) => match cmd {
            HostCmd::Generate { instructions, file } => {
                // RIN_GIT_REV builds the images from another Rin revision (e.g. refs/heads/<branch>)
                // to try unreleased changes; releases build from the `v<version>` tag.
                let git_rev = std::env::var("RIN_GIT_REV")
                    .unwrap_or_else(|_| format!("v{}", config::VERSION));
                let generated =
                    generate::generate(&file, &instructions, &team::rin_home()?, &git_rev)?;
                for warning in &generated.warnings {
                    eprintln!("warning: {warning}");
                }
                println!(
                    "team {} written to {} (now the current team)",
                    generated.team.name,
                    generated.team.dir.display()
                );
            }
            HostCmd::Use { name } => cli::use_team(&name)?,

            HostCmd::Up => cli::up(&team()?)?,

            HostCmd::Down => cli::down(&team()?)?,

            HostCmd::Shell { id } => cli::shell(&team()?, &id)?,

            HostCmd::Status => cli::status(&team()?).await?,

            HostCmd::Log { follow, id } => cli::log(&team()?, follow, id.as_deref()).await?,
        },

        #[cfg(feature = "hub")]
        Cmd::Hub(cmd) => match cmd {
            HubCmd::Hub => hub::main().await?,
        },

        #[cfg(feature = "agent")]
        Cmd::Agent(cmd) => match cmd {
            AgentCmd::Runtime => runtime::main().await?,
            AgentCmd::Mcp => mcp::main().await?,
        },

        Cmd::Send { to, text } => {
            let (client, hops) = sender(cli.team.as_deref())?;
            println!("{}", client.send(&to, &text.join(" "), hops).await?);
        }
        Cmd::Agents => {
            let (client, _) = sender(cli.team.as_deref())?;
            print!("{}", display::agents(&client.agents().await?));
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli).await {
        eprintln!("rin: {e:#}");
        std::process::exit(1);
    }
}
