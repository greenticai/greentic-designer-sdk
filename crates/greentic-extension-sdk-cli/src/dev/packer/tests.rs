//! Tests for the `.gtxpack` packer (split out of `mod.rs` to keep source
//! files under the 500-line limit).

use super::*;
use std::fs::File;

fn make_project(root: &Path) -> PathBuf {
    let desc = br#"{
  "apiVersion": "greentic.ai/v2",
  "kind": "DesignExtension",
  "compat": {"min_designer_version": ">=1.0.0", "min_runner_version": "^0.12.0", "contract_version": "1.2.4-research"},
  "metadata": {"id": "com.example.demo", "name": "demo", "version": "0.1.0", "summary": "x", "author": {"name": "a"}, "license": "Apache-2.0"},
  "engine": {"greenticDesigner": "^0.1.0", "extRuntime": "^0.1.0"},
  "capabilities": {"offered": [], "required": []},
  "runtime": {"components": {"stub": {"oci_ref": "oci://ghcr.io/example/stub:latest", "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "world": "greentic:component/stub@0.1.0"}}, "permissions": {"network": [], "secrets": [], "callExtensionKinds": []}},
  "contributions": {}
}"#;
    std::fs::write(root.join("describe.json"), desc).unwrap();
    let wasm_dir = root.join("target/wasm32-wasip2/debug");
    std::fs::create_dir_all(&wasm_dir).unwrap();
    let wasm = wasm_dir.join("demo.wasm");
    std::fs::write(&wasm, b"\0asm\x01\x00\x00\x00").unwrap();
    wasm
}

#[test]
fn build_pack_produces_zip_with_describe_and_wasm() {
    let tmp = tempfile::tempdir().unwrap();
    let wasm = make_project(tmp.path());
    let out = tmp.path().join("dist/demo-0.1.0.gtxpack");
    let info = build_pack(tmp.path(), &wasm, &out).unwrap();
    assert_eq!(info.ext_name, "demo");
    assert_eq!(info.ext_version, "0.1.0");
    assert_eq!(info.ext_kind, "DesignExtension");
    assert!(info.size > 0);

    let file = File::open(&out).unwrap();
    let mut zip = zip::ZipArchive::new(file).unwrap();
    let names: Vec<_> = (0..zip.len())
        .map(|i| zip.by_index(i).unwrap().name().to_string())
        .collect();
    assert!(names.iter().any(|n| n == "describe.json"));
    assert!(names.iter().any(|n| n == "extension.wasm"));
}

#[test]
fn build_pack_with_key_produces_verifiable_signed_pack() {
    let tmp = tempfile::tempdir().unwrap();
    let wasm = make_project(tmp.path());
    let out = tmp.path().join("dist/demo-0.1.0.gtxpack");
    let sk = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);

    let info = build_pack_with_key(tmp.path(), &wasm, &out, Some(&sk)).unwrap();
    let zip_bytes = std::fs::read(&out).unwrap();
    let manifest_bytes = read_manifest_from_zip(&zip_bytes).unwrap();
    let describe: DescribeJson = serde_json::from_slice(&info.describe_bytes).unwrap();

    // The produced pack must pass exactly the checks the registry runs at
    // install: archive integrity, manifest binding, anchored authenticity.
    greentic_extension_sdk_contract::verify_archive_against_manifest(&zip_bytes).unwrap();
    greentic_extension_sdk_contract::verify_manifest_binding(&describe, &manifest_bytes).unwrap();
    greentic_extension_sdk_contract::verify_describe_with_key(&describe, &sk.verifying_key())
        .unwrap();
}

#[test]
fn verify_produced_pack_accepts_good_and_rejects_tampered() {
    let tmp = tempfile::tempdir().unwrap();
    let wasm = make_project(tmp.path());
    let out = tmp.path().join("dist/demo-0.1.0.gtxpack");
    let sk = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
    let info = build_pack_with_key(tmp.path(), &wasm, &out, Some(&sk)).unwrap();
    let zip_bytes = std::fs::read(&out).unwrap();
    let describe: DescribeJson = serde_json::from_slice(&info.describe_bytes).unwrap();

    // The real output passes the install-time self-check.
    verify_produced_pack(&zip_bytes, &describe, Some(&sk)).unwrap();

    // A signature checked against the wrong key is rejected.
    let other = ed25519_dalek::SigningKey::from_bytes(&[9u8; 32]);
    assert!(verify_produced_pack(&zip_bytes, &describe, Some(&other)).is_err());
}

#[test]
fn build_pack_unsigned_is_manifest_bound_but_unsigned() {
    let tmp = tempfile::tempdir().unwrap();
    let wasm = make_project(tmp.path());
    let out = tmp.path().join("dist/demo-0.1.0.gtxpack");

    let info = build_pack(tmp.path(), &wasm, &out).unwrap();
    let describe: DescribeJson = serde_json::from_slice(&info.describe_bytes).unwrap();
    assert!(
        describe.manifest_sha256.is_some(),
        "manifest must be bound even without a signing key"
    );
    assert!(describe.signature.is_none(), "no key → no signature");

    let zip_bytes = std::fs::read(&out).unwrap();
    let manifest_bytes = read_manifest_from_zip(&zip_bytes).unwrap();
    greentic_extension_sdk_contract::verify_manifest_binding(&describe, &manifest_bytes).unwrap();
}

#[test]
fn build_pack_is_deterministic_across_runs() {
    let tmp = tempfile::tempdir().unwrap();
    let wasm = make_project(tmp.path());
    let out1 = tmp.path().join("a.gtxpack");
    let out2 = tmp.path().join("b.gtxpack");
    let a = build_pack(tmp.path(), &wasm, &out1).unwrap();
    let b = build_pack(tmp.path(), &wasm, &out2).unwrap();
    assert_eq!(a.sha256, b.sha256);
}

fn describe_with_gtpack_file(file: &str) -> serde_json::Value {
    serde_json::json!({
        "runtime": { "components": {
            "comp": { "gtpack": {
                "file": file,
                "sha256": "0000000000000000000000000000000000000000000000000000000000000000"
            }}
        }}
    })
}

#[test]
fn collect_runtime_files_rejects_parent_dir_traversal() {
    let tmp = tempfile::tempdir().unwrap();
    let describe = describe_with_gtpack_file("../escape.gtpack");
    let mut entries = Vec::new();
    let err = collect_runtime_component_files(
        &describe,
        tmp.path(),
        &tmp.path().join("out.gtxpack"),
        &mut entries,
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("project-relative"),
        "expected a path-traversal rejection, got: {err}"
    );
}

#[test]
fn collect_runtime_files_rejects_absolute_path() {
    let tmp = tempfile::tempdir().unwrap();
    let describe = describe_with_gtpack_file("/etc/passwd");
    let mut entries = Vec::new();
    let err = collect_runtime_component_files(
        &describe,
        tmp.path(),
        &tmp.path().join("out.gtxpack"),
        &mut entries,
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("project-relative"),
        "expected a path-traversal rejection, got: {err}"
    );
}

#[test]
fn collect_runtime_files_accepts_legit_relative_path() {
    let tmp = tempfile::tempdir().unwrap();
    let bytes = b"runtime-pack-bytes";
    std::fs::create_dir_all(tmp.path().join("runtime")).unwrap();
    std::fs::write(tmp.path().join("runtime/p.gtpack"), bytes).unwrap();
    let describe = serde_json::json!({
        "runtime": { "components": {
            "comp": { "gtpack": { "file": "runtime/p.gtpack", "sha256": sha256_hex(bytes) }}
        }}
    });
    let mut entries = Vec::new();
    collect_runtime_component_files(
        &describe,
        tmp.path(),
        &tmp.path().join("out.gtxpack"),
        &mut entries,
    )
    .unwrap();
    assert_eq!(entries.len(), 1);
}

#[test]
fn build_pack_includes_assets_when_present() {
    let tmp = tempfile::tempdir().unwrap();
    let wasm = make_project(tmp.path());
    std::fs::create_dir_all(tmp.path().join("i18n")).unwrap();
    std::fs::write(tmp.path().join("i18n/en.json"), br#"{"hello":"world"}"#).unwrap();
    let out = tmp.path().join("demo.gtxpack");
    build_pack(tmp.path(), &wasm, &out).unwrap();
    let file = File::open(&out).unwrap();
    let zip = zip::ZipArchive::new(file).unwrap();
    let names: Vec<_> = zip.file_names().map(str::to_string).collect();
    assert!(names.iter().any(|n| n == "i18n/en.json"));
}

#[test]
fn build_pack_includes_icon_assets_dir() {
    // describe.metadata.icon resolves to `assets/<file>` after unpack;
    // the consuming runtime serves it from `<extension-dir>/<icon-rel>`,
    // so the dir must ship inside the gtxpack zip.
    let tmp = tempfile::tempdir().unwrap();
    let wasm = make_project(tmp.path());
    std::fs::create_dir_all(tmp.path().join("assets")).unwrap();
    std::fs::write(
        tmp.path().join("assets/icon.svg"),
        br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1"/>"#,
    )
    .unwrap();
    let out = tmp.path().join("demo.gtxpack");
    build_pack(tmp.path(), &wasm, &out).unwrap();
    let file = File::open(&out).unwrap();
    let zip = zip::ZipArchive::new(file).unwrap();
    let names: Vec<_> = zip.file_names().map(str::to_string).collect();
    assert!(
        names.iter().any(|n| n == "assets/icon.svg"),
        "assets/icon.svg missing from gtxpack; entries: {names:?}"
    );
}

#[test]
fn build_pack_errors_if_describe_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let wasm_dir = tmp.path().join("target/wasm32-wasip2/debug");
    std::fs::create_dir_all(&wasm_dir).unwrap();
    std::fs::write(wasm_dir.join("x.wasm"), b"\0asm").unwrap();
    let out = tmp.path().join("out.gtxpack");
    let err = build_pack(tmp.path(), &wasm_dir.join("x.wasm"), &out).unwrap_err();
    assert!(err.to_string().contains("describe.json"));
}

// ── Provider extension tests ─────────────────────────────────────────────

/// Write a minimal provider describe.json with a single `provider` component
/// whose `gtpack` block uses `gtpack_field` (or `null` to test the absent case).
fn write_provider_describe(root: &Path, gtpack_field: &serde_json::Value) {
    // Every component needs a top-level sha256 + world to be a valid
    // `RuntimeComponent`. Absent-gtpack uses oci_ref (the offline-fallback
    // gtpack is simply not declared); present-gtpack carries the full
    // `RuntimeGtpack` block (file, sha256, pack_id, component_version).
    const ZERO_SHA: &str = "0000000000000000000000000000000000000000000000000000000000000000";
    let component = if gtpack_field.is_null() {
        serde_json::json!({
            "oci_ref": "oci://ghcr.io/example/provider:latest",
            "sha256": ZERO_SHA,
            "world": "greentic:component/provider@0.1.0"
        })
    } else {
        // Augment the caller's {file, sha256} block with the fields the
        // contract requires so the describe parses as typed.
        let mut gtpack = gtpack_field.clone();
        let obj = gtpack.as_object_mut().expect("gtpack block is an object");
        obj.entry("pack_id")
            .or_insert_with(|| serde_json::json!("com.example.provider.runtime"));
        obj.entry("component_version")
            .or_insert_with(|| serde_json::json!("0.1.0"));
        serde_json::json!({
            "gtpack": gtpack,
            "sha256": ZERO_SHA,
            "world": "greentic:component/provider@0.1.0"
        })
    };
    let desc = serde_json::json!({
        "apiVersion": "greentic.ai/v2",
        "kind": "ProviderExtension",
        "compat": {
            "min_designer_version": ">=1.0.0",
            "min_runner_version": "^0.12.0",
            "contract_version": "1.2.4-research"
        },
        "metadata": {
            "id": "com.example.provider",
            "name": "provider",
            "version": "0.1.0",
            "summary": "test provider",
            "author": {"name": "tester"},
            "license": "Apache-2.0"
        },
        "engine": {"greenticDesigner": "^0.1.0", "extRuntime": "^0.1.0"},
        "capabilities": {"offered": [], "required": []},
        "runtime": {
            "components": { "provider": component },
            "permissions": {"network": [], "secrets": [], "callExtensionKinds": []}
        },
        "contributions": {}
    });
    std::fs::write(root.join("describe.json"), desc.to_string()).unwrap();
}

/// Create a complete provider project with a valid `runtime/provider.gtpack`.
/// Returns (`wasm_path`, `gtpack_bytes`, `sha256_hex`).
fn make_provider_project(root: &Path) -> (PathBuf, Vec<u8>, String) {
    let gtpack_bytes = b"fake-gtpack-content-for-testing".to_vec();
    let sha = sha256_hex(&gtpack_bytes);

    std::fs::create_dir_all(root.join("runtime")).unwrap();
    std::fs::write(root.join("runtime/provider.gtpack"), &gtpack_bytes).unwrap();

    write_provider_describe(
        root,
        &serde_json::json!({
            "file": "runtime/provider.gtpack",
            "sha256": sha
        }),
    );

    let wasm_dir = root.join("target/wasm32-wasip2/debug");
    std::fs::create_dir_all(&wasm_dir).unwrap();
    let wasm = wasm_dir.join("provider.wasm");
    std::fs::write(&wasm, b"\0asm\x01\x00\x00\x00").unwrap();

    (wasm, gtpack_bytes, sha)
}

#[test]
fn provider_pack_includes_runtime_gtpack() {
    let tmp = tempfile::tempdir().unwrap();
    let (wasm, gtpack_bytes, _sha) = make_provider_project(tmp.path());
    let out = tmp.path().join("dist/provider-0.1.0.gtxpack");

    let info = build_pack(tmp.path(), &wasm, &out).unwrap();
    assert_eq!(info.ext_name, "provider");
    assert_eq!(info.ext_kind, "ProviderExtension");

    let file = File::open(&out).unwrap();
    let mut zip = zip::ZipArchive::new(file).unwrap();
    let names: Vec<_> = (0..zip.len())
        .map(|i| zip.by_index(i).unwrap().name().to_string())
        .collect();

    assert!(
        names.iter().any(|n| n == "describe.json"),
        "missing describe.json"
    );
    assert!(
        names.iter().any(|n| n == "extension.wasm"),
        "missing extension.wasm"
    );
    assert!(
        names.iter().any(|n| n == "runtime/provider.gtpack"),
        "missing runtime/provider.gtpack; entries: {names:?}"
    );

    // Verify byte content is preserved intact.
    let mut archive = zip::ZipArchive::new(File::open(&out).unwrap()).unwrap();
    let mut entry = archive.by_name("runtime/provider.gtpack").unwrap();
    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut entry, &mut buf).unwrap();
    assert_eq!(buf, gtpack_bytes);
}

#[test]
fn provider_pack_fails_when_runtime_file_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let gtpack_bytes = b"fake-gtpack-content-for-testing".to_vec();
    let sha = sha256_hex(&gtpack_bytes);

    // Write describe.json pointing at a non-existent file.
    write_provider_describe(
        tmp.path(),
        &serde_json::json!({
            "file": "runtime/provider.gtpack",
            "sha256": sha
        }),
    );

    let wasm_dir = tmp.path().join("target/wasm32-wasip2/debug");
    std::fs::create_dir_all(&wasm_dir).unwrap();
    let wasm = wasm_dir.join("provider.wasm");
    std::fs::write(&wasm, b"\0asm\x01\x00\x00\x00").unwrap();

    let out = tmp.path().join("out.gtxpack");
    let err = build_pack(tmp.path(), &wasm, &out).unwrap_err();
    assert!(
        err.to_string().contains("not found"),
        "expected 'not found' in error; got: {err}"
    );
}

#[test]
fn provider_pack_fails_when_sha256_mismatch() {
    let tmp = tempfile::tempdir().unwrap();

    std::fs::create_dir_all(tmp.path().join("runtime")).unwrap();
    std::fs::write(tmp.path().join("runtime/provider.gtpack"), b"real-content").unwrap();

    // Declare a wrong (all-zeros) sha256.
    write_provider_describe(
        tmp.path(),
        &serde_json::json!({
            "file": "runtime/provider.gtpack",
            "sha256": "0000000000000000000000000000000000000000000000000000000000000000"
        }),
    );

    let wasm_dir = tmp.path().join("target/wasm32-wasip2/debug");
    std::fs::create_dir_all(&wasm_dir).unwrap();
    let wasm = wasm_dir.join("provider.wasm");
    std::fs::write(&wasm, b"\0asm\x01\x00\x00\x00").unwrap();

    let out = tmp.path().join("out.gtxpack");
    let err = build_pack(tmp.path(), &wasm, &out).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("mismatch") || msg.contains("sha256"),
        "expected 'mismatch' or 'sha256' in error; got: {msg}"
    );
}

#[test]
fn design_pack_unchanged_without_gtpack() {
    // DesignExtension has no runtime.gtpack field — build_pack must succeed
    // with original behavior (no extra entries beyond describe + wasm + assets).
    let tmp = tempfile::tempdir().unwrap();
    let wasm = make_project(tmp.path());
    let out = tmp.path().join("dist/demo-0.1.0.gtxpack");

    let info = build_pack(tmp.path(), &wasm, &out).unwrap();
    assert_eq!(info.ext_kind, "DesignExtension");

    let file = File::open(&out).unwrap();
    let mut zip = zip::ZipArchive::new(file).unwrap();
    let names: Vec<_> = (0..zip.len())
        .map(|i| zip.by_index(i).unwrap().name().to_string())
        .collect();

    // describe.json + extension.wasm + manifest.json (D.4.2: every
    // pack now carries a whole-archive integrity manifest); no
    // runtime/ entry.
    assert_eq!(
        names.len(),
        3,
        "expected describe + wasm + manifest; got: {names:?}"
    );
    assert!(names.iter().any(|n| n == "describe.json"));
    assert!(names.iter().any(|n| n == "extension.wasm"));
    assert!(names.iter().any(|n| n == "manifest.json"));
}

// ── Secret enrichment from nested .gtpack manifests ──────────────────────

/// Read `runtime.permissions.secrets` from the `describe.json` entry of a
/// produced `.gtxpack` on disk (the bytes that actually ship, post-round-trip).
fn read_published_secrets(out: &Path) -> Vec<String> {
    let mut archive = zip::ZipArchive::new(File::open(out).unwrap()).unwrap();
    let mut entry = archive.by_name("describe.json").unwrap();
    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut entry, &mut buf).unwrap();
    let describe: serde_json::Value = serde_json::from_slice(&buf).unwrap();
    describe["runtime"]["permissions"]["secrets"]
        .as_array()
        .expect("permissions.secrets must be an array in the published describe")
        .iter()
        .map(|v| v.as_str().expect("secret key is a string").to_string())
        .collect()
}

/// Read `requiredSecrets[].key` strings from the `describe.json` entry of a
/// produced `.gtxpack` on disk.
fn read_published_required_secrets(out: &Path) -> Vec<String> {
    let mut archive = zip::ZipArchive::new(File::open(out).unwrap()).unwrap();
    let mut entry = archive.by_name("describe.json").unwrap();
    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut entry, &mut buf).unwrap();
    let describe: serde_json::Value = serde_json::from_slice(&buf).unwrap();
    match describe["requiredSecrets"].as_array() {
        None => Vec::new(),
        Some(arr) => arr
            .iter()
            .map(|v| {
                v["key"]
                    .as_str()
                    .expect("requiredSecrets[].key is a string")
                    .to_string()
            })
            .collect(),
    }
}

/// Build the raw bytes of a `.gtpack` (a ZIP carrying `manifest.cbor`) whose
/// `PackManifest.secret_requirements` declares `keys`. Used to exercise the
/// packer's auto-enrichment without depending on the full pack toolchain.
fn make_gtpack_with_secret_keys(keys: &[&str]) -> Vec<u8> {
    use greentic_types::{
        PackId, PackKind, PackManifest, PackSignatures, SecretKey, SecretRequirement,
    };

    // `SecretRequirement` is `#[non_exhaustive]`, so build from `Default` and
    // set the one field that matters for enrichment (the key).
    let secret_requirements = keys
        .iter()
        .map(|k| {
            let mut req = SecretRequirement::default();
            req.key = SecretKey::new(*k).expect("valid secret key");
            req
        })
        .collect();

    let manifest = PackManifest {
        schema_version: "1".to_string(),
        pack_id: PackId::new("com.example.provider.runtime").expect("valid pack id"),
        name: None,
        version: semver::Version::new(0, 1, 0),
        kind: PackKind::Provider,
        publisher: "tester".to_string(),
        components: Vec::new(),
        flows: Vec::new(),
        dependencies: Vec::new(),
        capabilities: Vec::new(),
        secret_requirements,
        signatures: PackSignatures::default(),
        bootstrap: None,
        extensions: None,
        agents: std::collections::BTreeMap::new(),
    };
    let cbor = greentic_types::encode_pack_manifest(&manifest).expect("encode manifest");

    let mut buf = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zip::write::ZipWriter::start_file(&mut zip, "manifest.cbor", opts).unwrap();
        std::io::Write::write_all(&mut zip, &cbor).unwrap();
        zip.finish().unwrap();
    }
    buf
}

/// Build a provider project whose `runtime/provider.gtpack` is a real `.gtpack`
/// declaring `keys` as secret requirements, optionally with an author-listed
/// secret already present in `permissions.secrets`.
fn make_provider_project_with_secret_pack(
    root: &Path,
    keys: &[&str],
    author_secrets: &[&str],
) -> PathBuf {
    let gtpack_bytes = make_gtpack_with_secret_keys(keys);
    let sha = sha256_hex(&gtpack_bytes);

    std::fs::create_dir_all(root.join("runtime")).unwrap();
    std::fs::write(root.join("runtime/provider.gtpack"), &gtpack_bytes).unwrap();

    write_provider_describe(
        root,
        &serde_json::json!({ "file": "runtime/provider.gtpack", "sha256": sha }),
    );

    // Patch in author-listed secrets when requested.
    // Author secrets live in `requiredSecrets` (top-level), not in
    // `permissions.secrets` — the latter is left as authored (empty).
    if !author_secrets.is_empty() {
        let path = root.join("describe.json");
        let mut describe: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let reqs: Vec<serde_json::Value> = author_secrets
            .iter()
            .map(|k| serde_json::json!({"key": k}))
            .collect();
        describe["requiredSecrets"] = serde_json::json!(reqs);
        std::fs::write(&path, describe.to_string()).unwrap();
    }

    let wasm_dir = root.join("target/wasm32-wasip2/debug");
    std::fs::create_dir_all(&wasm_dir).unwrap();
    let wasm = wasm_dir.join("provider.wasm");
    std::fs::write(&wasm, b"\0asm\x01\x00\x00\x00").unwrap();
    wasm
}

#[test]
fn provider_pack_enriches_secrets_from_nested_gtpack() {
    // Nested pack declares B_TOKEN, A_TOKEN (unsorted, no author secrets).
    let tmp = tempfile::tempdir().unwrap();
    let wasm = make_provider_project_with_secret_pack(tmp.path(), &["B_TOKEN", "A_TOKEN"], &[]);
    let out = tmp.path().join("dist/provider-0.1.0.gtxpack");

    build_pack(tmp.path(), &wasm, &out).unwrap();

    // The published describe.json must carry the union in `requiredSecrets`,
    // deduped and sorted by key — NOT in `permissions.secrets`.
    assert_eq!(
        read_published_required_secrets(&out),
        vec!["A_TOKEN".to_string(), "B_TOKEN".to_string()],
        "requiredSecrets must be auto-enriched from the nested .gtpack manifest"
    );
    assert!(
        read_published_secrets(&out).is_empty(),
        "permissions.secrets must not be populated by the enrichment step"
    );
}

#[test]
fn provider_pack_merges_author_and_nested_secrets() {
    // Nested pack declares B_TOKEN, A_TOKEN; author already listed C_TOKEN and
    // a duplicate A_TOKEN in requiredSecrets — result must be the deduped,
    // sorted union in requiredSecrets (NOT in permissions.secrets).
    let tmp = tempfile::tempdir().unwrap();
    let wasm = make_provider_project_with_secret_pack(
        tmp.path(),
        &["B_TOKEN", "A_TOKEN"],
        &["C_TOKEN", "A_TOKEN"],
    );
    let out = tmp.path().join("dist/provider-0.1.0.gtxpack");

    build_pack(tmp.path(), &wasm, &out).unwrap();

    assert_eq!(
        read_published_required_secrets(&out),
        vec![
            "A_TOKEN".to_string(),
            "B_TOKEN".to_string(),
            "C_TOKEN".to_string()
        ],
        "requiredSecrets must be the deduped, sorted union of author + nested keys"
    );
    assert!(
        read_published_secrets(&out).is_empty(),
        "permissions.secrets must not be populated by the enrichment step"
    );
}

#[test]
fn provider_pack_no_secrets_leaves_permissions_empty() {
    // Nested .gtpack with zero secret_requirements must not invent any.
    let tmp = tempfile::tempdir().unwrap();
    let wasm = make_provider_project_with_secret_pack(tmp.path(), &[], &[]);
    let out = tmp.path().join("dist/provider-0.1.0.gtxpack");

    build_pack(tmp.path(), &wasm, &out).unwrap();

    assert!(
        read_published_required_secrets(&out).is_empty(),
        "empty nested secret_requirements must leave requiredSecrets empty"
    );
    assert!(
        read_published_secrets(&out).is_empty(),
        "empty nested secret_requirements must leave permissions.secrets empty"
    );
}

#[test]
fn provider_pack_handles_absent_gtpack_field() {
    // Component without a gtpack key — should behave like the design case
    // (no extra entry, no error).
    let tmp = tempfile::tempdir().unwrap();

    write_provider_describe(tmp.path(), &serde_json::Value::Null);

    let wasm_dir = tmp.path().join("target/wasm32-wasip2/debug");
    std::fs::create_dir_all(&wasm_dir).unwrap();
    let wasm = wasm_dir.join("provider.wasm");
    std::fs::write(&wasm, b"\0asm\x01\x00\x00\x00").unwrap();

    let out = tmp.path().join("out.gtxpack");
    let info = build_pack(tmp.path(), &wasm, &out).unwrap();

    // describe + wasm + manifest.json (D.4.2: every pack now carries a
    // whole-archive integrity manifest).
    let file = File::open(&out).unwrap();
    let mut zip = zip::ZipArchive::new(file).unwrap();
    let names: Vec<_> = (0..zip.len())
        .map(|i| zip.by_index(i).unwrap().name().to_string())
        .collect();
    assert_eq!(
        names.len(),
        3,
        "absent gtpack should produce describe + wasm + manifest; got: {names:?}"
    );
    let _ = info;
}

/// A scaffold-shaped project: one self-contained component whose `gtpack.file`
/// is the wasm the build itself produces, with the all-zero digests
/// `gtdx new` writes because the wasm does not exist yet.
fn make_self_contained_project(root: &Path) -> PathBuf {
    let desc = br#"{
  "apiVersion": "greentic.ai/v2",
  "kind": "DesignExtension",
  "compat": {"min_designer_version": ">=1.2.0", "min_runner_version": "^0.12.0", "contract_version": "1.2.4-research"},
  "metadata": {"id": "greentic.demo", "name": "demo", "version": "0.1.0", "summary": "x", "author": {"name": "a"}, "license": "Apache-2.0"},
  "capabilities": {"offered": [], "required": []},
  "runtime": {"components": {"demo": {"gtpack": {"file": "extension.wasm", "sha256": "0000000000000000000000000000000000000000000000000000000000000000", "pack_id": "greentic.demo", "component_version": "0.1.0"}, "sha256": "0000000000000000000000000000000000000000000000000000000000000000", "world": "greentic:demo/extension@1.0.0"}}, "permissions": {"network": [], "secrets": [], "callExtensionKinds": []}},
  "contributions": {}
}"#;
    std::fs::write(root.join("describe.json"), desc).unwrap();
    let wasm_dir = root.join("target/wasm32-wasip2/debug");
    std::fs::create_dir_all(&wasm_dir).unwrap();
    let wasm = wasm_dir.join("demo.wasm");
    std::fs::write(&wasm, b"\0asm\x01\x00\x00\x00payload").unwrap();
    wasm
}

/// Read `describe.json` back out of a built pack — the bytes a consumer sees.
fn describe_from_pack(pack: &Path) -> serde_json::Value {
    use std::io::Read as _;
    let file = File::open(pack).unwrap();
    let mut zip = zip::ZipArchive::new(file).unwrap();
    let mut entry = zip.by_name("describe.json").unwrap();
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf).unwrap();
    serde_json::from_slice(&buf).unwrap()
}

#[test]
fn build_pack_fills_self_contained_component_hashes() {
    let tmp = tempfile::tempdir().unwrap();
    let wasm = make_self_contained_project(tmp.path());
    let out = tmp.path().join("dist/demo-0.1.0.gtxpack");

    build_pack(tmp.path(), &wasm, &out).unwrap();

    let expected = sha256_hex(&std::fs::read(&wasm).unwrap());
    let shipped = describe_from_pack(&out);
    let comp = &shipped["runtime"]["components"]["demo"];
    assert_eq!(comp["sha256"].as_str().unwrap(), expected);
    assert_eq!(comp["gtpack"]["sha256"].as_str().unwrap(), expected);
}

#[test]
fn build_pack_fills_hashes_on_disk_describe_too() {
    let tmp = tempfile::tempdir().unwrap();
    let wasm = make_self_contained_project(tmp.path());
    let out = tmp.path().join("dist/demo-0.1.0.gtxpack");

    build_pack(tmp.path(), &wasm, &out).unwrap();

    // `gtdx lint --publish` reads the project's describe.json, not the pack,
    // so the placeholders have to clear there as well or E_SHA256_ZERO keeps
    // firing after a successful publish.
    let on_disk: serde_json::Value =
        serde_json::from_slice(&std::fs::read(tmp.path().join("describe.json")).unwrap()).unwrap();
    let expected = sha256_hex(&std::fs::read(&wasm).unwrap());
    assert_eq!(
        on_disk["runtime"]["components"]["demo"]["sha256"]
            .as_str()
            .unwrap(),
        expected
    );
    assert_eq!(
        on_disk["runtime"]["components"]["demo"]["gtpack"]["sha256"]
            .as_str()
            .unwrap(),
        expected
    );
}

#[test]
fn build_pack_leaves_externally_supplied_hashes_alone() {
    // The `stub` component is OCI-referenced: its digest describes an artifact
    // this build never produced, so filling it would be a lie.
    let tmp = tempfile::tempdir().unwrap();
    let wasm = make_project(tmp.path());
    let out = tmp.path().join("dist/demo-0.1.0.gtxpack");

    build_pack(tmp.path(), &wasm, &out).unwrap();

    let shipped = describe_from_pack(&out);
    assert_eq!(
        shipped["runtime"]["components"]["stub"]["sha256"]
            .as_str()
            .unwrap(),
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
}

#[test]
fn signed_self_contained_pack_still_verifies_with_filled_hashes() {
    let tmp = tempfile::tempdir().unwrap();
    let wasm = make_self_contained_project(tmp.path());
    let out = tmp.path().join("dist/demo-0.1.0.gtxpack");
    let sk = ed25519_dalek::SigningKey::from_bytes(&[9u8; 32]);

    let info = build_pack_with_key(tmp.path(), &wasm, &out, Some(&sk)).unwrap();
    let zip_bytes = std::fs::read(&out).unwrap();
    let manifest_bytes = read_manifest_from_zip(&zip_bytes).unwrap();
    let describe: DescribeJson = serde_json::from_slice(&info.describe_bytes).unwrap();

    // Hashes are filled before signing, so the signature must cover them.
    greentic_extension_sdk_contract::verify_archive_against_manifest(&zip_bytes).unwrap();
    greentic_extension_sdk_contract::verify_manifest_binding(&describe, &manifest_bytes).unwrap();
    greentic_extension_sdk_contract::verify_describe_with_key(&describe, &sk.verifying_key())
        .unwrap();
}

#[test]
fn filling_already_correct_hashes_reports_no_change() {
    // `describe.json` is a watched path, so a write during `gtdx dev` queues
    // another rebuild. Reporting "unchanged" once the digests are already right
    // is what makes the watch loop settle instead of rebuilding forever.
    let mut describe = serde_json::json!({
        "runtime": {"components": {"demo": {
            "gtpack": {"file": "extension.wasm", "sha256": "ab".repeat(32)},
            "sha256": "ab".repeat(32)
        }}}
    });
    assert!(!fill_self_contained_hashes(&mut describe, &"ab".repeat(32)));
    assert!(fill_self_contained_hashes(&mut describe, &"cd".repeat(32)));
}

#[test]
fn rebuilding_identical_wasm_leaves_describe_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let wasm = make_self_contained_project(tmp.path());
    let out = tmp.path().join("dist/demo-0.1.0.gtxpack");
    let describe_path = tmp.path().join("describe.json");

    build_pack(tmp.path(), &wasm, &out).unwrap();
    let after_first = std::fs::metadata(&describe_path)
        .unwrap()
        .modified()
        .unwrap();

    // Same source, same wasm — the second pack must not touch describe.json, or
    // every `gtdx dev` rebuild would trip the watcher into one more rebuild.
    build_pack(tmp.path(), &wasm, &out).unwrap();
    let after_second = std::fs::metadata(&describe_path)
        .unwrap()
        .modified()
        .unwrap();

    assert_eq!(after_first, after_second);
}
