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
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    let s = toml::to_string_pretty(cfg)
        .map_err(|e| RegistryError::Storage(format!("toml ser: {e}")))?;
    // Create owner-only (0600) so the config — which may grow to hold sensitive
    // fields — never has a world-readable window (audit cycle-2 P3).
    #[cfg(unix)]
    {
        use std::io::Write as _;
        use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        f.write_all(s.as_bytes())?;
        // Cover the pre-existing-file case (mode only applies on create).
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    std::fs::write(path, s)?;
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::{GtdxConfig, save};
    use std::os::unix::fs::PermissionsExt as _;

    fn mode_of(path: &std::path::Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn save_restricts_file_and_parent_to_owner_only() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path().join("gtdx");
        let path = dir.join("config.toml");
        save(&path, &GtdxConfig::default()).unwrap();
        assert_eq!(mode_of(&path), 0o600, "config file must be owner-only");
        assert_eq!(mode_of(&dir), 0o700, "config dir must be owner-only");
    }
}
