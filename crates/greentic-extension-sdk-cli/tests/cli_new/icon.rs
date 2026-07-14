//! `gtdx new --icon` integration tests.

use std::process::Command;

use crate::fixtures::{gtdx_bin, run};

#[test]
fn new_with_icon_sets_metadata_and_assets() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("demo");
    let icon = tmp.path().join("logo.svg");
    std::fs::write(&icon, b"<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>").unwrap();

    let (ok, _out, err) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("demo")
        .arg("--dir")
        .arg(&proj)
        .arg("-y")
        .arg("--no-git")
        .arg("--icon")
        .arg(&icon));
    assert!(ok, "gtdx new --icon failed: {err}");

    assert!(
        proj.join("assets/icon.svg").exists(),
        "assets/icon.svg missing"
    );
    let d: serde_json::Value =
        serde_json::from_slice(&std::fs::read(proj.join("describe.json")).unwrap()).unwrap();
    assert_eq!(d["metadata"]["icon"], "assets/icon.svg");
}
