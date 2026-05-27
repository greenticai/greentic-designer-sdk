//! `manifest.json` — whole-archive integrity ledger for `.gtxpack`.
//!
//! Audit P0 #2: signing only `describe.json` (JCS) leaves the WASM binary
//! and every other entry unsigned. An attacker who can rewrite a `.gtxpack`
//! in transit (or a malicious mirror) can swap `extension.wasm` while
//! re-pointing `describe.metadata.artifact_sha256` — verification still
//! passes because `verify_describe()` never looks at the archive contents.
//!
//! The fix: enumerate every archive entry (excluding `manifest.json` itself)
//! with its sha256 + byte length in a sorted ledger. Sign
//! `serde_jcs::to_vec(&manifest)`. Verification:
//!   1. read `manifest.json` from the archive
//!   2. verify its signature against the publisher cert (D.5)
//!   3. recompute every entry's sha256 + compare against the manifest
//!
//! Any tampered file flips its sha256 → verification fails fast.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Per-entry uncompressed byte cap (64 MiB) — defends against decompression
/// bombs. Generous enough for any real extension wasm/asset.
pub const MAX_ENTRY_BYTES: u64 = 64 * 1024 * 1024;
/// Whole-archive uncompressed byte cap (256 MiB).
pub const MAX_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    /// Schema discriminator. Hard-coded `"greentic.gtxpack.manifest/v1"`.
    pub schema: String,
    /// Sorted-by-path list of every entry except `manifest.json` itself.
    pub entries: Vec<ManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestEntry {
    pub path: String,
    pub sha256: String,
    pub size: u64,
}

pub const MANIFEST_ENTRY_NAME: &str = "manifest.json";
pub const DESCRIBE_ENTRY_NAME: &str = "describe.json";
pub const MANIFEST_SCHEMA_V1: &str = "greentic.gtxpack.manifest/v1";

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("manifest missing from archive")]
    Missing,
    #[error("entry '{0}' not in manifest")]
    UnexpectedEntry(String),
    #[error("entry '{path}' sha256 mismatch: expected {expected}, computed {computed}")]
    ShaMismatch {
        path: String,
        expected: String,
        computed: String,
    },
    #[error("entry '{path}' present in manifest but absent from archive")]
    MissingEntry { path: String },
    #[error("manifest schema unsupported: {0}")]
    UnsupportedSchema(String),
    #[error("entry '{path}' declared size {size} exceeds cap {cap}")]
    EntryTooLarge { path: String, size: u64, cap: u64 },
    #[error("archive uncompressed size exceeds cap {cap}")]
    ArchiveTooLarge { cap: u64 },
}

/// Build a [`Manifest`] from the entries that will be (or are) inside a
/// `.gtxpack`. Paths are sorted lexicographically. `manifest.json` and any
/// trailing-slash directory markers are excluded.
#[must_use]
pub fn build_manifest<I, S, B>(entries: I) -> Manifest
where
    I: IntoIterator<Item = (S, B)>,
    S: AsRef<str>,
    B: AsRef<[u8]>,
{
    let mut rows: Vec<ManifestEntry> = entries
        .into_iter()
        .filter_map(|(p, b)| {
            let p = p.as_ref();
            if p == MANIFEST_ENTRY_NAME || p == DESCRIBE_ENTRY_NAME || p.ends_with('/') {
                return None;
            }
            let body = b.as_ref();
            let mut hasher = Sha256::new();
            hasher.update(body);
            Some(ManifestEntry {
                path: p.to_string(),
                sha256: format!("{:x}", hasher.finalize()),
                size: body.len() as u64,
            })
        })
        .collect();
    rows.sort_by(|a, b| a.path.cmp(&b.path));
    Manifest {
        schema: MANIFEST_SCHEMA_V1.to_string(),
        entries: rows,
    }
}

/// Verify a `.gtxpack` byte stream against an in-archive `manifest.json`.
///
/// Returns `Ok(())` when every non-manifest entry hashes to the value the
/// manifest records and no extra files are present. The publisher signature
/// over the manifest itself is checked separately (see D.5 trust root).
///
/// # Errors
///
/// Returns the relevant [`ManifestError`] variant on any of: missing
/// `manifest.json`, unsupported schema string, sha256 mismatch on any
/// entry, smuggled entry not listed in the manifest, or manifest entry
/// absent from the archive.
pub fn verify_archive_against_manifest(zip_bytes: &[u8]) -> Result<(), ManifestError> {
    use std::io::Read;

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes))?;

    // Read manifest.json with an uncompressed size guard to prevent OOM from a
    // crafted manifest entry before we even start verifying real entries.
    let manifest: Manifest = {
        let mut f = archive
            .by_name(MANIFEST_ENTRY_NAME)
            .map_err(|_| ManifestError::Missing)?;
        if f.size() > MAX_ENTRY_BYTES {
            return Err(ManifestError::EntryTooLarge {
                path: MANIFEST_ENTRY_NAME.to_string(),
                size: f.size(),
                cap: MAX_ENTRY_BYTES,
            });
        }
        let mut body = Vec::new();
        f.by_ref()
            .take(MAX_ENTRY_BYTES + 1)
            .read_to_end(&mut body)?;
        serde_json::from_slice(&body)
            .map_err(|e| ManifestError::UnsupportedSchema(format!("parse: {e}")))?
    };
    if manifest.schema != MANIFEST_SCHEMA_V1 {
        return Err(ManifestError::UnsupportedSchema(manifest.schema));
    }

    // Build an O(1) lookup from path → manifest row. Constructed once before
    // iterating archive entries to avoid O(n²) linear scans (audit M1).
    let lookup: std::collections::BTreeMap<&str, &ManifestEntry> = manifest
        .entries
        .iter()
        .map(|r| (r.path.as_str(), r))
        .collect();

    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut total_uncompressed: u64 = 0;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();

        // describe.json is excluded from the manifest ledger on purpose: it
        // carries its own JCS publisher signature and binds the manifest via
        // describe.manifestSha256, so listing it here would be circular
        // (the signed describe references a manifest that references describe).
        if name == MANIFEST_ENTRY_NAME || name == DESCRIBE_ENTRY_NAME || entry.is_dir() {
            continue;
        }

        let row = lookup
            .get(name.as_str())
            .ok_or_else(|| ManifestError::UnexpectedEntry(name.clone()))?;

        // Guard 1: zip header declares an uncompressed size that exceeds the cap.
        // Lying headers are also caught by Guard 3 below, but checking here
        // avoids even starting an expensive decompression.
        if entry.size() > MAX_ENTRY_BYTES {
            return Err(ManifestError::EntryTooLarge {
                path: name,
                size: entry.size(),
                cap: MAX_ENTRY_BYTES,
            });
        }

        // Guard 2: manifest-recorded size exceeds the cap. Catches tampered /
        // maliciously crafted manifests that declare enormous sizes before the
        // archive is decompressed (audit H2).
        if row.size > MAX_ENTRY_BYTES {
            return Err(ManifestError::EntryTooLarge {
                path: name,
                size: row.size,
                cap: MAX_ENTRY_BYTES,
            });
        }

        // Guard 3: accumulate uncompressed bytes across all entries (saturating
        // to avoid wrapping) and reject if the whole archive would exceed the cap.
        total_uncompressed = total_uncompressed.saturating_add(entry.size());
        if total_uncompressed > MAX_ARCHIVE_BYTES {
            return Err(ManifestError::ArchiveTooLarge {
                cap: MAX_ARCHIVE_BYTES,
            });
        }

        // Guard 4: bounded decompression read catches lying zip headers that
        // claim a small uncompressed size but expand beyond the cap.
        let mut body = Vec::new();
        entry
            .by_ref()
            .take(MAX_ENTRY_BYTES + 1)
            .read_to_end(&mut body)?;
        if body.len() as u64 > MAX_ENTRY_BYTES {
            return Err(ManifestError::EntryTooLarge {
                path: name,
                size: body.len() as u64,
                cap: MAX_ENTRY_BYTES,
            });
        }

        let computed = {
            let mut hasher = Sha256::new();
            hasher.update(&body);
            format!("{:x}", hasher.finalize())
        };
        if computed != row.sha256 {
            return Err(ManifestError::ShaMismatch {
                path: name,
                expected: row.sha256.clone(),
                computed,
            });
        }
        seen.insert(name);
    }

    for row in &manifest.entries {
        if !seen.contains(&row.path) {
            return Err(ManifestError::MissingEntry {
                path: row.path.clone(),
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();
        {
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
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
    fn build_manifest_excludes_describe_json() {
        let m = build_manifest(vec![
            ("describe.json", &br#"{"k":1}"#[..]),
            ("extension.wasm", &b"\0asm"[..]),
            ("manifest.json", &b"{}"[..]),
        ]);
        let paths: Vec<&str> = m.entries.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(
            paths,
            vec!["extension.wasm"],
            "describe.json + manifest.json excluded"
        );
    }

    #[test]
    fn build_manifest_sorts_entries_and_excludes_self() {
        let m = build_manifest(vec![
            ("z.md", &b"alpha"[..]),
            ("a.wasm", &b"\0asm\x01\x00\x00\x00"[..]),
            ("manifest.json", &b"{}"[..]),
            ("describe.json", &br#"{"k":1}"#[..]),
        ]);
        let paths: Vec<&str> = m.entries.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, vec!["a.wasm", "z.md"]);
    }

    #[test]
    fn verify_passes_on_intact_archive() {
        let entries: Vec<(&str, &[u8])> = vec![
            ("describe.json", b"{\"k\":1}"),
            ("extension.wasm", b"\0asm\x01\x00\x00\x00"),
        ];
        let manifest = build_manifest(entries.iter().map(|(p, b)| (*p, *b)));
        let manifest_json = serde_json::to_vec(&manifest).unwrap();
        let mut with_manifest = entries.clone();
        with_manifest.push(("manifest.json", &manifest_json));
        let bytes = build_zip(&with_manifest);
        verify_archive_against_manifest(&bytes).unwrap();
    }

    #[test]
    fn verify_fails_when_wasm_tampered() {
        let entries: Vec<(&str, &[u8])> = vec![
            ("describe.json", b"{\"k\":1}"),
            ("extension.wasm", b"\0asm\x01\x00\x00\x00"),
        ];
        let manifest = build_manifest(entries.iter().map(|(p, b)| (*p, *b)));
        let manifest_json = serde_json::to_vec(&manifest).unwrap();
        let with_manifest: Vec<(&str, &[u8])> = vec![
            ("describe.json", b"{\"k\":1}"),
            ("extension.wasm", b"\0asm\x01\x00\x00\xff"),
            ("manifest.json", &manifest_json),
        ];
        let bytes = build_zip(&with_manifest);
        let err = verify_archive_against_manifest(&bytes).unwrap_err();
        assert!(
            matches!(err, ManifestError::ShaMismatch { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn verify_fails_when_extra_file_smuggled_in() {
        let entries: Vec<(&str, &[u8])> = vec![("describe.json", b"{\"k\":1}")];
        let manifest = build_manifest(entries.iter().map(|(p, b)| (*p, *b)));
        let manifest_json = serde_json::to_vec(&manifest).unwrap();
        let with_manifest: Vec<(&str, &[u8])> = vec![
            ("describe.json", b"{\"k\":1}"),
            ("backdoor.wasm", b"evil"),
            ("manifest.json", &manifest_json),
        ];
        let bytes = build_zip(&with_manifest);
        let err = verify_archive_against_manifest(&bytes).unwrap_err();
        assert!(
            matches!(err, ManifestError::UnexpectedEntry(_)),
            "got {err:?}"
        );
    }

    #[test]
    fn verify_fails_when_manifest_missing() {
        let entries: Vec<(&str, &[u8])> = vec![("describe.json", b"{\"k\":1}")];
        let bytes = build_zip(&entries);
        let err = verify_archive_against_manifest(&bytes).unwrap_err();
        assert!(matches!(err, ManifestError::Missing));
    }

    #[test]
    fn verify_fails_when_manifest_lists_missing_entry() {
        let mut manifest = build_manifest(vec![
            ("describe.json", &b"{\"k\":1}"[..]),
            ("extension.wasm", &b"\0asm\x01\x00\x00\x00"[..]),
        ]);
        // Inject a phantom entry the archive will not contain.
        manifest.entries.push(ManifestEntry {
            path: "ghost.txt".to_string(),
            sha256: "0".repeat(64),
            size: 0,
        });
        manifest.entries.sort_by(|a, b| a.path.cmp(&b.path));
        let manifest_json = serde_json::to_vec(&manifest).unwrap();
        let with_manifest: Vec<(&str, &[u8])> = vec![
            ("describe.json", b"{\"k\":1}"),
            ("extension.wasm", b"\0asm\x01\x00\x00\x00"),
            ("manifest.json", &manifest_json),
        ];
        let bytes = build_zip(&with_manifest);
        let err = verify_archive_against_manifest(&bytes).unwrap_err();
        assert!(
            matches!(err, ManifestError::MissingEntry { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn verify_rejects_oversize_declared_entry() {
        let huge = MAX_ENTRY_BYTES + 1;
        let mut manifest = build_manifest(vec![("extension.wasm", &b"\0asm"[..])]);
        manifest.entries[0].size = huge;
        let manifest_json = serde_json::to_vec(&manifest).unwrap();
        let with_manifest: Vec<(&str, &[u8])> = vec![
            ("extension.wasm", b"\0asm"),
            ("manifest.json", &manifest_json),
        ];
        let bytes = build_zip(&with_manifest);
        let err = verify_archive_against_manifest(&bytes).unwrap_err();
        assert!(
            matches!(err, ManifestError::EntryTooLarge { .. }),
            "got {err:?}"
        );
    }

    /// Regression guard: `verify_archive_against_manifest` must tolerate any
    /// modification to describe.json without failing. describe.json integrity
    /// is protected by its own JCS publisher signature — re-including it in
    /// the manifest ledger would create a circular dependency (signed describe
    /// references a manifest that references describe). This test locks in
    /// that exclusion: if a future refactor accidentally re-adds describe.json
    /// to the manifest, the second zip (with different describe bytes) will
    /// produce a sha256 mismatch and the assertion will catch it.
    #[test]
    fn verify_tolerates_modified_describe_json() {
        let wasm_bytes: &[u8] = b"\0asm\x01\x00\x00\x00";
        let describe_v1: &[u8] = b"{\"version\":1,\"id\":\"my-ext\"}";
        let describe_v2: &[u8] = b"{\"version\":2,\"id\":\"my-ext\",\"extra\":true}";

        // Build manifest from entries that should be covered (wasm only —
        // describe.json and manifest.json are excluded by build_manifest).
        let manifest = build_manifest(vec![("extension.wasm", wasm_bytes)]);
        let manifest_json = serde_json::to_vec(&manifest).unwrap();

        // Zip A: describe.json has v1 bytes.
        let zip_a = build_zip(&[
            ("describe.json", describe_v1),
            ("extension.wasm", wasm_bytes),
            ("manifest.json", manifest_json.as_slice()),
        ]);

        // Zip B: describe.json has v2 bytes (everything else identical).
        let zip_b = build_zip(&[
            ("describe.json", describe_v2),
            ("extension.wasm", wasm_bytes),
            ("manifest.json", manifest_json.as_slice()),
        ]);

        assert!(
            verify_archive_against_manifest(&zip_a).is_ok(),
            "zip_a (describe v1) should pass manifest verification"
        );
        assert!(
            verify_archive_against_manifest(&zip_b).is_ok(),
            "zip_b (describe v2) should pass manifest verification — \
             describe.json bytes must not affect the manifest ledger"
        );
    }
}
