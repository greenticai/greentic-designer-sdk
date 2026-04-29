//! Integration tests for `gtdx dev --once`.
//!
//! These tests exercise a full cargo-component build cycle and are therefore
//! gated behind `GTDX_RUN_BUILD=1` so CI matrices without the cargo-component
//! toolchain stay green.

use std::path::PathBuf;
use std::process::Command;

fn gtdx_bin() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("target/debug/gtdx");
    p
}

fn gate() -> bool {
    std::env::var("GTDX_RUN_BUILD").ok().as_deref() == Some("1")
}

fn run(cmd: &mut Command) -> (bool, String, String) {
    let out = cmd.output().expect("spawn");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn dev_once_no_install_packs_design_extension() {
    if !gate() {
        eprintln!("skipped: set GTDX_RUN_BUILD=1 to enable (requires cargo-component)");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("demo");
    let home = tmp.path().join("home");

    // 1) scaffold
    let (ok, o, e) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("demo")
        .arg("--dir")
        .arg(&proj)
        .arg("--author")
        .arg("tester")
        .arg("-y")
        .arg("--no-git"));
    assert!(ok, "gtdx new failed: {o}\n{e}");

    // 2) run dev --once --no-install
    let (ok, o, e) = run(Command::new(gtdx_bin())
        .env("GREENTIC_HOME", &home)
        .arg("dev")
        .arg("--once")
        .arg("--no-install")
        .arg("--manifest")
        .arg(proj.join("Cargo.toml")));
    assert!(ok, "gtdx dev --once failed: {o}\n{e}");

    // 3) assert pack landed
    let dist = proj.join("dist");
    let entries: Vec<_> = std::fs::read_dir(&dist).unwrap().flatten().collect();
    let pack = entries
        .iter()
        .find(|e| e.path().extension().and_then(|s| s.to_str()) == Some("gtxpack"))
        .expect("expected a .gtxpack in dist/");
    assert!(std::fs::metadata(pack.path()).unwrap().len() > 0);
}

#[test]
fn dev_once_json_format_emits_valid_jsonl() {
    if !gate() {
        eprintln!("skipped: set GTDX_RUN_BUILD=1 to enable");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("demo");
    let home = tmp.path().join("home");

    assert!(
        Command::new(gtdx_bin())
            .arg("new")
            .arg("demo")
            .arg("--dir")
            .arg(&proj)
            .arg("--author")
            .arg("tester")
            .arg("-y")
            .arg("--no-git")
            .status()
            .unwrap()
            .success()
    );

    let (ok, stdout, stderr) = run(Command::new(gtdx_bin())
        .env("GREENTIC_HOME", &home)
        .arg("dev")
        .arg("--once")
        .arg("--no-install")
        .arg("--format")
        .arg("json")
        .arg("--manifest")
        .arg(proj.join("Cargo.toml")));
    assert!(ok, "stderr: {stderr}");

    let lines: Vec<_> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();
    assert!(!lines.is_empty());
    for line in &lines {
        let v: serde_json::Value = serde_json::from_str(line).unwrap_or_else(|e| {
            panic!("not valid JSON: {line}\nerror: {e}");
        });
        assert!(v.get("ts").is_some(), "missing ts field in {line}");
        assert!(v.get("event").is_some(), "missing event field in {line}");
    }
    // At least one build_start and one pack_ok
    let events: Vec<_> = lines
        .iter()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter_map(|v| v["event"].as_str().map(str::to_string))
        .collect();
    assert!(events.contains(&"build_start".into()));
    assert!(events.contains(&"pack_ok".into()));
}
