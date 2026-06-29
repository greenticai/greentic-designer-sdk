//! Integration tests for `gtdx enable`.

use tempfile::TempDir;

fn gtdx_bin() -> std::path::PathBuf {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("target/debug/gtdx");
    p
}

#[test]
fn enable_writes_state_file() {
    let tmp = TempDir::new().unwrap();
    std::fs::create_dir_all(tmp.path().join("extensions/design/test.foo-0.1.0")).unwrap();

    let status = std::process::Command::new(gtdx_bin())
        .args([
            "--home",
            tmp.path().to_str().unwrap(),
            "enable",
            "test.foo@0.1.0",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let content = std::fs::read_to_string(tmp.path().join("extensions-state.json")).unwrap();
    assert!(content.contains("\"test.foo@0.1.0\""));
    assert!(content.contains("true"));
}

#[test]
fn enable_errors_when_not_installed() {
    let tmp = TempDir::new().unwrap();
    let output = std::process::Command::new(gtdx_bin())
        .args([
            "--home",
            tmp.path().to_str().unwrap(),
            "enable",
            "missing.ext@0.1.0",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not installed"));
}

#[test]
fn enable_errors_on_ambiguous_version() {
    let tmp = TempDir::new().unwrap();
    std::fs::create_dir_all(tmp.path().join("extensions/design/test.foo-0.1.0")).unwrap();
    std::fs::create_dir_all(tmp.path().join("extensions/design/test.foo-0.2.0")).unwrap();

    let output = std::process::Command::new(gtdx_bin())
        .args(["--home", tmp.path().to_str().unwrap(), "enable", "test.foo"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("ambiguous"));
}

#[test]
fn enable_does_not_match_extension_with_dashed_id_prefix() {
    let tmp = TempDir::new().unwrap();
    // Two installed extensions: one whose id starts with "greentic.foo"
    // and another whose id is just "greentic.foo" but with a dash continuation.
    // Only the dashed-id variant exists.
    std::fs::create_dir_all(tmp.path().join("extensions/design/greentic.foo-bar-0.1.0")).unwrap();

    // Asking to enable `greentic.foo` (the prefix-only id) must FAIL with
    // "not installed" — must not silently match the dashed-id dir.
    let output = std::process::Command::new(gtdx_bin())
        .args([
            "--home",
            tmp.path().to_str().unwrap(),
            "enable",
            "greentic.foo",
        ])
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not installed"), "actual stderr: {stderr}");
}
