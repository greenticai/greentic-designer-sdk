use std::process::Command;

use greentic_extension_sdk_contract::ExtensionKind;
use greentic_extension_sdk_testing::{ExtensionFixtureBuilder, pack_directory};
use tempfile::TempDir;

fn gtdx_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_gtdx"))
}

#[test]
fn outdated_runs_with_no_extensions_installed() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();

    // Point at a registry that is definitely not configured. A bare `outdated`
    // on a fresh home now resolves to the built-in greentic-store (that is the
    // fix for "no such registry" on first run), which would make this test
    // depend on the network. Naming a missing registry keeps the case the test
    // is actually about — status cannot be determined — deterministic.
    let output = Command::new(gtdx_bin())
        .arg("--home")
        .arg(&home)
        .arg("outdated")
        .arg("--registry")
        .arg("no-such-registry")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn outdated_lists_installed_extension_as_unknown_without_registry() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    // Install a fixture extension directly into the design kind dir.
    let dir = home.join("extensions/design/greentic.foo-1.0.0");
    std::fs::create_dir_all(&dir).unwrap();
    let fixture = ExtensionFixtureBuilder::new(ExtensionKind::Design, "greentic.foo", "1.0.0")
        .offer("greentic:perm/x", "1.0.0")
        .with_wasm(b"wasm".to_vec())
        .build()
        .unwrap();
    let pack = tmp.path().join("p.gtxpack");
    pack_directory(fixture.root(), &pack).unwrap();
    // Copy the fixture's describe.json into the install dir for scanning.
    std::fs::copy(fixture.describe_path.clone(), dir.join("describe.json")).unwrap();

    // Name a registry that is definitely not configured. A bare `outdated` on a
    // fresh home now resolves to the built-in greentic-store — that is the fix
    // for "no such registry" on first run — which would make this test depend
    // on the network. This keeps the case under test (status cannot be
    // determined) deterministic and offline.
    let output = Command::new(gtdx_bin())
        .arg("--home")
        .arg(&home)
        .arg("outdated")
        .arg("--registry")
        .arg("no-such-registry")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("greentic.foo"), "stdout was: {stdout}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("not configured")
            || stdout.contains("not configured"),
        "expected a 'not configured' signal.\nstdout: {stdout}"
    );
}
