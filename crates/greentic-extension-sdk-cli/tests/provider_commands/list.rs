//! `gtdx list` filtering tests.

use std::process::Command;

use tempfile::TempDir;

use crate::fixtures::{gtdx_bin, setup_fixture_extensions};

#[test]
fn gtdx_list_filters_by_kind_provider() {
    let tmp = TempDir::new().unwrap();
    setup_fixture_extensions(tmp.path());

    let output = Command::new(gtdx_bin())
        .args([
            "--home",
            tmp.path().to_str().unwrap(),
            "list",
            "--kind",
            "provider",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should contain provider extension
    assert!(
        stdout.contains("greentic.provider.telegram"),
        "stdout: {stdout}"
    );

    // Should NOT contain design extension
    assert!(
        !stdout.contains("greentic.design.adaptive-cards"),
        "stdout: {stdout}"
    );

    // Should show [provider] header
    assert!(stdout.contains("[provider]"), "stdout: {stdout}");
}

#[test]
fn gtdx_list_filters_by_kind_design() {
    let tmp = TempDir::new().unwrap();
    setup_fixture_extensions(tmp.path());

    let output = Command::new(gtdx_bin())
        .args([
            "--home",
            tmp.path().to_str().unwrap(),
            "list",
            "--kind",
            "design",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should contain design extension
    assert!(
        stdout.contains("greentic.design.adaptive-cards"),
        "stdout: {stdout}"
    );

    // Should NOT contain provider extension
    assert!(
        !stdout.contains("greentic.provider.telegram"),
        "stdout: {stdout}"
    );

    // Should show [design] header
    assert!(stdout.contains("[design]"), "stdout: {stdout}");
}

#[test]
fn gtdx_list_shows_all_kinds_by_default() {
    let tmp = TempDir::new().unwrap();
    setup_fixture_extensions(tmp.path());

    let output = Command::new(gtdx_bin())
        .args(["--home", tmp.path().to_str().unwrap(), "list"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should contain both design and provider extensions
    assert!(
        stdout.contains("greentic.design.adaptive-cards"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("greentic.provider.telegram"),
        "stdout: {stdout}"
    );
}

#[test]
fn gtdx_list_handles_missing_kind_dir() {
    let tmp = TempDir::new().unwrap();
    std::fs::create_dir_all(tmp.path().join("extensions")).unwrap();

    // Don't create any provider dir
    let output = Command::new(gtdx_bin())
        .args([
            "--home",
            tmp.path().to_str().unwrap(),
            "list",
            "--kind",
            "provider",
        ])
        .output()
        .unwrap();

    // Should succeed with empty output, not panic
    assert!(output.status.success());
}
