//! `manifest.json` — whole-archive integrity ledger for `.gtxpack`.
//!
//! Audit P0 #2: signing only `describe.json` (JCS) leaves the WASM binary
//! and every other entry unsigned. An attacker who can rewrite a `.gtxpack`
//! in transit (or a malicious mirror) can swap `extension.wasm` while
//! re-pointing `describe.metadata.artifact_sha256` — verification still
//! passes because `verify_describe_self_consistent()` never looks at the
//! archive contents.
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
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Schema discriminator. Hard-coded `"greentic.gtxpack.manifest/v1"`.
    pub schema: String,
    /// Sorted-by-path list of every entry except `manifest.json` itself.
    pub entries: Vec<ManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
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
    #[error("archive uncompressed size {total} exceeds cap {cap}")]
    ArchiveTooLarge { total: u64, cap: u64 },
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
    verify_archive_against_manifest_with_caps(zip_bytes, MAX_ENTRY_BYTES, MAX_ARCHIVE_BYTES)
}

/// Inner implementation of [`verify_archive_against_manifest`] with injectable
/// byte caps. Using parameterised caps in tests avoids large allocations —
/// tiny caps (e.g. 8 bytes) let fast unit tests exercise bomb/total-cap paths
/// without touching the 64/256 MiB production limits.
fn verify_archive_against_manifest_with_caps(
    zip_bytes: &[u8],
    max_entry_bytes: u64,
    max_archive_bytes: u64,
) -> Result<(), ManifestError> {
    use std::io::Read;

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes))?;

    // Read manifest.json with an uncompressed size guard to prevent OOM from a
    // crafted manifest entry before we even start verifying real entries.
    let manifest: Manifest = {
        let mut f = archive
            .by_name(MANIFEST_ENTRY_NAME)
            .map_err(|_| ManifestError::Missing)?;
        if f.size() > max_entry_bytes {
            return Err(ManifestError::EntryTooLarge {
                path: MANIFEST_ENTRY_NAME.to_string(),
                size: f.size(),
                cap: max_entry_bytes,
            });
        }
        let mut body = Vec::new();
        f.by_ref()
            .take(max_entry_bytes + 1)
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
        // Lying headers are also caught by Guard 4 below (actual byte count after
        // decompression), but checking here avoids even starting an expensive
        // decompression for honest-but-oversized entries.
        if entry.size() > max_entry_bytes {
            return Err(ManifestError::EntryTooLarge {
                path: name,
                size: entry.size(),
                cap: max_entry_bytes,
            });
        }

        // Guard 2: manifest-recorded size exceeds the cap. Catches tampered /
        // maliciously crafted manifests that declare enormous sizes before the
        // archive is decompressed (audit H2).
        if row.size > max_entry_bytes {
            return Err(ManifestError::EntryTooLarge {
                path: name,
                size: row.size,
                cap: max_entry_bytes,
            });
        }

        // Guard 4: bounded decompression read catches lying zip headers that
        // claim a small uncompressed size but expand beyond the cap.
        let mut body = Vec::new();
        entry
            .by_ref()
            .take(max_entry_bytes + 1)
            .read_to_end(&mut body)?;
        if body.len() as u64 > max_entry_bytes {
            return Err(ManifestError::EntryTooLarge {
                path: name,
                size: body.len() as u64,
                cap: max_entry_bytes,
            });
        }

        // Guard 3: accumulate ACTUAL decompressed bytes across all entries
        // (saturating to avoid wrapping) and reject if the whole archive exceeds
        // the cap. Must happen AFTER the bounded read so we count real bytes, not
        // the zip header-declared size (which an attacker can set to 0 to bypass
        // the total cap — audit H2 fix).
        total_uncompressed = total_uncompressed.saturating_add(body.len() as u64);
        if total_uncompressed > max_archive_bytes {
            return Err(ManifestError::ArchiveTooLarge {
                total: total_uncompressed,
                cap: max_archive_bytes,
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
mod tests;
