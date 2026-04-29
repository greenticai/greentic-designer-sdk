//! Deterministic `.gtxpack` writer shared between `gtdx dev` and `gtdx publish`.
//!
//! Guarantees:
//! - Entries serialized in sorted path order.
//! - Timestamps zeroed to 1980-01-01 00:00:00 (the ZIP epoch minimum).
//! - Unix permissions normalized to 0o644 (files) / 0o755 (dirs).
//! - Text assets (json/md/wit/txt) have CRLF normalized to LF before hashing.
//! - Binary assets passed through untouched.

use std::io::{Cursor, Write};

use sha2::{Digest, Sha256};
use zip::DateTime;
use zip::write::SimpleFileOptions;

/// One file-or-dir entry fed into the pack writer.
#[derive(Debug, Clone)]
pub struct PackEntry {
    /// Path inside the zip (forward-slash separated, relative, no leading "/").
    pub path: String,
    /// Raw bytes (post-normalization if text).
    pub bytes: Vec<u8>,
    /// Directory entries are emitted without body but with Unix 0o755 mode.
    pub is_dir: bool,
}

impl PackEntry {
    #[must_use]
    pub fn file(path: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            path: path.into(),
            bytes,
            is_dir: false,
        }
    }
}

/// Returns true if the entry's path should have CRLF normalized to LF.
#[must_use]
pub fn is_text_path(path: &str) -> bool {
    matches!(
        std::path::Path::new(path)
            .extension()
            .and_then(|s| s.to_str()),
        Some("json" | "md" | "wit" | "txt" | "toml" | "yaml" | "yml")
    )
}

/// Normalize CRLF → LF for text entries, leave binary entries untouched.
#[must_use]
pub fn normalize_entry(mut entry: PackEntry) -> PackEntry {
    if is_text_path(&entry.path) {
        entry.bytes.retain(|b| *b != b'\r');
    }
    entry
}

fn zip_epoch() -> DateTime {
    DateTime::from_date_and_time(1980, 1, 1, 0, 0, 0)
        .expect("1980-01-01 00:00:00 is the minimum valid ZIP datetime")
}

/// Build a deterministic `.gtxpack` from `entries`. Returns the ZIP bytes.
/// Callers compute SHA256 separately via [`sha256_hex`].
///
/// # Errors
/// Returns a zip/io error if the ZIP writer fails.
pub fn build_gtxpack(entries: Vec<PackEntry>) -> Result<Vec<u8>, PackWriterError> {
    let mut entries: Vec<_> = entries.into_iter().map(normalize_entry).collect();
    entries.sort_by(|a, b| a.path.cmp(&b.path));

    let buf = Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(buf);
    let epoch = zip_epoch();

    for entry in entries {
        let opts = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .last_modified_time(epoch)
            .unix_permissions(if entry.is_dir { 0o755 } else { 0o644 });
        if entry.is_dir {
            zip.add_directory(&entry.path, opts)?;
        } else {
            zip.start_file(&entry.path, opts)?;
            zip.write_all(&entry.bytes)?;
        }
    }

    let cursor = zip.finish()?;
    Ok(cursor.into_inner())
}

/// Lowercase hex SHA256 of the given bytes.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write as _;
        write!(&mut out, "{b:02x}").expect("write to String");
    }
    out
}

#[derive(Debug, thiserror::Error)]
pub enum PackWriterError {
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entries() -> Vec<PackEntry> {
        vec![
            PackEntry::file("z.md", b"alpha\n".to_vec()),
            PackEntry::file("a.wasm", b"\0asm\x01\x00\x00\x00".to_vec()),
            PackEntry::file("describe.json", b"{\"k\":1}\n".to_vec()),
        ]
    }

    #[test]
    fn deterministic_sha256_across_runs() {
        let a = build_gtxpack(sample_entries()).unwrap();
        let b = build_gtxpack(sample_entries()).unwrap();
        assert_eq!(sha256_hex(&a), sha256_hex(&b));
    }

    #[test]
    fn input_order_is_normalized() {
        let ordered = sample_entries();
        let mut reversed = ordered.clone();
        reversed.reverse();
        let a = build_gtxpack(ordered).unwrap();
        let b = build_gtxpack(reversed).unwrap();
        assert_eq!(sha256_hex(&a), sha256_hex(&b));
    }

    #[test]
    fn crlf_in_text_is_normalized_to_lf() {
        let crlf = vec![PackEntry::file("doc.md", b"line1\r\nline2\r\n".to_vec())];
        let lf = vec![PackEntry::file("doc.md", b"line1\nline2\n".to_vec())];
        let a = build_gtxpack(crlf).unwrap();
        let b = build_gtxpack(lf).unwrap();
        assert_eq!(sha256_hex(&a), sha256_hex(&b));
    }

    #[test]
    fn crlf_in_binary_is_preserved() {
        // `.wasm` is a binary path — CRLF-looking bytes must pass through.
        let with_cr = vec![PackEntry::file("blob.wasm", b"\x00\r\n\x01".to_vec())];
        let without_cr = vec![PackEntry::file("blob.wasm", b"\x00\n\x01".to_vec())];
        let a = build_gtxpack(with_cr).unwrap();
        let b = build_gtxpack(without_cr).unwrap();
        assert_ne!(sha256_hex(&a), sha256_hex(&b));
    }

    #[test]
    fn sha256_hex_is_lowercase_64_chars() {
        let s = sha256_hex(b"hello");
        assert_eq!(s.len(), 64);
        assert!(
            s.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
    }

    #[test]
    fn zip_contains_expected_names() {
        let bytes = build_gtxpack(sample_entries()).unwrap();
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let names: Vec<_> = (0..zip.len())
            .map(|i| zip.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(names.contains(&"a.wasm".to_string()));
        assert!(names.contains(&"describe.json".to_string()));
        assert!(names.contains(&"z.md".to_string()));
    }
}
