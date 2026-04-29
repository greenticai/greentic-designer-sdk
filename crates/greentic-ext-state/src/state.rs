use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const STATE_FILENAME: &str = "extensions-state.json";

/// Persistent enable/disable state for installed extensions.
///
/// Schema v1.0 — keys in `default.enabled` use the format `<id>@<version>`.
/// Missing keys default to enabled. The `tenants` map is reserved for the
/// future designer-admin track and is ignored by current readers.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ExtensionState {
    #[serde(default = "default_schema")]
    pub schema: String,
    #[serde(default)]
    pub default: ScopeState,
    #[serde(default)]
    pub tenants: HashMap<String, ScopeState>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ScopeState {
    #[serde(default)]
    pub enabled: HashMap<String, bool>,
}

fn default_schema() -> String {
    "1.0".to_string()
}

impl ExtensionState {
    /// Load state from `<home>/extensions-state.json`. Missing file returns
    /// the default (everything enabled). Parse errors propagate.
    pub fn load(home: &Path) -> Result<Self, crate::StateError> {
        let path = state_path(home);
        match std::fs::read_to_string(&path) {
            Ok(content) => Ok(serde_json::from_str(&content)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Returns true if the extension at the given version is enabled.
    /// Extensions absent from the state file default to enabled.
    #[must_use]
    pub fn is_enabled(&self, ext_id: &str, version: &str) -> bool {
        let key = format!("{ext_id}@{version}");
        self.default.enabled.get(&key).copied().unwrap_or(true)
    }

    /// Set the enabled flag for an extension at a specific version.
    pub fn set_enabled(&mut self, ext_id: &str, version: &str, enabled: bool) {
        let key = format!("{ext_id}@{version}");
        self.default.enabled.insert(key, enabled);
    }

    /// Persist this state atomically to `<home>/extensions-state.json`.
    ///
    /// Uses `tmp + fsync + rename` so concurrent readers always see a
    /// complete snapshot, never a half-written file. Concurrent writers
    /// are gated by an advisory `.lock` file with bounded retries.
    pub fn save_atomic(&self, home: &Path) -> Result<(), crate::StateError> {
        let path = state_path(home);
        let content = serde_json::to_vec_pretty(self)?;
        crate::atomic::write_atomic(&path, &content)
    }
}

pub(crate) fn state_path(home: &Path) -> PathBuf {
    home.join(STATE_FILENAME)
}
