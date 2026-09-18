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

    let output = Command::new(gtdx_bin())
        .arg("--home")
        .arg(&home)
        .arg("outdated")
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

    let output = Command::new(gtdx_bin())
        .arg("--home")
        .arg(&home)
        .arg("outdated")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("greentic.foo"), "stdout was: {stdout}");
    // The default registry is the built-in `greentic-store`, which resolves
    // without a `config.toml` entry — so "not configured" is exactly what must
    // NOT be claimed here. This assertion used to require that message,
    // codifying the bug: `outdated` looked only at `config.toml` and told the
    // user to configure a store that already works. The extension is still
    // reported (Unknown when the store has nothing for it), and the command
    // still exits 0.
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("not configured"),
        "the built-in store must not be reported as unconfigured: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The public `greentic-store` is built in — the README is explicit that
/// `gtdx registries add` is not required for it. `outdated` looked the default
/// registry up in `config.toml` alone, so on any home without an explicit
/// entry it announced the store was "not configured" and told the user to go
/// configure the thing that already works.
#[test]
fn outdated_does_not_claim_the_builtin_store_is_unconfigured() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    install_fixture(&home);

    let output = Command::new(gtdx_bin())
        .arg("--home")
        .arg(&home)
        .arg("outdated")
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    assert!(
        !stderr.contains("not configured"),
        "the built-in store must not be reported as unconfigured: {stderr}"
    );
}

/// A registry name that genuinely is not configured still warns — and still
/// exits 0 with an Unknown row, rather than failing the whole command.
#[test]
fn outdated_reports_a_genuinely_unknown_registry_without_failing() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    install_fixture(&home);

    let output = Command::new(gtdx_bin())
        .arg("--home")
        .arg(&home)
        .arg("outdated")
        .arg("--registry")
        .arg("no-such-registry")
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "must still exit 0: {stderr}");
    assert!(
        stderr.contains("no-such-registry"),
        "warning should name the registry: {stderr}"
    );
}

/// Put one installed extension on disk so `outdated` reaches registry
/// resolution instead of short-circuiting on "No extensions installed."
fn install_fixture(home: &std::path::Path) {
    let dir = home
        .join("extensions")
        .join(ExtensionKind::Design.dir_name())
        .join("greentic.fixture-0.1.0");
    std::fs::create_dir_all(&dir).unwrap();
    let fixture = ExtensionFixtureBuilder::new(ExtensionKind::Design, "greentic.fixture", "0.1.0")
        .offer("greentic:perm/x", "1.0.0")
        .with_wasm(b"wasm".to_vec())
        .build()
        .unwrap();
    std::fs::copy(&fixture.describe_path, dir.join("describe.json")).unwrap();
}
