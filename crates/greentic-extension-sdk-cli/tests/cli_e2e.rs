use std::process::Command;

use greentic_extension_sdk_contract::ExtensionKind;
use greentic_extension_sdk_testing::{ExtensionFixtureBuilder, pack_directory};
use tempfile::TempDir;

fn gtdx_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_gtdx"))
}

#[test]
fn validate_command_accepts_valid_extension() {
    let fixture = ExtensionFixtureBuilder::new(ExtensionKind::Design, "greentic.cli-test", "0.1.0")
        .offer("greentic:cli/y", "1.0.0")
        .with_wasm(b"wasm".to_vec())
        .build()
        .unwrap();

    let output = Command::new(gtdx_bin())
        .arg("validate")
        .arg(fixture.root())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn install_from_local_pack_copies_into_home() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    let pack_dir = tmp.path().join("packs");
    std::fs::create_dir_all(&pack_dir).unwrap();

    let fixture =
        ExtensionFixtureBuilder::new(ExtensionKind::Design, "greentic.cli-install", "0.1.0")
            .offer("greentic:ci/y", "1.0.0")
            .with_wasm(b"wasm".to_vec())
            .build()
            .unwrap();
    let pack = pack_dir.join("greentic.cli-install-0.1.0.gtxpack");
    pack_directory(fixture.root(), &pack).unwrap();

    let output = Command::new(gtdx_bin())
        .arg("--home")
        .arg(&home)
        .arg("install")
        .arg(&pack)
        .arg("-y")
        .arg("--trust")
        .arg("loose")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        home.join("extensions/design/greentic.cli-install-0.1.0/describe.json")
            .exists()
    );
}

#[test]
fn list_shows_installed_extensions() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    let design_dir = home.join("extensions/design/greentic.demo-0.1.0");
    std::fs::create_dir_all(&design_dir).unwrap();

    let fixture = ExtensionFixtureBuilder::new(ExtensionKind::Design, "greentic.demo", "0.1.0")
        .offer("greentic:d/y", "1.0.0")
        .build()
        .unwrap();
    std::fs::copy(
        fixture.root().join("describe.json"),
        design_dir.join("describe.json"),
    )
    .unwrap();

    let output = Command::new(gtdx_bin())
        .arg("--home")
        .arg(&home)
        .arg("list")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("greentic.demo@0.1.0"), "got: {stdout}");
}

#[test]
fn doctor_exits_zero_on_empty_home() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();

    let output = Command::new(gtdx_bin())
        .arg("--home")
        .arg(&home)
        .arg("doctor")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Doctor now prints 4 sections; on a fresh home it reports no installed
    // extensions and should exit 0 with "all checks passed".
    assert!(stdout.contains("installed extensions"), "got: {stdout}");
    assert!(stdout.contains("all checks passed"), "got: {stdout}");
}

#[test]
fn registries_list_shows_default_only() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");

    let output = Command::new(gtdx_bin())
        .arg("--home")
        .arg(&home)
        .arg("registries")
        .arg("list")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("default: greentic-store"), "got: {stdout}");
}
