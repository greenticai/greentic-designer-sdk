//! Tests for publish orchestration (split out of `mod.rs`).

use super::*;
use backend::Backend;
use ed25519_dalek::pkcs8::{EncodePrivateKey, spki::der::pem::LineEnding};
use greentic_extension_sdk_registry::oci::OciRegistry;

/// A `PublishConfig` carrying only the signing-related fields under test;
/// everything else is an inert default (these tests never reach packing).
fn signing_cfg(home: &Path, key_id: Option<&str>, key_path: Option<PathBuf>) -> PublishConfig {
    PublishConfig {
        project_dir: home.to_path_buf(),
        registry_uri: "local".into(),
        home: home.to_path_buf(),
        dist_dir: home.join("dist"),
        profile: Profile::Debug,
        dry_run: true,
        force: false,
        sign: true,
        key_id: key_id.map(str::to_string),
        key_path,
        key_env: crate::signing::DEFAULT_KEY_ENV.to_string(),
        version_override: None,
        trust_policy: "loose".into(),
        verify_only: false,
        oci_token: None,
        wasm_override: None,
    }
}

#[test]
fn wasm_override_uses_provided_file_and_skips_build() {
    // A pre-built component (e.g. a generated MCP component) is packed as-is
    // — resolve must return the exact path without touching the toolchain.
    let tmp = tempfile::tempdir().unwrap();
    let wasm = tmp.path().join("component.wasm");
    std::fs::write(&wasm, b"\0asm\x01\0\0\0").unwrap();
    let mut cfg = signing_cfg(tmp.path(), None, None);
    cfg.sign = false;
    cfg.wasm_override = Some(wasm.clone());

    let got = resolve_publish_wasm(&cfg).unwrap();
    assert_eq!(got, wasm);
}

#[test]
fn wasm_override_missing_file_errors_before_build() {
    // A bad `--wasm` path must fail fast with a Build error, not fall
    // through to `cargo component build`.
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = signing_cfg(tmp.path(), None, None);
    cfg.sign = false;
    cfg.wasm_override = Some(tmp.path().join("does-not-exist.wasm"));

    let err = resolve_publish_wasm(&cfg).unwrap_err();
    assert!(
        matches!(err, PublishError::Build(ref m) if m.contains("--wasm path is not a file")),
        "expected Build error naming the missing --wasm path, got: {err}"
    );
}

#[test]
fn wasm_override_rejects_a_directory() {
    // `is_file()` guards against pointing `--wasm` at a directory.
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("not-a-file");
    std::fs::create_dir(&dir).unwrap();
    let mut cfg = signing_cfg(tmp.path(), None, None);
    cfg.sign = false;
    cfg.wasm_override = Some(dir);

    assert!(matches!(
        resolve_publish_wasm(&cfg).unwrap_err(),
        PublishError::Build(_)
    ));
}

fn write_pem_key(path: &Path, seed: [u8; 32]) {
    let pem = ed25519_dalek::SigningKey::from_bytes(&seed)
        .to_pkcs8_pem(LineEnding::LF)
        .unwrap();
    std::fs::write(path, pem.as_bytes()).unwrap();
}

fn minimal_describe() -> DescribeJson {
    serde_json::from_str(
        r#"{
  "apiVersion": "greentic.ai/v2",
  "kind": "DesignExtension",
  "compat": {"min_designer_version": ">=1.0.0", "min_runner_version": "^0.12.0", "contract_version": "1.2.0"},
  "metadata": {"id": "com.example.demo", "name": "demo", "version": "0.1.0", "summary": "x", "author": {"name": "a"}, "license": "MIT"},
  "engine": {"greenticDesigner": "^0.1.0", "extRuntime": "^0.1.0"},
  "capabilities": {"offered": [], "required": []},
  "runtime": {"components": {"stub": {"oci_ref": "oci://ghcr.io/example/stub:latest", "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "world": "greentic:component/stub@0.1.0"}}, "permissions": {"network": [], "secrets": [], "callExtensionKinds": []}},
  "contributions": {}
}"#,
    )
    .unwrap()
}

#[tokio::test]
async fn run_publish_strict_without_sign_is_rejected() {
    // `--trust strict` must refuse an unsigned publish (audit cycle-1 P2).
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = signing_cfg(tmp.path(), None, None);
    cfg.trust_policy = "strict".into();
    cfg.sign = false;
    let err = run_publish(&cfg).await.unwrap_err();
    assert!(
        err.to_string().contains("strict requires"),
        "strict without --sign should be rejected, got: {err}"
    );
}

#[tokio::test]
async fn verify_only_oci_does_not_report_false_slot_free() {
    // verify-only against OCI must NOT return a success that prints
    // "slot free" without probing the server — that gives CI a false green.
    // Until a real conflict probe exists, it returns NotImplemented (audit N3).
    let backend = Backend::Oci(OciRegistry::new("oci", "ghcr.io", "greenticai/ext", None));
    let err = verify_only(&backend, &minimal_describe(), false)
        .await
        .unwrap_err();
    assert!(
        matches!(err, PublishError::NotImplemented(_)),
        "OCI verify-only must be NotImplemented, not a false success; got: {err:?}"
    );
}

#[test]
fn resolve_signing_key_rejects_path_traversal() {
    let tmp = tempfile::tempdir().unwrap();
    for bad in ["../../etc/passwd", "..", "a/b", "a\\b", ""] {
        let err = resolve_signing_key(&signing_cfg(tmp.path(), Some(bad), None)).unwrap_err();
        assert!(
            err.to_string().contains("key-id"),
            "key_id {bad:?} should be rejected as unsafe, got: {err}"
        );
    }
}

#[test]
fn resolve_signing_key_loads_pkcs8_pem_by_key_id() {
    // A key written by `gtdx keygen --out ~/.greentic/keys/<id>.key` (PKCS8
    // PEM) must load verbatim — the format `publish` previously rejected.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("keys")).unwrap();
    write_pem_key(&tmp.path().join("keys/ci.key"), [9u8; 32]);

    let (key, label) = resolve_signing_key(&signing_cfg(tmp.path(), Some("ci"), None)).unwrap();
    assert_eq!(key.to_bytes(), [9u8; 32]);
    assert_eq!(label, "ci", "key_id label must be the explicit --key-id");
}

#[test]
fn resolve_signing_key_prefers_explicit_path_and_derives_label() {
    let tmp = tempfile::tempdir().unwrap();
    let key_file = tmp.path().join("release-signing.key");
    write_pem_key(&key_file, [3u8; 32]);

    // --key with no --key-id → label derived from the file stem.
    let (key, label) = resolve_signing_key(&signing_cfg(tmp.path(), None, Some(key_file))).unwrap();
    assert_eq!(key.to_bytes(), [3u8; 32]);
    assert_eq!(label, "release-signing");
}

#[test]
fn write_canonical_dist_removes_staging() {
    let tmp = tempfile::tempdir().unwrap();
    let dist = tmp.path().join("dist");
    std::fs::create_dir_all(&dist).unwrap();
    let staging = dist.join("publish-staging.gtxpack");
    std::fs::write(&staging, b"pack-bytes").unwrap();

    let final_dist = write_canonical_dist(&staging, &dist, "demo", "0.1.0", b"pack-bytes").unwrap();

    assert!(final_dist.exists());
    assert_eq!(final_dist.file_name().unwrap(), "demo-0.1.0.gtxpack");
    assert!(
        !staging.exists(),
        "staging pack must not linger in ./dist after publish"
    );
}

/// Empirical repro: `component-guardrail-topic`'s `describe.json` has
/// `metadata.name = "Topic / scope guardrail"`. Previously the raw name was
/// used verbatim as a filename component, so the write targeted a
/// nonexistent nested directory (`dist/Topic /...`) and failed with a bare
/// `No such file or directory (os error 2)`. A `/` in the name must still
/// produce a real, single-segment `.gtxpack` written directly under `dist`.
#[test]
fn write_canonical_dist_sanitizes_slash_in_name() {
    let tmp = tempfile::tempdir().unwrap();
    let dist = tmp.path().join("dist");
    std::fs::create_dir_all(&dist).unwrap();
    let staging = dist.join("publish-staging.gtxpack");
    std::fs::write(&staging, b"pack-bytes").unwrap();

    let final_dist = write_canonical_dist(
        &staging,
        &dist,
        "Topic / scope guardrail",
        "0.1.0",
        b"pack-bytes",
    )
    .expect("publish must succeed despite '/' in metadata.name");

    assert!(final_dist.is_file());
    assert_eq!(
        final_dist.parent().unwrap(),
        dist,
        "must land directly in dist_dir, not a nested dir"
    );
    assert_eq!(std::fs::read(&final_dist).unwrap(), b"pack-bytes");
}

#[test]
fn write_canonical_dist_sanitizes_backslash_in_name() {
    let tmp = tempfile::tempdir().unwrap();
    let dist = tmp.path().join("dist");
    std::fs::create_dir_all(&dist).unwrap();
    let staging = dist.join("publish-staging.gtxpack");
    std::fs::write(&staging, b"pack-bytes").unwrap();

    let final_dist = write_canonical_dist(&staging, &dist, r"weird\name", "0.1.0", b"pack-bytes")
        .expect("publish must succeed despite '\\' in metadata.name");

    assert!(final_dist.is_file());
    assert_eq!(final_dist.parent().unwrap(), dist);
}

#[test]
fn write_canonical_dist_sanitizes_degenerate_names() {
    for bad in ["", ".", ".."] {
        let tmp = tempfile::tempdir().unwrap();
        let dist = tmp.path().join("dist");
        std::fs::create_dir_all(&dist).unwrap();
        let staging = dist.join("publish-staging.gtxpack");
        std::fs::write(&staging, b"pack-bytes").unwrap();

        let final_dist = write_canonical_dist(&staging, &dist, bad, "0.1.0", b"pack-bytes")
            .unwrap_or_else(|e| panic!("name {bad:?} must still publish, got: {e}"));

        assert!(final_dist.is_file());
        assert_eq!(
            final_dist.parent().unwrap(),
            dist,
            "name {bad:?} must not escape dist_dir"
        );
    }
}
