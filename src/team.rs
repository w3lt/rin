//! `$RIN_HOME` and the generated team folders in it.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

use crate::config::Config;

/// Where the hub is published on your machine.
pub const HOST_HUB_URL: &str = "http://127.0.0.1:10302";
pub const HUB_PORT: u16 = 10302;

/// `$RIN_HOME`, default `~/.rin`.
pub fn rin_home() -> Result<PathBuf> {
    if let Some(home) = std::env::var_os("RIN_HOME").filter(|h| !h.is_empty()) {
        return Ok(PathBuf::from(home));
    }
    let home =
        std::env::var_os("HOME").ok_or_else(|| anyhow!("neither RIN_HOME nor HOME is set"))?;
    Ok(PathBuf::from(home).join(".rin"))
}

fn current_file(home: &Path) -> PathBuf {
    home.join("current")
}

pub fn set_current(home: &Path, name: &str) -> Result<()> {
    crate::files::write_if_changed(&current_file(home), format!("{name}\n").as_bytes())
}

/// One generated team in `$RIN_HOME/teams/<name>`.
#[derive(Debug, Clone)]
pub struct Team {
    pub name: String,
    pub dir: PathBuf,
}

impl Team {
    pub fn at(home: &Path, name: &str) -> Team {
        Team {
            name: name.to_string(),
            dir: home.join("teams").join(name),
        }
    }

    /// The team named by `--team`, or else the current team.
    pub fn resolve(flag: Option<&str>) -> Result<Team> {
        let home = rin_home()?;
        let name = match flag {
            Some(name) => name.to_string(),
            None => {
                let file = current_file(&home);
                let name = fs::read_to_string(&file).map_err(|_| {
                    anyhow!("no current team: run `rin generate` first, or pass --team <name>")
                })?;
                name.trim().to_string()
            }
        };
        let team = Team::at(&home, &name);
        if !team.rin_json().is_file() {
            bail!(
                "unknown team {name:?}: no {} (run `rin generate`)",
                team.rin_json().display()
            );
        }
        Ok(team)
    }

    pub fn project(&self) -> String {
        format!("rin-{}", self.name)
    }

    pub fn compose_file(&self) -> PathBuf {
        self.dir.join("docker-compose.yml")
    }

    pub fn rin_json(&self) -> PathBuf {
        self.dir.join("rin.json")
    }

    pub fn tokens_file(&self) -> PathBuf {
        self.dir.join("tokens.json")
    }

    pub fn config(&self) -> Result<Config> {
        crate::config::from_json_file(&self.rin_json())
    }

    pub fn tokens(&self) -> Result<Tokens> {
        read_tokens(&self.tokens_file())?.ok_or_else(|| {
            anyhow!(
                "{} is missing: run `rin generate`",
                self.tokens_file().display()
            )
        })
    }
}

/// `tokens.json`: one token per agent id, plus `user` for you.
pub type Tokens = BTreeMap<String, String>;

/// Reads a `tokens.json`; `None` if there is none yet.
pub fn read_tokens(path: &Path) -> Result<Option<Tokens>> {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text)
            .map(Some)
            .with_context(|| format!("{} is not a tokens file", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("cannot read {}", path.display())),
    }
}
