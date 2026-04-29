//! Integration tests for `gtdx list --status`.

use greentic_ext_contract::{
    DescribeJson, ExtensionKind,
    describe::{Author, Capabilities, Engine, Metadata, Permissions, Runtime},
};
use tempfile::TempDir;

fn gtdx_bin() -> std::path::PathBuf {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("target/debug/gtdx");
    p
}

fn write_design_fixture(home: &std::path::Path, id: &str, version: &str) {
    let dir = home
        .join("extensions")
        .join("design")
        .join(format!("{id}-{version}"));
    std::fs::create_dir_all(&dir).unwrap();

    let describe = DescribeJson {
        schema_ref: None,
        api_version: "greentic.ai/v1".into(),
        kind: ExtensionKind::Design,
        metadata: Metadata {
            id: id.into(),
            name: id.into(),
            version: version.into(),
            summary: format!("Test fixture for {id}"),
            description: None,
            author: Author {
                name: "Test".into(),
                email: None,
                public_key: None,
            },
            license: "MIT".into(),
            homepage: None,
            repository: None,
            keywords: vec![],
            icon: None,
            screenshots: vec![],
        },
        engine: Engine {
            greentic_designer: "*".into(),
            ext_runtime: "*".into(),
        },
        capabilities: Capabilities {
            offered: vec![],
            required: vec![],
        },
        runtime: Runtime {
            component: "extension.wasm".into(),
            memory_limit_mb: 64,
            permissions: Permissions::default(),
            gtpack: None,
        },
        execution: None,
        contributions: serde_json::json!({}),
        signature: None,
    };

    std::fs::write(
        dir.join("describe.json"),
        serde_json::to_string_pretty(&describe).unwrap(),
    )
    .unwrap();
}

#[test]
fn list_status_shows_disabled_extensions() {
    let tmp = TempDir::new().unwrap();
    write_design_fixture(tmp.path(), "test.qux", "0.1.0");

    // Disable it first.
    let s = std::process::Command::new(gtdx_bin())
        .args([
            "--home",
            tmp.path().to_str().unwrap(),
            "disable",
            "test.qux@0.1.0",
        ])
        .status()
        .unwrap();
    assert!(s.success());

    let output = std::process::Command::new(gtdx_bin())
        .args(["--home", tmp.path().to_str().unwrap(), "list", "--status"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("test.qux"), "stdout was: {stdout}");
    assert!(stdout.contains("disabled"), "stdout was: {stdout}");
}

#[test]
fn list_without_status_does_not_show_column() {
    let tmp = TempDir::new().unwrap();
    write_design_fixture(tmp.path(), "test.qux", "0.1.0");

    let output = std::process::Command::new(gtdx_bin())
        .args(["--home", tmp.path().to_str().unwrap(), "list"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("disabled"),
        "did not expect status column, got: {stdout}"
    );
    assert!(
        !stdout.contains("enabled"),
        "did not expect status column, got: {stdout}"
    );
}
