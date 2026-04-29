use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::RegistryError;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GtdxConfig {
    #[serde(default)]
    pub default: DefaultSection,
    #[serde(default, rename = "registries")]
    pub registries: Vec<RegistryEntry>,
    #[serde(default, rename = "extensions")]
    pub extensions: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultSection {
    pub registry: String,
    #[serde(rename = "trust-policy")]
    pub trust_policy: String,
}

impl Default for DefaultSection {
    fn default() -> Self {
        Self {
            registry: "greentic-store".into(),
            trust_policy: "normal".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryEntry {
    pub name: String,
    pub url: String,
    #[serde(rename = "token-env", default)]
    pub token_env: Option<String>,
}

pub fn load(path: &Path) -> Result<GtdxConfig, RegistryError> {
    if !path.exists() {
        return Ok(GtdxConfig::default());
    }
    let s = std::fs::read_to_string(path)?;
    Ok(toml::from_str(&s)?)
}

pub fn save(path: &Path, cfg: &GtdxConfig) -> Result<(), RegistryError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let s = toml::to_string_pretty(cfg)
        .map_err(|e| RegistryError::Storage(format!("toml ser: {e}")))?;
    std::fs::write(path, s)?;
    Ok(())
}
