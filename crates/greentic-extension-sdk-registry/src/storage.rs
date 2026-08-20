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
        if !final_dir.exists() {
            std::fs::rename(staging, final_dir)?;
            return Ok(());
        }
        // A non-directory occupying the slot is a corrupt/unexpected state; do
        // not try to swap around it — fail so the caller rolls back.
        if !std::fs::symlink_metadata(final_dir)?.is_dir() {
            return Err(RegistryError::Storage(format!(
                "install slot is not a directory: {}",
                final_dir.display()
            )));
        }
        // Replace an existing install without a window where the slot is empty:
        // move the old install aside first, swap the new one in, then delete the
        // old. If the swap fails, restore the old install so a failed upgrade
        // never leaves the extension missing (audit cycle-2 P3).
        // Append `.old` to the FULL directory name — `with_extension` would
        // truncate the last version segment (`greentic.x-1.0.0` -> `.x-1.0.old`)
        // and collide across versions. Qualify with pid + a counter so two
        // processes never share a backup path.
        let backup = {
            use std::sync::atomic::{AtomicU64, Ordering};
            static BACKUP_SEQ: AtomicU64 = AtomicU64::new(0);
            let seq = BACKUP_SEQ.fetch_add(1, Ordering::Relaxed);
            let mut name = final_dir.file_name().unwrap_or_default().to_os_string();
            name.push(format!(".{}.{seq}.old", std::process::id()));
            final_dir.with_file_name(name)
        };
        let _ = std::fs::remove_dir_all(&backup);
        std::fs::rename(final_dir, &backup)?;
        match std::fs::rename(staging, final_dir) {
            Ok(()) => {
                let _ = std::fs::remove_dir_all(&backup);
                Ok(())
            }
            Err(e) => {
                // The rollback used to be `let _ = ...`, discarding exactly the
                // case this code exists to prevent: if the restore also fails,
                // the old install is stranded at the backup path and
                // `final_dir` does not exist — the extension is simply gone,
                // and the returned error said nothing about it or where the
                // backup went.
                if let Err(restore_err) = std::fs::rename(&backup, final_dir) {
                    return Err(RegistryError::Storage(format!(
                        "install swap failed ({e}) and the rollback also failed \
                         ({restore_err}); the previous install is preserved at {} — \
                         restore it with: mv {} {}",
                        backup.display(),
                        backup.display(),
                        final_dir.display()
                    )));
                }
                Err(e.into())
            }
        }
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
