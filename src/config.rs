use std::collections::HashMap;

use anyhow::{anyhow, Context, Result};
use config::{File, FileFormat};
use serde::Deserialize;

pub static DEFAULT_CONFIG: &str = include_str!("../default-settings.toml");

pub fn load() -> Result<Config> {
    let config = config::Config::builder()
        .add_source(File::from_str(DEFAULT_CONFIG, FileFormat::Toml))
        .add_source(File::new("/etc/global-hotkeys.toml", FileFormat::Toml).required(true))
        .build()
        .context("failed to read config")?;

    config.try_deserialize().context("failed to parse config")
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub run_as: String,
    pub shell: String,
    pub env: HashMap<String, String>,
    pub keycodes: HashMap<String, u32>,
    pub hotkeys: Vec<Hotkey>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hotkey {
    pub name: Option<String>,
    pub key: String,
    pub command: String,
}

#[derive(Debug)]
pub struct ParsedHotkey {
    pub keys: Vec<u32>,
    pub command: String,
}

impl ParsedHotkey {
    pub fn new(hotkey: &Hotkey, keycodes: &HashMap<String, u32>) -> Result<Self> {
        let mut keys = hotkey
            .key
            .split('+')
            .map(|key| {
                keycodes
                    .get(key)
                    .copied()
                    .ok_or_else(|| anyhow!("undefined keycode: {}", key))
            })
            .collect::<Result<Vec<_>>>()?;

        if keys.is_empty() {
            return Err(anyhow!("no keys defined for hotkey"));
        }

        // important for detection later
        // this means order doesn't matter for all but the last key
        let len = keys.len();
        keys[..len - 1].sort_unstable();

        Ok(Self {
            keys,
            command: hotkey.command.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_default_config() {
        let res = config::Config::builder()
            .add_source(File::from_str(DEFAULT_CONFIG, FileFormat::Toml))
            .build()
            .and_then(|c| c.try_deserialize::<Config>());

        assert!(res.is_ok(), "{res:?}");
    }
}
