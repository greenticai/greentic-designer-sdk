use std::path::{Path, PathBuf};

use greentic_extension_sdk_contract::ExtensionKind;

use crate::error::RegistryError;

#[derive(Debug, Clone)]
pub struct Storage {
    root: PathBuf,
}

impl Storage {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    #[must_use]
    pub fn root(&self) -> &std::path::Path {
        &self.root
    }

    #[must_use]
    pub fn clone_shallow(&self) -> Self {
        Self {
            root: self.root.clone(),
        }
    }

    #[must_use]
    pub fn extensions_root(&self) -> PathBuf {
        self.root.join("extensions")
    }

    #[must_use]
    pub fn kind_dir(&self, kind: ExtensionKind) -> PathBuf {
        self.extensions_root().join(kind.dir_name())
    }

    #[must_use]
    pub fn extension_dir(&self, kind: ExtensionKind, name: &str, version: &str) -> PathBuf {
        self.kind_dir(kind).join(format!("{name}-{version}"))
    }

    #[must_use]
    pub fn registry_json(&self) -> PathBuf {
        self.root.join("registry.json")
    }

    /// Creates a unique `.tmp` staging directory next to the final install
    /// path. Caller writes into staging, then calls [`commit_install`].
    ///
    /// The staging name carries a per-process pid + atomic counter so two
    /// concurrent installs of the same extension never share (or delete) each
    /// other's staging dir.
    pub fn begin_install(
        &self,
        kind: ExtensionKind,
        name: &str,
        version: &str,
    ) -> Result<(PathBuf, PathBuf), RegistryError> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static STAGING_SEQ: AtomicU64 = AtomicU64::new(0);

        let final_dir = self.extension_dir(kind, name, version);
        let seq = STAGING_SEQ.fetch_add(1, Ordering::Relaxed);
        let staging = self
            .kind_dir(kind)
            .join(format!("{name}-{version}.{}.{seq}.tmp", std::process::id()));
        std::fs::create_dir_all(&staging)?;
        Ok((staging, final_dir))
    }

    pub fn commit_install(&self, staging: &Path, final_dir: &Path) -> Result<(), RegistryError> {
        if let Some(parent) = final_dir.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if final_dir.exists() {
            std::fs::remove_dir_all(final_dir)?;
        }
        std::fs::rename(staging, final_dir)?;
        Ok(())
    }

    pub fn abort_install(&self, staging: &Path) {
        let _ = std::fs::remove_dir_all(staging);
    }

    pub fn remove_extension(
        &self,
        kind: ExtensionKind,
        name: &str,
        version: &str,
    ) -> Result<(), RegistryError> {
        let dir = self.extension_dir(kind, name, version);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        Ok(())
    }
}
