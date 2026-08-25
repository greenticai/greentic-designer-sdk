//! `gtdx uninstall` across kinds.

use std::process::Command;

use tempfile::TempDir;

use crate::fixtures::{gtdx_bin, setup_fixture_extensions};

/// `uninstall` swept a hand-written list of kinds that omitted `Provider`, so a
/// provider extension could never be removed — and the command reported
/// success while it stayed on disk.
#[test]
fn gtdx_uninstall_removes_a_provider_extension() {
    let tmp = TempDir::new().unwrap();
    setup_fixture_extensions(tmp.path());
    let installed = tmp
        .path()
        .join("extensions/provider/greentic.provider.telegram-0.2.0");
    assert!(installed.exists(), "fixture should start installed");

    let output = Command::new(gtdx_bin())
        .args([
            "--home",
            tmp.path().to_str().unwrap(),
            "uninstall",
            "greentic.provider.telegram",
            "--version",
            "0.2.0",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!installed.exists(), "provider extension was not removed");
}

/// Removing nothing is a failed removal, not a quiet success: a script that
/// uninstalls and checks the status must be able to tell the difference.
#[test]
fn gtdx_uninstall_fails_when_nothing_matches() {
    let tmp = TempDir::new().unwrap();
    setup_fixture_extensions(tmp.path());

    let output = Command::new(gtdx_bin())
        .args([
            "--home",
            tmp.path().to_str().unwrap(),
            "uninstall",
            "greentic.not.installed",
        ])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "uninstalling a missing extension must exit non-zero"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("nothing to remove"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A version that does not match must not fall through to removing some other
/// version of the same extension.
#[test]
fn gtdx_uninstall_respects_the_version_filter() {
    let tmp = TempDir::new().unwrap();
    setup_fixture_extensions(tmp.path());
    let installed = tmp
        .path()
        .join("extensions/provider/greentic.provider.telegram-0.2.0");

    let output = Command::new(gtdx_bin())
        .args([
            "--home",
            tmp.path().to_str().unwrap(),
            "uninstall",
            "greentic.provider.telegram",
            "--version",
            "9.9.9",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success(), "no 9.9.9 is installed");
    assert!(
        installed.exists(),
        "0.2.0 must survive an unmatched version"
    );
}

/// `enable`/`disable` resolved the installed version by sweeping a hard-coded
/// list of kind directories that omitted `mcp`, so an installed
/// `wasix:mcp/router` extension reported "extension not installed" and could
/// never be toggled. Covered here for every kind so the list cannot drift
/// again for one of them unnoticed.
#[test]
fn gtdx_disable_reaches_every_installed_kind() {
    use greentic_extension_sdk_contract::ExtensionKind;

    for kind in ExtensionKind::ALL {
        let tmp = TempDir::new().unwrap();
        let dir = tmp
            .path()
            .join("extensions")
            .join(kind.dir_name())
            .join("greentic.toggle-me-0.1.0");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("describe.json"),
            r#"{"metadata":{"id":"greentic.toggle-me","version":"0.1.0"},"capabilities":{"offered":[],"required":[]}}"#,
        )
        .unwrap();

        let output = Command::new(gtdx_bin())
            .args([
                "--home",
                tmp.path().to_str().unwrap(),
                "disable",
                "greentic.toggle-me",
            ])
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "disable failed for kind {}: {}",
            kind.dir_name(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
