//! Post-extraction check: the tree about to be committed holds every file the
//! archive's `manifest.json` ledger lists, byte for byte.
//!
//! `verify_integrity` proves the ARCHIVE matches its ledger before anything is
//! written. It says nothing about what is on disk once extraction and the
//! per-kind post-install steps have run — and the designer's runtime checks
//! exactly that: it stats every ledger path in the installed directory and
//! refuses an extension missing any of them ("manifest lists missing file:
//! …"). A post-install step that removed a ledger file therefore produced an
//! install gtdx reported as successful and the designer would never load.
//! Checking the staged tree here turns that into a failed install that names
//! the path, before the broken tree is committed.

use std::path::{Component, Path};

use greentic_extension_sdk_contract::manifest::{MANIFEST_ENTRY_NAME, Manifest};
use sha2::{Digest, Sha256};

use crate::error::RegistryError;

/// Verify every `manifest.json` entry exists in `staging` as a regular file
/// with the recorded size and sha256.
///
/// A staged tree with no `manifest.json` is not checked here: whether an
/// archive must carry a ledger is `verify_integrity`'s decision, and the
/// fetch-and-install path already refuses one without it.
pub(crate) fn verify_staged_tree(staging: &Path) -> Result<(), RegistryError> {
    let manifest_path = staging.join(MANIFEST_ENTRY_NAME);
    let raw = match std::fs::read(&manifest_path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    let manifest: Manifest = serde_json::from_slice(&raw).map_err(|e| {
        RegistryError::Storage(format!("installed manifest.json is unreadable: {e}"))
    })?;

    for entry in &manifest.entries {
        let rel = Path::new(&entry.path);
        if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
            return Err(RegistryError::Storage(format!(
                "manifest.json lists a path outside the extension directory: {}",
                entry.path
            )));
        }
        let on_disk = staging.join(rel);
        let bytes = match std::fs::read(&on_disk) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(RegistryError::Storage(format!(
                    "install incomplete: manifest.json lists {} but it was not written to the \
                     extension directory",
                    entry.path
                )));
            }
            Err(e) => {
                return Err(RegistryError::Storage(format!(
                    "install incomplete: cannot read {} listed in manifest.json: {e}",
                    entry.path
                )));
            }
        };
        if bytes.len() as u64 != entry.size {
            return Err(RegistryError::Storage(format!(
                "install incomplete: {} is {} bytes on disk, manifest.json records {}",
                entry.path,
                bytes.len(),
                entry.size
            )));
        }
        let computed = crate::hex::encode(&Sha256::digest(&bytes));
        if !computed.eq_ignore_ascii_case(&entry.sha256) {
            return Err(RegistryError::Storage(format!(
                "install incomplete: {} on disk hashes to {computed}, manifest.json records {}",
                entry.path, entry.sha256
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_extension_sdk_contract::build_manifest;

    fn stage(files: &[(&str, &[u8])]) -> tempfile::TempDir {
        let tmp = tempfile::TempDir::new().unwrap();
        for (path, body) in files {
            let p = tmp.path().join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        }
        let manifest = build_manifest(files.iter().map(|(p, b)| (*p, *b)));
        std::fs::write(
            tmp.path().join(MANIFEST_ENTRY_NAME),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        tmp
    }

    #[test]
    fn a_complete_tree_passes() {
        let tmp = stage(&[
            ("extension.wasm", b"wasm"),
            ("runtime/provider.gtpack", b"pack"),
        ]);
        verify_staged_tree(tmp.path()).unwrap();
    }

    #[test]
    fn a_missing_ledger_file_fails_naming_the_path() {
        let tmp = stage(&[
            ("extension.wasm", b"wasm"),
            ("runtime/provider.gtpack", b"pack"),
        ]);
        std::fs::remove_file(tmp.path().join("runtime/provider.gtpack")).unwrap();
        let err = verify_staged_tree(tmp.path()).unwrap_err().to_string();
        assert!(err.contains("runtime/provider.gtpack"), "got: {err}");
    }

    #[test]
    fn a_changed_ledger_file_fails() {
        let tmp = stage(&[("extension.wasm", b"wasm")]);
        std::fs::write(tmp.path().join("extension.wasm"), b"WASM").unwrap();
        let err = verify_staged_tree(tmp.path()).unwrap_err().to_string();
        assert!(err.contains("extension.wasm"), "got: {err}");
    }

    #[test]
    fn a_tree_without_a_manifest_is_not_checked_here() {
        let tmp = tempfile::TempDir::new().unwrap();
        verify_staged_tree(tmp.path()).unwrap();
    }
}
