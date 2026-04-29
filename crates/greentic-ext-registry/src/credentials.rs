use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::RegistryError;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Credentials {
    #[serde(default)]
    pub tokens: std::collections::BTreeMap<String, String>,
}

impl Credentials {
    pub fn load(path: &Path) -> Result<Self, RegistryError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let s = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&s)?)
    }

    pub fn save(&self, path: &Path) -> Result<(), RegistryError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let s = toml::to_string_pretty(self)
            .map_err(|e| RegistryError::Storage(format!("toml ser: {e}")))?;
        std::fs::write(path, s)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perm = std::fs::metadata(path)?.permissions();
            perm.set_mode(0o600);
            std::fs::set_permissions(path, perm)?;
        }
        Ok(())
    }

    pub fn set(&mut self, registry: &str, token: &str) {
        self.tokens.insert(registry.into(), token.into());
    }

    #[must_use]
    pub fn get(&self, registry: &str) -> Option<&str> {
        self.tokens.get(registry).map(String::as_str)
    }

    pub fn remove(&mut self, registry: &str) -> Option<String> {
        self.tokens.remove(registry)
    }
}
