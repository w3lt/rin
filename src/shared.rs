pub mod cmd;
pub mod config;
#[macro_use]
pub mod log;

/// Returns early with an `anyhow` error prefixed with `[Rin]`.
#[macro_export]
macro_rules! rin_bail {
    ($($arg:tt)*) => {
        ::anyhow::bail!("{}", format_args!($($arg)*))
    };
}

use crate::client::{Client, current_hops};
use anyhow::Result;

cfg_select! {
    feature = "host" => {
        use crate::team;
        use crate::host_cli;
    }
    _ => {}
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
        Ok((host_cli::client(&team)?, 1))
    }

    #[cfg(not(feature = "host"))]
    rin_bail!("RIN_TOKEN is not set: run this inside an agent container")
}
