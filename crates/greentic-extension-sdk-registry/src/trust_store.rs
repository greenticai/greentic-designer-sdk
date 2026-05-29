//! Trust-on-first-use (TOFU) publisher key store for the `Normal` trust policy.
//!
//! On the first install of an extension we *pin* the public key that signed it.
//! Every later install of the same extension id must present the same key, so a
//! compromised registry (or a different signer) can't push a malicious update
//! under an id the user already trusts. This needs no trust root — it's the
//! protection available today while the root-of-trust (C1) is org-blocked.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use fs2::FileExt;
use serde::{Deserialize, Serialize};

use crate::error::RegistryError;

const TRUST_DIR: &str = "trust";
const STORE_FILE: &str = "publishers.json";
const LOCK_FILE: &str = ".trust.lock";

#[derive(Debug, Default, Serialize, Deserialize)]
struct TrustData {
    /// `extension-id -> base64 ed25519 public key` pinned on first install.
    #[serde(default)]
    publishers: BTreeMap<String, String>,
}

/// Persistent TOFU store at `<root>/trust/publishers.json`.
pub struct TrustStore {
    dir: PathBuf,
}

impl TrustStore {
    #[must_use]
    pub fn new(root: &Path) -> Self {
        Self {
            dir: root.join(TRUST_DIR),
        }
    }

    fn store_path(&self) -> PathBuf {
        self.dir.join(STORE_FILE)
    }

    /// Return the pinned key for `id`, if any.
    pub fn pinned(&self, id: &str) -> Result<Option<String>, RegistryError> {
        Ok(self.load()?.publishers.remove(id))
    }

    /// Read-only check (used by `Strict`): is `key` the trusted key for `id`?
    /// Unlike [`Self::pin_or_verify`] this never pins — under Strict an unknown
    /// publisher must be rejected, not trusted on first use.
    pub fn is_trusted(&self, id: &str, key: &str) -> Result<bool, RegistryError> {
        Ok(self.pinned(id)?.as_deref() == Some(key))
    }

    /// TOFU check for `id`:
    /// - not pinned yet → pin `key` and accept (first use),
    /// - pinned and equal → accept,
    /// - pinned and different → [`RegistryError::PublisherKeyChanged`].
    ///
    /// The whole read-modify-write runs under an exclusive advisory lock so two
    /// concurrent installs can't both pin (or lose) an entry.
    pub fn pin_or_verify(&self, id: &str, key: &str) -> Result<(), RegistryError> {
        std::fs::create_dir_all(&self.dir)?;
        let _lock = self.acquire_lock()?;
        let mut data = self.load()?;
        match data.publishers.get(id) {
            Some(pinned) if pinned == key => Ok(()),
            Some(pinned) => Err(RegistryError::PublisherKeyChanged {
                name: id.to_string(),
                pinned: pinned.clone(),
                presented: key.to_string(),
            }),
            None => {
                data.publishers.insert(id.to_string(), key.to_string());
                self.save(&data)
            }
        }
    }

    fn load(&self) -> Result<TrustData, RegistryError> {
        match std::fs::read(self.store_path()) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(TrustData::default()),
            Err(e) => Err(e.into()),
        }
    }

    fn save(&self, data: &TrustData) -> Result<(), RegistryError> {
        let path = self.store_path();
        let tmp = path.with_extension("json.tmp");
        let write = || -> Result<(), RegistryError> {
            let mut f = File::create(&tmp)?;
            f.write_all(&serde_json::to_vec_pretty(data)?)?;
            f.sync_all()?;
            drop(f);
            std::fs::rename(&tmp, &path)?;
            Ok(())
        };
        let result = write();
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        result
    }

    /// Exclusive advisory lock on a persistent `<dir>/.trust.lock`. The lock
    /// file is intentionally never deleted (deleting it would race another
    /// holder on the same inode). Released when the returned handle drops.
    fn acquire_lock(&self) -> Result<File, RegistryError> {
        let lock_path = self.dir.join(LOCK_FILE);
        let file = File::options()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)?;
        file.lock_exclusive()
            .map_err(|e| RegistryError::Storage(format!("lock {}: {e}", lock_path.display())))?;
        Ok(file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn first_use_pins_and_accepts() {
        let tmp = TempDir::new().unwrap();
        let store = TrustStore::new(tmp.path());
        store.pin_or_verify("ext.a", "KEY1").unwrap();
        assert_eq!(store.pinned("ext.a").unwrap().as_deref(), Some("KEY1"));
    }

    #[test]
    fn same_key_accepted_on_reinstall() {
        let tmp = TempDir::new().unwrap();
        let store = TrustStore::new(tmp.path());
        store.pin_or_verify("ext.a", "KEY1").unwrap();
        store.pin_or_verify("ext.a", "KEY1").unwrap();
    }

    #[test]
    fn different_key_is_rejected() {
        let tmp = TempDir::new().unwrap();
        let store = TrustStore::new(tmp.path());
        store.pin_or_verify("ext.a", "KEY1").unwrap();
        let err = store.pin_or_verify("ext.a", "KEY2").unwrap_err();
        assert!(
            matches!(err, RegistryError::PublisherKeyChanged { .. }),
            "got {err}"
        );
    }

    #[test]
    fn pin_persists_across_store_instances() {
        let tmp = TempDir::new().unwrap();
        TrustStore::new(tmp.path())
            .pin_or_verify("ext.a", "KEY1")
            .unwrap();
        // A fresh store at the same root must see the pin.
        let err = TrustStore::new(tmp.path())
            .pin_or_verify("ext.a", "KEY2")
            .unwrap_err();
        assert!(matches!(err, RegistryError::PublisherKeyChanged { .. }));
    }

    #[test]
    fn is_trusted_only_after_pin_and_only_for_matching_key() {
        let tmp = TempDir::new().unwrap();
        let store = TrustStore::new(tmp.path());
        assert!(!store.is_trusted("ext.a", "KEY1").unwrap()); // unknown → not trusted
        store.pin_or_verify("ext.a", "KEY1").unwrap();
        assert!(store.is_trusted("ext.a", "KEY1").unwrap()); // pinned key → trusted
        assert!(!store.is_trusted("ext.a", "KEY2").unwrap()); // other key → not trusted
    }

    #[test]
    fn distinct_extensions_pin_independently() {
        let tmp = TempDir::new().unwrap();
        let store = TrustStore::new(tmp.path());
        store.pin_or_verify("ext.a", "KEY1").unwrap();
        store.pin_or_verify("ext.b", "KEY2").unwrap();
        assert_eq!(store.pinned("ext.a").unwrap().as_deref(), Some("KEY1"));
        assert_eq!(store.pinned("ext.b").unwrap().as_deref(), Some("KEY2"));
    }
}
