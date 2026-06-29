//! Integration tests for `gtdx disable`.

use tempfile::TempDir;

fn gtdx_bin() -> std::path::PathBuf {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("target/debug/gtdx");
    p
}

#[test]
fn disable_sets_state_false() {
    let tmp = TempDir::new().unwrap();
    std::fs::create_dir_all(tmp.path().join("extensions/design/test.bar-0.1.0")).unwrap();

    let status = std::process::Command::new(gtdx_bin())
        .args([
            "--home",
            tmp.path().to_str().unwrap(),
            "disable",
            "test.bar@0.1.0",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let content = std::fs::read_to_string(tmp.path().join("extensions-state.json")).unwrap();
    assert!(content.contains("\"test.bar@0.1.0\""));
    assert!(content.contains("false"));
}

#[test]
fn disable_warns_when_dependent_extension_present() {
    let tmp = TempDir::new().unwrap();

    // Provider with offered capability.
    let provider_dir = tmp.path().join("extensions/design/test.cap-provider-0.1.0");
    std::fs::create_dir_all(&provider_dir).unwrap();
    std::fs::write(
        provider_dir.join("describe.json"),
        r#"{
        "metadata": { "id": "test.cap-provider" },
        "capabilities": { "offered": ["test:cap-x"] }
    }"#,
    )
    .unwrap();

    // Consumer with required capability.
    let consumer_dir = tmp.path().join("extensions/design/test.cap-consumer-0.1.0");
    std::fs::create_dir_all(&consumer_dir).unwrap();
    std::fs::write(
        consumer_dir.join("describe.json"),
        r#"{
        "metadata": { "id": "test.cap-consumer" },
        "capabilities": { "required": ["test:cap-x"] }
    }"#,
    )
    .unwrap();

    let output = std::process::Command::new(gtdx_bin())
        .args([
            "--home",
            tmp.path().to_str().unwrap(),
            "disable",
            "test.cap-provider@0.1.0",
        ])
        .output()
        .unwrap();
    // Disable does NOT block — must succeed even with dependents.
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("test.cap-consumer"), "stderr was: {stderr}");
    assert!(stderr.contains("test:cap-x"), "stderr was: {stderr}");
}
