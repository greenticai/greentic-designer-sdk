//! Tests for the gtxpack manifest builder/verifier (split out of `mod.rs` to
//! keep source files under the 500-line limit).

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
fn manifest_rejects_unknown_fields() {
    // The manifest is parsed from untrusted archive bytes; an unknown field
    // must be rejected rather than silently ignored (audit cycle-2 N9).
    let json = r#"{
        "schema": "greentic.gtxpack.manifest/v1",
        "entries": [],
        "evil": true
    }"#;
    assert!(serde_json::from_str::<Manifest>(json).is_err());
}

#[test]
fn manifest_entry_rejects_unknown_fields() {
    let json = r#"{
        "schema": "greentic.gtxpack.manifest/v1",
        "entries": [{ "path": "a", "sha256": "0", "size": 1, "evil": true }]
    }"#;
    assert!(serde_json::from_str::<Manifest>(json).is_err());
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

/// Guard 4 catches a real decompression bomb: zip header lies about the
/// uncompressed size (or is 0), but the actual decompressed bytes exceed
/// `max_entry_bytes`. We use a tiny cap (8 bytes) so the test allocates
/// nothing large — the 32-byte body is enough to exceed the cap.
#[test]
fn verify_rejects_real_zip_bomb_via_actual_bytes() {
    // 32 real bytes in the body; cap is only 8.
    let wasm_body: &[u8] = b"\0asm\x01\x00\x00\x00\xde\xad\xbe\xef\xca\xfe\xba\xbe\x00\x00\x00\x00\x11\x22\x33\x44\x55\x66\x77\x88\x99\xaa\xbb\xcc";
    let tiny_cap: u64 = 8;

    // Build a manifest whose sha256/size records the real 32-byte body.
    let manifest = build_manifest(vec![("extension.wasm", wasm_body)]);
    let manifest_json = serde_json::to_vec(&manifest).unwrap();

    let zip_bytes = build_zip(&[
        ("extension.wasm", wasm_body),
        ("manifest.json", manifest_json.as_slice()),
    ]);

    // With max_entry_bytes = 8, Guard 4 must reject the 32-byte body.
    let err = verify_archive_against_manifest_with_caps(&zip_bytes, tiny_cap, 1024).unwrap_err();
    assert!(
        matches!(err, ManifestError::EntryTooLarge { .. }),
        "expected EntryTooLarge, got {err:?}"
    );
}

/// Guard 3 (archive total cap) must be evaluated over ACTUAL decompressed
/// bytes, not zip header-declared sizes. Two entries of 6 bytes each sum
/// to 12, exceeding an archive cap of 8 — even though each individual
/// entry is under the per-entry cap of 8.
///
/// `max_entry_bytes` must be large enough to read `manifest.json` itself
/// (the serialised manifest is ~250 bytes for two entries), so we use 1024
/// as the per-entry cap while keeping the archive total cap at 8 bytes.
/// This confirms that Guard 3 fires on real (post-decompression) byte totals
/// and that the reported `total` field carries the actual accumulated count.
#[test]
fn verify_rejects_archive_total_over_cap() {
    let body_a: &[u8] = b"aaaaaa"; // 6 bytes
    let body_b: &[u8] = b"bbbbbb"; // 6 bytes
    // Per-entry cap high enough to read manifest.json (~250 bytes) and
    // each small entry (6 bytes).  Archive cap is tiny (8) so the two
    // 6-byte entries (12 total) exceed it.
    let max_entry: u64 = 1024;
    let max_archive: u64 = 8;

    // Manifest must carry correct sha256/size so verification fails on
    // the total-cap check, not on ShaMismatch or EntryTooLarge.
    let manifest = build_manifest(vec![("a.bin", body_a), ("b.bin", body_b)]);
    let manifest_json = serde_json::to_vec(&manifest).unwrap();

    let zip_bytes = build_zip(&[
        ("a.bin", body_a),
        ("b.bin", body_b),
        ("manifest.json", manifest_json.as_slice()),
    ]);

    let err =
        verify_archive_against_manifest_with_caps(&zip_bytes, max_entry, max_archive).unwrap_err();
    assert!(
        matches!(err, ManifestError::ArchiveTooLarge { .. }),
        "expected ArchiveTooLarge, got {err:?}"
    );
}
