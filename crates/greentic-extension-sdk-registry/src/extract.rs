//! Staging extraction for `.gtxpack` archives, with traversal, symlink,
//! duplicate-entry and nesting guards. Split out of `lifecycle.rs` so
//! filesystem hardening lives in one reviewable place.

use std::io::{Cursor, Read as _};

use crate::error::RegistryError;
use crate::types::ExtensionArtifact;

/// Maximum directory nesting allowed for an archive entry (components of the
/// entry path, not the staging prefix). Real packs are at most a few levels
/// deep; a crafted archive with thousands of nested directories is a
/// denial-of-service attempt (inode/path-length exhaustion), not a pack.
const MAX_ENTRY_DEPTH: usize = 16;

/// Ceiling on bytes written per entry, and across the whole extraction.
///
/// Depth was capped here but size was not: `io::copy` wrote as much as the
/// entry decompressed to. `manifest.json`'s ledger caps ledgered entries, but
/// this is the one writer that runs after those checks, so a lone hostile
/// entry could still fill the disk. Mirrors the contract crate's limits.
const MAX_EXTRACT_ENTRY_BYTES: u64 = greentic_extension_sdk_contract::MAX_ENTRY_BYTES;
const MAX_EXTRACT_TOTAL_BYTES: u64 = greentic_extension_sdk_contract::MAX_ARCHIVE_BYTES;

pub(crate) fn extract_to_staging(
    artifact: &ExtensionArtifact,
    staging: &std::path::Path,
) -> Result<(), RegistryError> {
    let cursor = Cursor::new(&artifact.bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| RegistryError::Storage(format!("zip open: {e}")))?;
    let mut seen: std::collections::HashSet<std::path::PathBuf> = std::collections::HashSet::new();
    let mut total_written: u64 = 0;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| RegistryError::Storage(format!("zip entry: {e}")))?;

        // Reject symlink entries outright. The current writer (io::copy)
        // materializes them as regular files, but a symlink-aware unpacker
        // would let `describe.json -> /etc/...` or a cross-extension link
        // escape staging. Fail closed regardless of the writer (audit N7).
        if let Some(mode) = entry.unix_mode()
            && (mode & 0o17_0000) == 0o12_0000
        {
            return Err(RegistryError::Storage(format!(
                "zip entry is a symlink, refusing to extract: {}",
                entry.name()
            )));
        }

        let entry_path = entry.mangled_name();
        // Reject pathological nesting before any directory creation (DoS via
        // inode/path-length exhaustion — June-2026 audit).
        if entry_path.components().count() > MAX_ENTRY_DEPTH {
            return Err(RegistryError::Storage(format!(
                "zip entry exceeds max nesting depth of {MAX_ENTRY_DEPTH}: {}",
                entry_path.display()
            )));
        }
        let out_path = staging.join(entry_path);
        // Defense in depth: reject any entry whose resolved path
        // contains a `..` component — mangled_name() already strips
        // leading slashes and `..`, so this should never fire in practice.
        if out_path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(RegistryError::Storage(format!(
                "zip entry escapes staging: {}",
                out_path.display()
            )));
        }
        // Reject a second entry resolving to a path already written, so a
        // later entry can't overwrite an earlier (verified) one (audit N7).
        if !entry.is_dir() && !seen.insert(out_path.clone()) {
            return Err(RegistryError::Storage(format!(
                "duplicate zip entry path: {}",
                out_path.display()
            )));
        }
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = std::fs::File::create(&out_path)?;
        // Bounded copy: `take` on the reader, then check whether it stopped
        // because the entry ended or because it hit the cap.
        let written = std::io::copy(
            &mut (&mut entry).take(MAX_EXTRACT_ENTRY_BYTES + 1),
            &mut out,
        )?;
        if written > MAX_EXTRACT_ENTRY_BYTES {
            return Err(RegistryError::ArtifactTooLarge {
                limit: usize::try_from(MAX_EXTRACT_ENTRY_BYTES).unwrap_or(usize::MAX),
            });
        }
        total_written = total_written.saturating_add(written);
        if total_written > MAX_EXTRACT_TOTAL_BYTES {
            return Err(RegistryError::ArtifactTooLarge {
                limit: usize::try_from(MAX_EXTRACT_TOTAL_BYTES).unwrap_or(usize::MAX),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_extension_sdk_contract::ExtensionKind;
    use greentic_extension_sdk_testing::ExtensionFixtureBuilder;
    use std::io::Write as _;

    fn base_describe() -> greentic_extension_sdk_contract::DescribeJson {
        let fx = ExtensionFixtureBuilder::new(ExtensionKind::Design, "greentic.xtest", "1.0.0")
            .offer("greentic:i/c", "1.0.0")
            .with_wasm(vec![])
            .build()
            .unwrap();
        serde_json::from_slice(&std::fs::read(&fx.describe_path).unwrap()).unwrap()
    }

    pub(super) fn artifact(bytes: Vec<u8>) -> ExtensionArtifact {
        ExtensionArtifact {
            name: "greentic.xtest".into(),
            version: "1.0.0".into(),
            describe: base_describe(),
            bytes,
            signature: None,
        }
    }

    pub(super) fn zip_with_files(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();
        {
            let mut w = zip::ZipWriter::new(Cursor::new(&mut buf));
            for (name, body) in entries {
                w.start_file::<_, ()>(*name, zip::write::FileOptions::default())
                    .unwrap();
                w.write_all(body).unwrap();
            }
            w.finish().unwrap();
        }
        buf
    }

    #[test]
    fn extracts_flat_entries() {
        let tmp = tempfile::TempDir::new().unwrap();
        let bytes = zip_with_files(&[("a.txt", b"hello"), ("dir/b.txt", b"world")]);
        extract_to_staging(&artifact(bytes), tmp.path()).unwrap();
        assert_eq!(std::fs::read(tmp.path().join("a.txt")).unwrap(), b"hello");
        assert_eq!(
            std::fs::read(tmp.path().join("dir/b.txt")).unwrap(),
            b"world"
        );
    }

    #[test]
    fn extract_rejects_symlink_entries() {
        // A symlink entry must be refused so it can never escape staging,
        // independent of how the writer materializes it (audit N7).
        let mut buf: Vec<u8> = Vec::new();
        {
            let mut w = zip::ZipWriter::new(Cursor::new(&mut buf));
            w.add_symlink::<_, _, ()>("link", "/etc/passwd", zip::write::FileOptions::default())
                .unwrap();
            w.finish().unwrap();
        }
        let staging = tempfile::TempDir::new().unwrap();
        let err = extract_to_staging(&artifact(buf), staging.path())
            .expect_err("symlink entry must be rejected");
        assert!(matches!(err, RegistryError::Storage(m) if m.contains("symlink")));
    }

    #[test]
    fn extract_rejects_duplicate_entry_paths() {
        let mut buf: Vec<u8> = Vec::new();
        {
            let mut w = zip::ZipWriter::new(Cursor::new(&mut buf));
            let opts = zip::write::FileOptions::<()>::default();
            // Two distinct entry names that resolve to the same staging path.
            w.start_file("dup.txt", opts).unwrap();
            w.write_all(b"first").unwrap();
            w.start_file("./dup.txt", opts).unwrap();
            w.write_all(b"second").unwrap();
            w.finish().unwrap();
        }
        let staging = tempfile::TempDir::new().unwrap();
        let err = extract_to_staging(&artifact(buf), staging.path())
            .expect_err("duplicate path must be rejected");
        assert!(matches!(err, RegistryError::Storage(m) if m.contains("duplicate")));
    }

    #[test]
    fn rejects_entry_nested_too_deep() {
        let tmp = tempfile::TempDir::new().unwrap();
        let deep = (0..=MAX_ENTRY_DEPTH)
            .map(|i| format!("d{i}"))
            .collect::<Vec<_>>()
            .join("/");
        let path = format!("{deep}/f.txt");
        let bytes = zip_with_files(&[(path.as_str(), b"x")]);
        let err = extract_to_staging(&artifact(bytes), tmp.path()).unwrap_err();
        assert!(err.to_string().contains("max nesting depth"));
    }

    #[test]
    fn accepts_entry_at_max_depth() {
        let tmp = tempfile::TempDir::new().unwrap();
        // MAX_ENTRY_DEPTH components total: depth-1 dirs + file name.
        let deep = (0..MAX_ENTRY_DEPTH - 1)
            .map(|i| format!("d{i}"))
            .collect::<Vec<_>>()
            .join("/");
        let path = format!("{deep}/f.txt");
        let bytes = zip_with_files(&[(path.as_str(), b"x")]);
        extract_to_staging(&artifact(bytes), tmp.path()).unwrap();
    }
}

#[cfg(test)]
mod traversal_tests {
    use super::extract_to_staging;
    use super::tests::{artifact, zip_with_files};

    /// Zip-slip. The `Component::ParentDir` guard had **no test at all**, and
    /// neither did the absolute-path case — the two classic shapes this
    /// function exists to reject.
    #[test]
    fn rejects_entries_that_escape_the_staging_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let staging = tmp.path().join("staging");
        let outside = tmp.path().join("escaped.txt");

        for name in [
            "../escaped.txt",
            "sub/../../escaped.txt",
            "/etc/passwd",
            "..\\escaped.txt",
        ] {
            let zip = zip_with_files(&[(name, b"pwned".as_slice())]);
            // Either rejected outright, or sanitized to stay inside staging —
            // never written outside it.
            let _ = extract_to_staging(&artifact(zip), &staging);
            assert!(
                !outside.exists(),
                "entry {name:?} wrote outside the staging directory"
            );
        }
    }

    /// Depth was capped but size was not: `io::copy` wrote whatever the entry
    /// decompressed to.
    #[test]
    fn rejects_an_entry_larger_than_the_cap() {
        let tmp = tempfile::tempdir().unwrap();
        let staging = tmp.path().join("staging");

        let huge = vec![0u8; usize::try_from(super::MAX_EXTRACT_ENTRY_BYTES).unwrap() + 1];
        let zip = zip_with_files(&[("big.bin", huge.as_slice())]);

        let err = extract_to_staging(&artifact(zip), &staging).unwrap_err();
        assert!(
            matches!(err, crate::error::RegistryError::ArtifactTooLarge { .. }),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn extracts_an_ordinary_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let staging = tmp.path().join("staging");
        let zip = zip_with_files(&[("ok.txt", b"fine".as_slice())]);
        extract_to_staging(&artifact(zip), &staging).unwrap();
        assert_eq!(std::fs::read(staging.join("ok.txt")).unwrap(), b"fine");
    }
}
