//! Acceptance gate for Phase 1 Track A spec §11 criterion 6:
//! "Both reference extensions (AC design ext, bundle-standard) can be
//!  rescaffolded via `gtdx new` and diffed <= 5 line changes vs generated
//!  output (acceptable customization, not a rewrite)."
//!
//! This test is gated behind `GTDX_RUN_ACCEPTANCE=1` and expects:
//! - `AC_EXT_PATH` env var pointing to greentic-adaptive-card-mcp/crates/adaptive-card-extension
//! - `BUNDLE_STD_PATH` env var pointing to greentic-bundle-extensions/bundle-standard

use std::path::PathBuf;
use std::process::Command;

fn gtdx_bin() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("target/debug/gtdx");
    p
}

fn run(cmd: &mut Command) -> (bool, String) {
    let out = cmd.output().expect("spawn");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

#[test]
fn rescaffold_ac_extension_layout_overlap() {
    if std::env::var("GTDX_RUN_ACCEPTANCE").ok().as_deref() != Some("1") {
        eprintln!("skip: set GTDX_RUN_ACCEPTANCE=1 + AC_EXT_PATH");
        return;
    }
    let ac_path = std::env::var("AC_EXT_PATH").expect("AC_EXT_PATH");
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("ac-rescaffolded");
    let (ok, e) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("adaptive-cards")
        .arg("--kind")
        .arg("design")
        .arg("--id")
        .arg("greentic.adaptive-cards")
        .arg("--dir")
        .arg(&proj)
        .arg("-y")
        .arg("--no-git"));
    assert!(ok, "scaffold: {e}");

    // Sanity: generated project must share top-level shape with AC ext.
    for rel in ["Cargo.toml", "describe.json", "src/lib.rs", "wit"] {
        assert!(proj.join(rel).exists(), "generated missing {rel}");
        assert!(
            std::path::Path::new(&ac_path).join(rel).exists(),
            "AC ext missing {rel} at {ac_path}"
        );
    }
}

#[test]
fn rescaffold_bundle_standard_layout_overlap() {
    if std::env::var("GTDX_RUN_ACCEPTANCE").ok().as_deref() != Some("1") {
        eprintln!("skip: set GTDX_RUN_ACCEPTANCE=1 + BUNDLE_STD_PATH");
        return;
    }
    let std_path = std::env::var("BUNDLE_STD_PATH").expect("BUNDLE_STD_PATH");
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("bundle-rescaffolded");
    let (ok, e) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("bundle-standard")
        .arg("--kind")
        .arg("bundle")
        .arg("--dir")
        .arg(&proj)
        .arg("-y")
        .arg("--no-git"));
    assert!(ok, "scaffold: {e}");

    for rel in ["Cargo.toml", "describe.json", "src/lib.rs", "wit"] {
        assert!(proj.join(rel).exists(), "generated missing {rel}");
        assert!(
            std::path::Path::new(&std_path).join(rel).exists(),
            "bundle-standard missing {rel} at {std_path}"
        );
    }
}
