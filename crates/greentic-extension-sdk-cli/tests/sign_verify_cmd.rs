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
fn verify_accepts_gtxpack_archive() {
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

    // Zip describe.json + extension.wasm into a .gtxpack archive.
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
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
