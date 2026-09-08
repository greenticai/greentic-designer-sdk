//! Post-install processing for `ProviderExtension` kind.
//!
//! After the `.gtxpack` archive is extracted to a staging directory, this
//! module verifies the embedded `.gtpack` file and copies it to the runner's
//! provider pack directory before the staging tree is committed.

use std::io::Read as _;
use std::path::Path;

use greentic_extension_sdk_contract::DescribeJson;
use sha2::{Digest, Sha256};

use crate::error::RegistryError;
use crate::hex;

/// Decode a lowercase hex string into raw bytes.
///
/// Returns `None` if `s` has odd length or contains non-hex characters.
fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

/// Provider-specific post-install step.
///
/// Run after the `.gtxpack` contents are extracted to `staging` and before
/// `commit_install` renames staging to the final extension directory.
///
/// Responsibilities:
/// 1. Verify `runtime.gtpack` is present (defensive — `TryFrom` enforces this).
/// 2. Read the staged `.gtpack` bytes and verify the SHA-256 digest.
/// 3. Conflict-check against packs in `storage_root/runtime/packs/providers/manual/`.
/// 4. Copy verified bytes to `storage_root/runtime/packs/providers/gtdx/`, where
///    the runner picks them up.
///
/// The staged `.gtpack` is deliberately LEFT in the extension tree. Step 5 used
/// to delete it, and that silently broke every gtdx-installed provider
/// extension: `build_gtxpack_with_manifest` lists *every* non-directory archive
/// entry in `manifest.json`, `runtime/provider.gtpack` included, and the
/// describe commits to that ledger via `manifest_sha256`. Deleting the file
/// left the manifest naming a path that was no longer on disk, so
/// greentic-ext-runtime's `verify_dir_manifest` — which walks every manifest
/// entry and requires each to exist — refused to load the extension with
/// `manifest lists missing file: runtime/provider.gtpack`. Operators were
/// copying the file back by hand to recover.
///
/// Not listing the file in the manifest instead is not an option: the ledger is
/// exact in both directions, so `verify_archive_against_manifest` (run on this
/// same artifact by `verify::verify_artifact`) rejects any archive entry the
/// manifest does not name. Keeping the verified copy costs one duplicate of the
/// pack on disk and keeps those bytes inside the signed integrity ledger.
///
/// Caller must invoke `Storage::abort_install` on the staging dir if this
/// returns `Err` — staging will be left populated.
///
/// On success returns the path of the `.gtpack` written into the gtdx provider
/// dir, so the caller can roll it back if a later step (e.g. `commit_install`)
/// fails — otherwise that copy would be orphaned (a partial install).
pub(crate) fn post_install_provider(
    staging: &Path,
    describe: &DescribeJson,
    storage_root: &Path,
    force: bool,
) -> Result<std::path::PathBuf, RegistryError> {
    // Step 1: at least one component must carry a gtpack distribution channel.
    let gtpack = describe
        .runtime
        .components
        .values()
        .find_map(|c| c.gtpack.as_ref())
        .ok_or_else(|| {
            RegistryError::ProviderInstall(
                "provider extension has no component with runtime.gtpack (invariant violation)"
                    .into(),
            )
        })?;

    // Step 2: Read staged bytes and verify sha256.
    let staged_path = staging.join(&gtpack.file);
    let bytes = std::fs::read(&staged_path).map_err(|e| {
        RegistryError::ProviderInstall(format!(
            "cannot read staged gtpack at {}: {e}",
            staged_path.display()
        ))
    })?;

    let actual_digest = Sha256::digest(&bytes);
    let expected_bytes = hex_decode(&gtpack.sha256).ok_or_else(|| {
        RegistryError::ProviderInstall(format!(
            "describe.json sha256 is not valid hex: {}",
            gtpack.sha256
        ))
    })?;
    // Constant-time compare for consistency with the other digest checks
    // (audit cycle-2 P3); not a remote-timing surface, but keep it uniform.
    if !constant_time_eq::constant_time_eq(actual_digest.as_slice(), expected_bytes.as_slice()) {
        return Err(RegistryError::ProviderInstall(format!(
            "sha256 mismatch: describe={}, actual={}",
            gtpack.sha256,
            hex::encode(&actual_digest)
        )));
    }

    // Step 3: Conflict check against manual packs (skipped when force=true).
    if !force {
        let manual_dir = storage_root.join("runtime/packs/providers/manual");
        if manual_dir.exists() {
            check_manual_conflict(&manual_dir, &gtpack.pack_id)?;
        }
    }

    // Step 4: Copy verified bytes to the gtdx provider directory.
    let gtdx_dir = storage_root.join("runtime/packs/providers/gtdx");
    std::fs::create_dir_all(&gtdx_dir)?;
    let dest = gtdx_dir.join(format!(
        "{}-{}.gtpack",
        describe.metadata.id, describe.metadata.version
    ));
    std::fs::write(&dest, &bytes)?;

    // The staged `.gtpack` stays where it is — see the note on this function.
    Ok(dest)
}

/// Scan `manual_dir` for `*.gtpack` files and error if any share `pack_id`.
fn check_manual_conflict(manual_dir: &Path, pack_id: &str) -> Result<(), RegistryError> {
    for entry in std::fs::read_dir(manual_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("gtpack") {
            continue;
        }
        match read_pack_id_from_gtpack(&path) {
            Ok(found_id) if found_id == pack_id => {
                return Err(RegistryError::ProviderInstall(format!(
                    "conflict: manual pack at {} has same pack_id={pack_id}; \
                     remove manually or re-run with --force",
                    path.display()
                )));
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(path = %path.display(), err = %e, "skipping unreadable manual gtpack");
            }
        }
    }
    Ok(())
}

/// Typed container for the fields we need from `manifest.cbor`.
#[derive(serde::Deserialize)]
struct ManifestHead {
    pack_id: String,
}

/// Read `pack_id` from the `manifest.cbor` ZIP entry inside a `.gtpack`.
fn read_pack_id_from_gtpack(path: &Path) -> Result<String, RegistryError> {
    let file = std::fs::File::open(path)?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| RegistryError::Storage(format!("zip open {}: {e}", path.display())))?;
    let mut entry = zip.by_name("manifest.cbor").map_err(|_| {
        RegistryError::ProviderInstall(format!(
            "gtpack at {} is missing manifest.cbor",
            path.display()
        ))
    })?;
    let mut raw = Vec::new();
    entry.read_to_end(&mut raw)?;
    let head: ManifestHead = ciborium::from_reader(raw.as_slice())
        .map_err(|e| RegistryError::ProviderInstall(format!("cbor decode: {e}")))?;
    Ok(head.pack_id)
}
