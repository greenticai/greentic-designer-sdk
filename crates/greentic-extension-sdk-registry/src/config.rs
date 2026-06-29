use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::RegistryError;

/// Canonical name of the public Greentic extension store. Used as the default
/// registry so `gtdx login` / `gtdx publish` target the store out of the box
/// without the user having to `gtdx registries add` it first.
pub const GREENTIC_STORE_NAME: &str = "greentic-store";

/// Built-in URL backing [`GREENTIC_STORE_NAME`]. A `[[registries]]` entry in
/// `config.toml` with the same name overrides this (e.g. to point at a staging
/// store), so the constant is only a fallback, never a hard-coded override.
pub const GREENTIC_STORE_URL: &str = "https://store.greentic.cloud";

/// Resolve a registry name to its URL.
///
/// A user-configured `[[registries]]` entry always wins; when none matches and
/// the name is the canonical [`GREENTIC_STORE_NAME`], the built-in
/// [`GREENTIC_STORE_URL`] is returned. Returns `None` for any other unknown
/// name so callers can surface a clear "register it first" error.
#[must_use]
pub fn resolve_registry_url(cfg: &GtdxConfig, name: &str) -> Option<String> {
    if let Some(entry) = cfg.registries.iter().find(|entry| entry.name == name) {
        return Some(entry.url.clone());
    }
    (name == GREENTIC_STORE_NAME).then(|| GREENTIC_STORE_URL.to_string())
}

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

#[cfg(test)]
mod resolve_tests {
    use super::{
        GREENTIC_STORE_NAME, GREENTIC_STORE_URL, GtdxConfig, RegistryEntry, resolve_registry_url,
    };

    #[test]
    fn builtin_store_resolves_without_config() {
        let cfg = GtdxConfig::default();
        assert_eq!(
            resolve_registry_url(&cfg, GREENTIC_STORE_NAME).as_deref(),
            Some(GREENTIC_STORE_URL)
        );
    }

    #[test]
    fn configured_entry_overrides_builtin() {
        let mut cfg = GtdxConfig::default();
        cfg.registries.push(RegistryEntry {
            name: GREENTIC_STORE_NAME.to_string(),
            url: "https://staging.example.test".to_string(),
            token_env: None,
        });
        assert_eq!(
            resolve_registry_url(&cfg, GREENTIC_STORE_NAME).as_deref(),
            Some("https://staging.example.test")
        );
    }

    #[test]
    fn unknown_name_is_none() {
        let cfg = GtdxConfig::default();
        assert!(resolve_registry_url(&cfg, "no-such-registry").is_none());
    }
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
