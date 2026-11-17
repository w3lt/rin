//! Rin: a team of AI agents on your machine, one Docker container per agent.

mod config;
#[cfg(feature = "host")]
mod compose;
#[cfg(feature = "host")]
mod files;
#[cfg(feature = "host")]
mod generate;
#[cfg(feature = "host")]
mod team;
#[cfg(feature = "host")]
mod yaml;
mod proto;
#[cfg(feature = "hub")]
mod hub;
#[cfg(any(feature = "hub", feature = "agent"))]
mod signals;
#[cfg(feature = "agent")]
mod runtime;

fn main() {}
