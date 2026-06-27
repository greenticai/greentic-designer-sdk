use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const STATE_FILENAME: &str = "extensions-state.json";

/// Per-extension update policy, keyed by bare extension `id` (NOT `id@version`).
/// Additive in schema v1.1; absent in legacy v1.0 files (defaults apply).
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionPolicy {
    /// Cargo-like semver requirement: "^2.0", "~2.1", "=2.0.1", "*". `None` => "*".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraint: Option<String>,
    #[serde(default)]
    pub mode: UpdateMode,
    /// Set after a failed upgrade to suppress auto-retry of a broken version
    /// (manual retry still allowed). Honored by the Phase 3 reconciler.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_failed: Option<FailedUpgrade>,
}

/// `Manual` today; `Auto` is stored now but only honored from Phase 3.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateMode {
    #[default]
    Manual,
    Auto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailedUpgrade {
    pub version: String,
    pub reason: String,
}

/// Persistent enable/disable state for installed extensions.
///
/// Schema v1.0 — keys in `default.enabled` use the format `<id>@<version>`.
/// Schema v1.1 — adds `default.policies` keyed by bare `id`.
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
    /// Per-extension update policy, keyed by bare `id`. Added in schema v1.1.
    #[serde(default)]
    pub policies: HashMap<String, ExtensionPolicy>,
}

fn default_schema() -> String {
    "1.1".to_string()
}

impl ExtensionState {
    /// Load state from `<home>/extensions-state.json`. Missing file returns
    /// the default (everything enabled). Parse errors propagate.
    pub fn load(home: &Path) -> Result<Self, crate::StateError> {
        Self::read_from(&state_path(home))
    }

    /// Read and parse the state file at `path`. Missing file → default.
    fn read_from(path: &Path) -> Result<Self, crate::StateError> {
        match std::fs::read_to_string(path) {
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

    /// The update policy for `ext_id`, if one has been set.
    #[must_use]
    pub fn policy(&self, ext_id: &str) -> Option<&ExtensionPolicy> {
        self.default.policies.get(ext_id)
    }

    /// The semver constraint for `ext_id`, defaulting to `"*"` (track latest).
    #[must_use]
    pub fn constraint_for(&self, ext_id: &str) -> &str {
        self.default
            .policies
            .get(ext_id)
            .and_then(|p| p.constraint.as_deref())
            .unwrap_or("*")
    }

    /// Set (replace) the update policy for `ext_id`.
    pub fn set_policy(&mut self, ext_id: &str, policy: ExtensionPolicy) {
        self.default.policies.insert(ext_id.to_string(), policy);
    }

    /// Record a failed upgrade attempt for `ext_id`, preserving any existing
    /// constraint/mode.
    pub fn record_failed(&mut self, ext_id: &str, version: &str, reason: &str) {
        let entry = self.default.policies.entry(ext_id.to_string()).or_default();
        entry.last_failed = Some(FailedUpgrade {
            version: version.to_string(),
            reason: reason.to_string(),
        });
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

    /// Load, apply `mutate`, and persist — all while holding the write lock.
    ///
    /// This is the safe way to flip an enable/disable flag: a plain
    /// `load(); set_enabled(); save_atomic()` from two processes can interleave
    /// so the second writer clobbers the first's change (lost update). Holding
    /// the lock across the whole read-modify-write cycle serializes them.
    pub fn update<F>(home: &Path, mutate: F) -> Result<(), crate::StateError>
    where
        F: FnOnce(&mut Self),
    {
        let path = state_path(home);
        crate::atomic::with_lock(&path, || {
            let mut state = Self::read_from(&path)?;
            mutate(&mut state);
            let content = serde_json::to_vec_pretty(&state)?;
            crate::atomic::write_file_atomic(&path, &content)
        })
    }
}

pub(crate) fn state_path(home: &Path) -> PathBuf {
    home.join(STATE_FILENAME)
}
