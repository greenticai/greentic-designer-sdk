pub mod dev;
pub mod disable;
pub mod doctor;
pub mod enable;
pub mod info;
pub mod install;
pub mod keygen;
pub mod list;
pub mod login;
pub mod new;
pub mod publish;
pub mod registries;
pub mod search;
pub mod sign;
pub mod uninstall;
pub mod validate;
pub mod verify;

use std::path::Path;

use anyhow::Result;
use greentic_extension_sdk_registry::config;

pub fn load_config(home: &Path) -> Result<config::GtdxConfig> {
    config::load(&home.join("config.toml")).map_err(|e| anyhow::anyhow!("config: {e}"))
}

pub fn save_config(home: &Path, cfg: &config::GtdxConfig) -> Result<()> {
    config::save(&home.join("config.toml"), cfg).map_err(|e| anyhow::anyhow!("config save: {e}"))
}
