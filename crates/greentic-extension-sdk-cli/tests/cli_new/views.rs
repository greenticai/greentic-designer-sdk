//! `gtdx new --with-view` must produce a project that lints and validates on
//! the first run. A scaffold that emits an empty page teaches nothing — the
//! same lesson 1.2.7 and 1.2.8 already paid for on the other kinds.

use std::process::Command;

use crate::fixtures::{gtdx_bin, run};

#[test]
fn scaffold_with_view_produces_a_lintable_project() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let target = tmp.path().join("viewy");

    let (ok, _out, err) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("viewy")
        .arg("--kind")
        .arg("design")
        .arg("--with-view")
        .arg("--no-git")
        .arg("--dir")
        .arg(&target));
    assert!(ok, "scaffold failed: {err}");

    let entry = target.join("assets/views/hello/index.html");
    assert!(
        entry.exists(),
        "example page must exist at {}",
        entry.display()
    );
    assert!(target.join("assets/views/hello/bridge.js").exists());
    assert!(target.join("assets/views/hello/app.js").exists());

    let describe: serde_json::Value = serde_json::from_slice(
        &std::fs::read(target.join("describe.json")).expect("read describe"),
    )
    .expect("parse describe");
    let views = describe["contributions"]["views"]
        .as_array()
        .expect("views array");
    assert_eq!(views.len(), 1);
    assert_eq!(views[0]["id"], "hello");
    assert_eq!(views[0]["entry"], "index.html");
    assert!(
        describe["runtime"]["permissions"]["ui"].is_object(),
        "a scaffolded view must come with its permissions.ui block"
    );

    let (lint_ok, _o, lint_err) = run(Command::new(gtdx_bin())
        .arg("lint")
        .arg("--dir")
        .arg(&target));
    assert!(
        lint_ok,
        "a fresh --with-view scaffold must lint clean: {lint_err}"
    );
}

#[test]
fn scaffold_without_the_flag_ships_no_view() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let target = tmp.path().join("plain");

    let (ok, _out, err) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("plain")
        .arg("--kind")
        .arg("design")
        .arg("--no-git")
        .arg("--dir")
        .arg(&target));
    assert!(ok, "scaffold failed: {err}");

    assert!(
        !target.join("assets").exists(),
        "no view means no assets dir"
    );
    let describe: serde_json::Value = serde_json::from_slice(
        &std::fs::read(target.join("describe.json")).expect("read describe"),
    )
    .expect("parse describe");
    assert!(describe["contributions"].get("views").is_none());
}

#[test]
fn with_view_is_rejected_for_kind_mcp() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let target = tmp.path().join("routery");

    let (ok, _out, err) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("routery")
        .arg("--kind")
        .arg("mcp")
        .arg("--with-view")
        .arg("--no-git")
        .arg("--dir")
        .arg(&target));
    assert!(!ok, "mcp artifacts carry no contributions block at all");
    assert!(
        err.contains("--with-view"),
        "the error must name the flag it rejected: {err}"
    );
}
