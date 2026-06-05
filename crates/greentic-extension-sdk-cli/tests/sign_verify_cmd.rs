use std::io::Write as _;
use std::path::PathBuf;
use std::process::Command;

use greentic_extension_sdk_contract::ExtensionKind;
use greentic_extension_sdk_testing::{ExtensionFixture, ExtensionFixtureBuilder};
use tempfile::TempDir;

fn gtdx_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_gtdx"))
}

fn new_describe_fixture() -> (ExtensionFixture, PathBuf) {
    let fx = ExtensionFixtureBuilder::new(ExtensionKind::Design, "greentic.cli-sign", "0.1.0")
        .offer("greentic:test/y", "1.0.0")
        .with_wasm(b"wasm".to_vec())
        .build()
        .unwrap();
    let describe = fx.root().join("describe.json");
    (fx, describe)
}

#[test]
fn keygen_writes_valid_pkcs8_to_stdout() {
    let output = Command::new(gtdx_bin()).arg("keygen").output().unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let pem = String::from_utf8(output.stdout).unwrap();
    assert!(pem.starts_with("-----BEGIN PRIVATE KEY-----"));
    assert!(pem.trim_end().ends_with("-----END PRIVATE KEY-----"));
}

#[test]
fn keygen_refuses_overwrite() {
    let tmp = TempDir::new().unwrap();
    let key_path = tmp.path().join("k.pem");
    std::fs::write(&key_path, b"existing").unwrap();
    let output = Command::new(gtdx_bin())
        .arg("keygen")
        .arg("--out")
        .arg(&key_path)
        .output()
        .unwrap();
    assert!(!output.status.success(), "keygen should refuse overwrite");
}

#[test]
fn sign_then_verify_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let key_path = tmp.path().join("k.pem");
    let out = Command::new(gtdx_bin())
        .arg("keygen")
        .arg("--out")
        .arg(&key_path)
        .output()
        .unwrap();
    assert!(out.status.success());

    let (_fx, describe_path) = new_describe_fixture();
    let out = Command::new(gtdx_bin())
        .arg("sign")
        .arg(&describe_path)
        .arg("--key")
        .arg(&key_path)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "sign stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = Command::new(gtdx_bin())
        .arg("verify")
        .arg(&describe_path)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "verify stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.starts_with("OK  greentic.cli-sign v0.1.0"));
}

#[test]
fn sign_uses_env_var_when_no_key_flag() {
    let tmp = TempDir::new().unwrap();
    let key_path = tmp.path().join("k.pem");
    Command::new(gtdx_bin())
        .arg("keygen")
        .arg("--out")
        .arg(&key_path)
        .output()
        .unwrap();
    let pem = std::fs::read_to_string(&key_path).unwrap();

    let (_fx, describe_path) = new_describe_fixture();
    let out = Command::new(gtdx_bin())
        .arg("sign")
        .arg(&describe_path)
        .env("GREENTIC_EXT_SIGNING_KEY_PEM", &pem)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "sign stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = Command::new(gtdx_bin())
        .arg("verify")
        .arg(&describe_path)
        .output()
        .unwrap();
    assert!(out.status.success());
}

#[test]
fn sign_missing_key_emits_hint() {
    let (_fx, describe_path) = new_describe_fixture();
    let out = Command::new(gtdx_bin())
        .arg("sign")
        .arg(&describe_path)
        .env_remove("GREENTIC_EXT_SIGNING_KEY_PEM")
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("GREENTIC_EXT_SIGNING_KEY_PEM"),
        "expected env var name in stderr, got: {stderr}"
    );
    assert!(
        stderr.contains("--key"),
        "expected --key hint in stderr, got: {stderr}"
    );
}

#[test]
fn verify_rejects_tampered() {
    let tmp = TempDir::new().unwrap();
    let key_path = tmp.path().join("k.pem");
    Command::new(gtdx_bin())
        .arg("keygen")
        .arg("--out")
        .arg(&key_path)
        .output()
        .unwrap();

    let (_fx, describe_path) = new_describe_fixture();
    Command::new(gtdx_bin())
        .arg("sign")
        .arg(&describe_path)
        .arg("--key")
        .arg(&key_path)
        .output()
        .unwrap();

    // Mutate version after signing to invalidate the signature.
    let raw = std::fs::read_to_string(&describe_path).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    v["metadata"]["version"] = serde_json::json!("99.99.99");
    std::fs::write(&describe_path, serde_json::to_string_pretty(&v).unwrap()).unwrap();

    let out = Command::new(gtdx_bin())
        .arg("verify")
        .arg(&describe_path)
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("signature invalid"),
        "expected 'signature invalid' in stderr, got: {stderr}"
    );
}

#[test]
fn verify_accepts_directory() {
    let tmp = TempDir::new().unwrap();
    let key_path = tmp.path().join("k.pem");
    Command::new(gtdx_bin())
        .arg("keygen")
        .arg("--out")
        .arg(&key_path)
        .output()
        .unwrap();

    let (fx, describe_path) = new_describe_fixture();
    Command::new(gtdx_bin())
        .arg("sign")
        .arg(&describe_path)
        .arg("--key")
        .arg(&key_path)
        .output()
        .unwrap();

    let out = Command::new(gtdx_bin())
        .arg("verify")
        .arg(fx.root())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn verify_rejects_manifestless_gtxpack_archive() {
    // Audit P0-3: whole-archive integrity is enforced unconditionally. A
    // signed describe is no longer sufficient — an archive that omits the
    // manifest.json ledger has no way to prove its payload was not swapped,
    // so verify must reject it rather than silently trusting the signature.
    let tmp = TempDir::new().unwrap();
    let key_path = tmp.path().join("k.pem");
    Command::new(gtdx_bin())
        .arg("keygen")
        .arg("--out")
        .arg(&key_path)
        .output()
        .unwrap();

    let (fx, describe_path) = new_describe_fixture();
    Command::new(gtdx_bin())
        .arg("sign")
        .arg(&describe_path)
        .arg("--key")
        .arg(&key_path)
        .output()
        .unwrap();

    // Zip describe.json + extension.wasm into a .gtxpack archive, deliberately
    // omitting manifest.json.
    let pack_path = tmp.path().join("ext.gtxpack");
    {
        let f = std::fs::File::create(&pack_path).unwrap();
        let mut zip = zip::ZipWriter::new(f);
        let options: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("describe.json", options).unwrap();
        zip.write_all(&std::fs::read(&describe_path).unwrap())
            .unwrap();
        zip.start_file("extension.wasm", options).unwrap();
        zip.write_all(&std::fs::read(fx.root().join("extension.wasm")).unwrap())
            .unwrap();
        zip.finish().unwrap();
    }

    let out = Command::new(gtdx_bin())
        .arg("verify")
        .arg(&pack_path)
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "verify must reject a gtxpack with no manifest.json ledger"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("manifest"),
        "expected a missing-manifest error in stderr, got: {stderr}"
    );
}

#[test]
fn verify_rejects_gtxpack_with_tampered_wasm_when_manifest_present() {
    let tmp = TempDir::new().unwrap();
    let key_path = tmp.path().join("k.pem");
    Command::new(gtdx_bin())
        .arg("keygen")
        .arg("--out")
        .arg(&key_path)
        .output()
        .unwrap();

    let (fx, describe_path) = new_describe_fixture();
    Command::new(gtdx_bin())
        .arg("sign")
        .arg(&describe_path)
        .arg("--key")
        .arg(&key_path)
        .output()
        .unwrap();

    let describe_bytes = std::fs::read(&describe_path).unwrap();
    let good_wasm = std::fs::read(fx.root().join("extension.wasm")).unwrap();

    // Manifest is built over the *legitimate* wasm bytes...
    let manifest = greentic_extension_sdk_contract::build_manifest(vec![
        ("describe.json", describe_bytes.as_slice()),
        ("extension.wasm", good_wasm.as_slice()),
    ]);
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();

    // ...but the archive ships a swapped wasm. The describe signature is still
    // valid, so only the manifest ledger can catch this.
    let pack_path = tmp.path().join("ext.gtxpack");
    {
        let f = std::fs::File::create(&pack_path).unwrap();
        let mut zip = zip::ZipWriter::new(f);
        let options: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("describe.json", options).unwrap();
        zip.write_all(&describe_bytes).unwrap();
        zip.start_file("extension.wasm", options).unwrap();
        zip.write_all(b"malicious-payload").unwrap();
        zip.start_file("manifest.json", options).unwrap();
        zip.write_all(&manifest_bytes).unwrap();
        zip.finish().unwrap();
    }

    let out = Command::new(gtdx_bin())
        .arg("verify")
        .arg(&pack_path)
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "verify should reject a pack whose wasm does not match its manifest"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("integrity"),
        "expected an integrity error in stderr, got: {stderr}"
    );
}

/// Build a manifest-bound, signed `.gtxpack` at `pack_path` from `fx`'s
/// describe + wasm, signed with `signing_key`. Mirrors the producer order:
/// build manifest → bind → sign → pack.
fn build_bound_signed_pack(
    fx: &ExtensionFixture,
    describe_path: &std::path::Path,
    pack_path: &std::path::Path,
    signing_key: &ed25519_dalek::SigningKey,
    include_manifest: bool,
) {
    use greentic_extension_sdk_contract::{
        DescribeJson, bind_manifest, build_manifest, sign_describe,
    };

    let wasm = std::fs::read(fx.root().join("extension.wasm")).unwrap();
    let manifest = build_manifest(vec![("extension.wasm", wasm.as_slice())]);
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();

    let raw = std::fs::read_to_string(describe_path).unwrap();
    let mut describe: DescribeJson = serde_json::from_str(&raw).unwrap();
    bind_manifest(&mut describe, &manifest_bytes);
    sign_describe(&mut describe, signing_key).unwrap();
    let describe_bytes = serde_json::to_vec_pretty(&describe).unwrap();

    let f = std::fs::File::create(pack_path).unwrap();
    let mut zip = zip::ZipWriter::new(f);
    let options: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("describe.json", options).unwrap();
    zip.write_all(&describe_bytes).unwrap();
    zip.start_file("extension.wasm", options).unwrap();
    zip.write_all(&wasm).unwrap();
    if include_manifest {
        zip.start_file("manifest.json", options).unwrap();
        zip.write_all(&manifest_bytes).unwrap();
    }
    zip.finish().unwrap();
}

fn pubkey_b64(signing_key: &ed25519_dalek::SigningKey) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(signing_key.verifying_key().to_bytes())
}

#[test]
fn verify_runs_full_chain_on_manifest_bound_pack() {
    let tmp = TempDir::new().unwrap();
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&[5u8; 32]);
    let (fx, describe_path) = new_describe_fixture();
    let pack_path = tmp.path().join("ext.gtxpack");
    build_bound_signed_pack(&fx, &describe_path, &pack_path, &signing_key, true);

    // Plain verify: self-consistency + manifest binding + archive ledger.
    let out = Command::new(gtdx_bin())
        .arg("verify")
        .arg(&pack_path)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Anchored verify against the correct key: full C1+C2 chain.
    let out = Command::new(gtdx_bin())
        .arg("verify")
        .arg(&pack_path)
        .arg("--trusted-key")
        .arg(pubkey_b64(&signing_key))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "anchored verify stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        stdout.contains("(anchored)"),
        "anchored verify should note it, got: {stdout}"
    );
}

#[test]
fn verify_rejects_wrong_trusted_key() {
    let tmp = TempDir::new().unwrap();
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&[5u8; 32]);
    let attacker_key = ed25519_dalek::SigningKey::from_bytes(&[6u8; 32]);
    let (fx, describe_path) = new_describe_fixture();
    let pack_path = tmp.path().join("ext.gtxpack");
    build_bound_signed_pack(&fx, &describe_path, &pack_path, &signing_key, true);

    let out = Command::new(gtdx_bin())
        .arg("verify")
        .arg(&pack_path)
        .arg("--trusted-key")
        .arg(pubkey_b64(&attacker_key))
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "verify must reject a signature not produced by the trusted key"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("authenticity"),
        "expected an authenticity error, got: {stderr}"
    );
}

#[test]
fn verify_rejects_bound_describe_with_missing_manifest() {
    // A describe that claims a manifest binding over an archive that dropped its
    // manifest.json — a removed ledger, which must hard-fail (not warn).
    let tmp = TempDir::new().unwrap();
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&[5u8; 32]);
    let (fx, describe_path) = new_describe_fixture();
    let pack_path = tmp.path().join("ext.gtxpack");
    build_bound_signed_pack(&fx, &describe_path, &pack_path, &signing_key, false);

    let out = Command::new(gtdx_bin())
        .arg("verify")
        .arg(&pack_path)
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "bound describe + no manifest.json must fail"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("ledger"),
        "expected a removed-ledger error, got: {stderr}"
    );
}
