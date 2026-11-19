//! Rin: a team of AI agents on your machine, one Docker container per agent.

cfg_select! {
    feature = "host" => {
        mod host_cli;
        mod compose;
        mod files;
        mod yaml;
        mod team;

        use std::path::PathBuf;

        use clap_complete::{generate, Shell};
        use clap::CommandFactory;
    }
    _ => {}
}

cfg_select! {
    feature = "hub" => {
        mod hub;
    }
    _ => {}
}

cfg_select! {
    feature = "agent" => {
        mod mcp;
        mod agent_runtime;
    }
    _ => {}
}

cfg_select! {
    any(feature = "hub", feature = "agent") => {
        mod signals;
    }
    _ => {}
}

cfg_select! {
    all(test, feature = "host", feature = "hub", feature = "agent") => {
        mod tests;
    }
    _ => {}
}

mod client;
mod display;
mod proto;
mod shared;

use anyhow::Result;
use clap::{Parser, Subcommand};

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

#[cfg(feature = "host")]
#[derive(Subcommand)]
enum HostCmd {
    Completions {
        shell: Shell,
    },
    /// Init new Rin project
    Init {
        /// Directory where the configuration will be initialized
        #[arg(short = 'd', long = "dir", default_value = ".", value_name = "DIR")]
        directory: PathBuf,

        name: Option<String>,
    },
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
    Use {
        name: String,
    },
    /// Start the team (builds images the first time).
    Up,
    /// Stop the team; homes and the log are kept.
    Down,
    /// Open a shell in an agent's container.
    Shell {
        id: String,
    },
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

#[cfg(feature = "hub")]
#[derive(Subcommand)]
enum HubCmd {
    /// The hub server (hub container).
    #[command(hide = true)]
    Hub,
}

#[cfg(feature = "agent")]
#[derive(Subcommand)]
enum AgentCmd {
    /// An agent container's main process.
    #[command(hide = true)]
    Runtime,
    /// Rin's MCP server, started by the harness.
    #[command(hide = true)]
    Mcp,
}

async fn run(cli: Cli) -> Result<()> {
    #[cfg(feature = "host")]
    let team = || team::Team::resolve(cli.team.as_deref());
    match cli.command {
        #[cfg(feature = "host")]
        Cmd::Host(cmd) => match cmd {
            HostCmd::Completions { shell } => {
                let mut cmd = Cli::command();
                generate(shell, &mut cmd, "rin", &mut std::io::stdout())
            }
            HostCmd::Init { directory, name } => host_cli::init_project(name, directory)?,
            HostCmd::Generate { instructions, file } => {
                host_cli::generate_config(file, instructions)?
            }
            HostCmd::Use { name } => host_cli::use_team(&name)?,

            HostCmd::Up => host_cli::up(&team()?)?,

            HostCmd::Down => host_cli::down(&team()?)?,

            HostCmd::Shell { id } => host_cli::shell(&team()?, &id)?,

            HostCmd::Status => host_cli::status(&team()?).await?,

            HostCmd::Log { follow, id } => host_cli::log(&team()?, follow, id.as_deref()).await?,
        },

        #[cfg(feature = "hub")]
        Cmd::Hub(cmd) => match cmd {
            HubCmd::Hub => hub::main().await?,
        },

        #[cfg(feature = "agent")]
        Cmd::Agent(cmd) => match cmd {
            AgentCmd::Runtime => agent_runtime::main().await?,
            AgentCmd::Mcp => mcp::main().await?,
        },

        Cmd::Send { to, text } => shared::cmd::send_message(cli.team.as_deref(), to, &text).await?,
        Cmd::Agents => shared::cmd::show_agents(cli.team.as_deref()).await?,
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
