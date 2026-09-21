//! Trust-on-first-use (TOFU) publisher key store for the `Normal` trust policy.
//!
//! On the first install of an extension we *pin* the public key that signed it.
//! Every later install of the same extension id must present the same key, so a
//! compromised registry (or a different signer) can't push a malicious update
//! under an id the user already trusts. This needs no trust root — it's the
//! protection available today while the root-of-trust (C1) is org-blocked.
//!
//! A key change is refused unless this build vouches for it as a rotation —
//! see [`crate::trust_rotation`] for the compiled-in list and why it is safe.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use fs2::FileExt;
use serde::{Deserialize, Serialize};

use crate::error::RegistryError;
use crate::trust_rotation::{KnownRotation, is_known_rotation, key_prefix};

const TRUST_DIR: &str = "trust";
const STORE_FILE: &str = "publishers.json";
const LOCK_FILE: &str = ".trust.lock";

#[derive(Debug, Default, Serialize, Deserialize)]
struct TrustData {
    /// `extension-id -> base64 ed25519 public key` pinned on first install.
    #[serde(default)]
    publishers: BTreeMap<String, String>,
}

/// Which case of [`TrustStore::pin_or_verify_outcome`] accepted the key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinOutcome {
    /// The presented key equals the pinned one.
    Matched,
    /// Nothing was pinned for this id; the presented key is now pinned.
    PinnedOnFirstUse,
    /// The pinned key rotated to the presented one through a rotation this
    /// build vouches for; the presented key is now pinned in its place.
    Rotated {
        /// The key that was pinned before this call.
        previous: String,
    },
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
        Ok(self.load()?.publishers.get(id).cloned())
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
    /// - pinned and different, and `pinned → key` is a rotation this build
    ///   vouches for ([`crate::trust_rotation::KNOWN_ROTATIONS`]) → re-pin to
    ///   `key` and accept, with a `warn!` naming the id and both keys,
    /// - pinned and different otherwise → [`RegistryError::PublisherKeyChanged`].
    ///
    /// The whole read-modify-write runs under an exclusive advisory lock so two
    /// concurrent installs can't both pin (or lose) an entry.
    pub fn pin_or_verify(&self, id: &str, key: &str) -> Result<(), RegistryError> {
        self.pin_or_verify_outcome(id, key).map(|_| ())
    }

    /// [`Self::pin_or_verify`], reporting which case applied.
    pub fn pin_or_verify_outcome(&self, id: &str, key: &str) -> Result<PinOutcome, RegistryError> {
        self.pin_or_verify_with(id, key, crate::trust_rotation::KNOWN_ROTATIONS)
    }

    fn pin_or_verify_with(
        &self,
        id: &str,
        key: &str,
        rotations: &[KnownRotation],
    ) -> Result<PinOutcome, RegistryError> {
        std::fs::create_dir_all(&self.dir)?;
        let _lock = self.acquire_lock()?;
        let mut data = self.load()?;
        match data.publishers.get(id).cloned() {
            Some(pinned) if pinned == key => Ok(PinOutcome::Matched),
            Some(pinned) if is_known_rotation(rotations, id, &pinned, key) => {
                data.publishers.insert(id.to_string(), key.to_string());
                self.save(&data)?;
                tracing::warn!(
                    name = %id,
                    pinned = %key_prefix(&pinned),
                    presented = %key_prefix(key),
                    "publisher key rotated through a rotation this build vouches for; \
                     re-pinned to the new key"
                );
                Ok(PinOutcome::Rotated { previous: pinned })
            }
            Some(pinned) => Err(RegistryError::PublisherKeyChanged {
                name: id.to_string(),
                pinned,
                presented: key.to_string(),
            }),
            None => {
                data.publishers.insert(id.to_string(), key.to_string());
                self.save(&data)?;
                Ok(PinOutcome::PinnedOnFirstUse)
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

    const OLD: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
    const NEW: &str = "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB=";
    const ROTATIONS: &[KnownRotation] = &[KnownRotation {
        id_prefix: "greentic.",
        from: OLD,
        to: NEW,
    }];

    #[test]
    fn a_listed_rotation_is_accepted_and_re_pinned() {
        let tmp = TempDir::new().unwrap();
        let store = TrustStore::new(tmp.path());
        store.pin_or_verify("greentic.a", OLD).unwrap();
        let outcome = store
            .pin_or_verify_with("greentic.a", NEW, ROTATIONS)
            .unwrap();
        assert_eq!(
            outcome,
            PinOutcome::Rotated {
                previous: OLD.to_string()
            }
        );
        assert_eq!(store.pinned("greentic.a").unwrap().as_deref(), Some(NEW));
        // The next install with the new key is a plain match, not a rotation.
        let again = store
            .pin_or_verify_with("greentic.a", NEW, ROTATIONS)
            .unwrap();
        assert_eq!(again, PinOutcome::Matched);
    }

    #[test]
    fn the_re_pin_is_persisted_atomically() {
        let tmp = TempDir::new().unwrap();
        let store = TrustStore::new(tmp.path());
        store.pin_or_verify("greentic.a", OLD).unwrap();
        store.pin_or_verify("greentic.b", OLD).unwrap();
        store
            .pin_or_verify_with("greentic.a", NEW, ROTATIONS)
            .unwrap();
        let dir = tmp.path().join(TRUST_DIR);
        // No staging file left behind; the store file is whole and parseable.
        assert!(!dir.join(STORE_FILE).with_extension("json.tmp").exists());
        let data: TrustData =
            serde_json::from_slice(&std::fs::read(dir.join(STORE_FILE)).unwrap()).unwrap();
        assert_eq!(
            data.publishers.get("greentic.a").map(String::as_str),
            Some(NEW)
        );
        // Only the id being installed moves; its sibling keeps its own pin.
        assert_eq!(
            data.publishers.get("greentic.b").map(String::as_str),
            Some(OLD)
        );
        // A fresh store instance sees the re-pin.
        assert_eq!(
            TrustStore::new(tmp.path())
                .pin_or_verify_with("greentic.a", NEW, ROTATIONS)
                .unwrap(),
            PinOutcome::Matched
        );
    }

    #[test]
    fn a_reversed_rotation_is_refused() {
        let tmp = TempDir::new().unwrap();
        let store = TrustStore::new(tmp.path());
        store.pin_or_verify("greentic.a", NEW).unwrap();
        let err = store
            .pin_or_verify_with("greentic.a", OLD, ROTATIONS)
            .unwrap_err();
        assert!(matches!(err, RegistryError::PublisherKeyChanged { .. }));
        assert_eq!(store.pinned("greentic.a").unwrap().as_deref(), Some(NEW));
    }

    #[test]
    fn once_rotated_the_old_key_is_refused() {
        let tmp = TempDir::new().unwrap();
        let store = TrustStore::new(tmp.path());
        store.pin_or_verify("greentic.a", OLD).unwrap();
        store
            .pin_or_verify_with("greentic.a", NEW, ROTATIONS)
            .unwrap();
        let err = store
            .pin_or_verify_with("greentic.a", OLD, ROTATIONS)
            .unwrap_err();
        assert!(matches!(err, RegistryError::PublisherKeyChanged { .. }));
    }

    #[test]
    fn an_unlisted_new_key_is_refused_and_the_pin_kept() {
        let tmp = TempDir::new().unwrap();
        let store = TrustStore::new(tmp.path());
        store.pin_or_verify("greentic.a", OLD).unwrap();
        let other = "CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC=";
        let err = store
            .pin_or_verify_with("greentic.a", other, ROTATIONS)
            .unwrap_err();
        assert!(matches!(err, RegistryError::PublisherKeyChanged { .. }));
        assert_eq!(store.pinned("greentic.a").unwrap().as_deref(), Some(OLD));
    }

    #[test]
    fn a_key_matching_the_listed_one_only_by_prefix_is_refused() {
        let tmp = TempDir::new().unwrap();
        let store = TrustStore::new(tmp.path());
        store.pin_or_verify("greentic.a", OLD).unwrap();
        // Same leading characters as NEW, different key.
        let look_alike = "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBA=";
        for presented in [look_alike, &NEW[..16]] {
            let err = store
                .pin_or_verify_with("greentic.a", presented, ROTATIONS)
                .unwrap_err();
            assert!(matches!(err, RegistryError::PublisherKeyChanged { .. }));
        }
        assert_eq!(store.pinned("greentic.a").unwrap().as_deref(), Some(OLD));
    }

    #[test]
    fn a_listed_rotation_does_not_reach_an_id_outside_its_prefix() {
        let tmp = TempDir::new().unwrap();
        let store = TrustStore::new(tmp.path());
        store.pin_or_verify("acme.a", OLD).unwrap();
        let err = store
            .pin_or_verify_with("acme.a", NEW, ROTATIONS)
            .unwrap_err();
        assert!(matches!(err, RegistryError::PublisherKeyChanged { .. }));
    }

    #[test]
    fn concurrent_rotations_re_pin_exactly_once() {
        let tmp = TempDir::new().unwrap();
        TrustStore::new(tmp.path())
            .pin_or_verify("greentic.a", OLD)
            .unwrap();
        let root = tmp.path().to_path_buf();
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let root = root.clone();
                std::thread::spawn(move || {
                    TrustStore::new(&root).pin_or_verify_with("greentic.a", NEW, ROTATIONS)
                })
            })
            .collect();
        let outcomes: Vec<PinOutcome> = handles
            .into_iter()
            .map(|h| h.join().unwrap().unwrap())
            .collect();
        let rotated = outcomes
            .iter()
            .filter(|o| matches!(o, PinOutcome::Rotated { .. }))
            .count();
        assert_eq!(rotated, 1, "{outcomes:?}");
        assert!(
            outcomes
                .iter()
                .all(|o| matches!(o, PinOutcome::Rotated { .. } | PinOutcome::Matched))
        );
        assert_eq!(
            TrustStore::new(&root)
                .pinned("greentic.a")
                .unwrap()
                .as_deref(),
            Some(NEW)
        );
    }

    #[test]
    fn the_public_entry_point_applies_the_shipped_rotation() {
        let tmp = TempDir::new().unwrap();
        let store = TrustStore::new(tmp.path());
        let legacy = "66L1j2dtYwoRQ2R6Bt8qkshdZmdb2IW8lLRgibxAI60=";
        let current = "oOGcH3dja4oRDTL+W1MrGEu/sQ+17d3WWl6hwNiRQAg=";
        store
            .pin_or_verify("greentic.bundle-standard", legacy)
            .unwrap();
        store
            .pin_or_verify("greentic.bundle-standard", current)
            .unwrap();
        assert_eq!(
            store.pinned("greentic.bundle-standard").unwrap().as_deref(),
            Some(current)
        );
    }
}
