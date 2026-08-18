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
/// 4. Copy verified bytes to `storage_root/runtime/packs/providers/gtdx/`.
/// 5. Remove the `.gtpack` file (and empty parent dirs) from staging so it
///    does not end up in the final `extensions/provider/{id}-{version}/` tree.
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
    let staged_path = staged_pack_path(staging, &gtpack.file)?;
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

    // Step 5: Remove the gtpack from staging so it is not committed to
    //         the extensions tree.
    std::fs::remove_file(&staged_path)?;
    // Remove now-empty parent directories (best-effort; ignore errors).
    remove_empty_ancestors(&staged_path, staging);

    Ok(dest)
}

/// Resolve `file` (from `describe.runtime.components.*.gtpack.file`) against
/// the staging dir, rejecting anything that would escape it.
///
/// `file` is publisher-controlled: it arrives verbatim from a downloaded
/// describe, and the v2 schema constrains `gtpack` only to `type: object`.
/// Without this guard `staging.join(file)` reads, copies, and then *deletes*
/// an arbitrary path — `Path::join` discards the base entirely when handed an
/// absolute path, so `/etc/passwd` escapes without needing a single `..`.
///
/// This mirrors the check `gtdx`'s packer already applies on the author side
/// (`dev/packer/mod.rs`); the consumer side is the one facing hostile input.
fn staged_pack_path(staging: &Path, file: &str) -> Result<std::path::PathBuf, RegistryError> {
    use std::path::Component;

    let candidate = Path::new(file);
    let bad = |why: &str| {
        RegistryError::ProviderInstall(format!(
            "describe runtime gtpack.file = {file:?} must be a staging-relative path \
             with no `..` or absolute components ({why})"
        ))
    };

    if file.is_empty() {
        return Err(bad("empty"));
    }
    if candidate.is_absolute() {
        return Err(bad("absolute"));
    }
    for component in candidate.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir => return Err(bad("contains `..`")),
            Component::RootDir | Component::Prefix(_) => return Err(bad("rooted")),
        }
    }

    let resolved = staging.join(candidate);
    // Defence in depth: the component scan above should make this unreachable,
    // but a lexical containment check costs nothing and fails closed.
    if !resolved.starts_with(staging) {
        return Err(bad("escapes the staging directory"));
    }
    Ok(resolved)
}

/// Walk upward from `removed_file`'s parent toward (but not including) `stop`
/// and remove any empty directories encountered.
///
/// Every candidate must live under `stop`. The old `dir == stop` terminator
/// alone was only correct while `removed_file` was known to be inside `stop`;
/// if that ever stopped holding, the walk climbed toward `/` deleting empty
/// directories it had no business touching.
fn remove_empty_ancestors(removed_file: &Path, stop: &Path) {
    let mut current = removed_file.parent();
    while let Some(dir) = current {
        if dir == stop || !dir.starts_with(stop) {
            break;
        }
        // `remove_dir` succeeds only if the directory is empty.
        if std::fs::remove_dir(dir).is_err() {
            break;
        }
        current = dir.parent();
    }
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

#[cfg(test)]
mod path_guard_tests {
    use super::*;

    fn staging() -> &'static Path {
        Path::new("gtdx-staging-root")
    }

    /// The reason this guard has to exist at all: `Path::join` with an
    /// absolute argument silently discards the base, so an unvalidated
    /// `staging.join(&gtpack.file)` escapes staging without needing `..`.
    #[test]
    // Joining an absolute path is exactly what this test documents: the lint
    // fires on the very footgun the guard above exists to prevent.
    #[allow(clippy::join_absolute_paths)]
    fn join_with_absolute_path_discards_the_base() {
        assert_eq!(
            staging().join("/etc/passwd"),
            Path::new("/etc/passwd"),
            "std behaviour changed; the guard's rationale needs revisiting"
        );
    }

    #[test]
    fn accepts_a_plain_relative_pack_path() {
        let got = staged_pack_path(staging(), "packs/main.gtpack").unwrap();
        assert_eq!(got, staging().join("packs/main.gtpack"));
    }

    #[test]
    fn rejects_absolute_paths() {
        let err = staged_pack_path(staging(), "/home/victim/.aws/credentials").unwrap_err();
        assert!(
            matches!(err, RegistryError::ProviderInstall(ref m) if m.contains("must be a staging-relative path")),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn rejects_parent_dir_traversal() {
        for candidate in [
            "../escape.gtpack",
            "packs/../../escape.gtpack",
            "./../escape.gtpack",
        ] {
            let err = staged_pack_path(staging(), candidate).unwrap_err();
            assert!(
                matches!(err, RegistryError::ProviderInstall(_)),
                "{candidate} should have been rejected, got {err}"
            );
        }
    }

    #[test]
    fn rejects_empty_path() {
        assert!(staged_pack_path(staging(), "").is_err());
    }

    /// `remove_empty_ancestors` must never climb above `stop`, even if it is
    /// somehow handed a path outside it.
    #[test]
    fn remove_empty_ancestors_never_climbs_outside_stop() {
        let tmp = tempfile::tempdir().unwrap();
        let stop = tmp.path().join("staging");
        let outside = tmp.path().join("outside/deep");
        std::fs::create_dir_all(&stop).unwrap();
        std::fs::create_dir_all(&outside).unwrap();

        remove_empty_ancestors(&outside.join("victim.gtpack"), &stop);

        assert!(
            outside.exists(),
            "walked outside `stop` and removed an unrelated empty directory"
        );
    }
}
