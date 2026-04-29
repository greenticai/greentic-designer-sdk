//! Smoke test for `gtdx dev` watch mode.
//!
//! Gated behind `GTDX_RUN_SMOKE=1` because it:
//!   * spawns a long-lived gtdx process,
//!   * requires cargo-component on PATH,
//!   * is timing-sensitive on slow CI hardware.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn gtdx_bin() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("target/debug/gtdx");
    p
}

fn gate() -> bool {
    std::env::var("GTDX_RUN_SMOKE").ok().as_deref() == Some("1")
}

#[test]
fn dev_watch_rebuilds_and_reinstalls_on_source_edit() {
    if !gate() {
        eprintln!("skipped: set GTDX_RUN_SMOKE=1 to enable");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("demo");
    let home = tmp.path().join("home");

    // scaffold
    let status = Command::new(gtdx_bin())
        .arg("new")
        .arg("demo")
        .arg("--dir")
        .arg(&proj)
        .arg("--author")
        .arg("tester")
        .arg("-y")
        .arg("--no-git")
        .status()
        .unwrap();
    assert!(status.success());

    // spawn dev --watch
    let mut child = Command::new(gtdx_bin())
        .env("GREENTIC_HOME", &home)
        .arg("dev")
        .arg("--watch")
        .arg("--manifest")
        .arg(proj.join("Cargo.toml"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let stdout = child.stdout.take().unwrap();
    let reader = BufReader::new(stdout);
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut saw_ready = false;
    for line in reader.lines() {
        let line = line.unwrap_or_default();
        eprintln!("child: {line}");
        if line.contains("ready.") {
            saw_ready = true;
            break;
        }
        if Instant::now() > deadline {
            break;
        }
    }
    assert!(saw_ready, "gtdx dev never emitted ready within 30s");

    // edit a file
    let src = proj.join("src/lib.rs");
    let orig = std::fs::read_to_string(&src).unwrap();
    std::fs::write(&src, format!("{orig}\n// bump")).unwrap();

    // the rebuild should land within 15s (generous)
    std::thread::sleep(Duration::from_secs(15));

    // Storage path uses describe.metadata.id ("com.example.demo"), not name.
    let installed = home.join("extensions/design/com.example.demo-0.1.0");
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        installed.exists(),
        "expected reinstall at {}",
        installed.display()
    );
}

#[test]
fn dev_watch_survives_build_failure() {
    if !gate() {
        eprintln!("skipped: set GTDX_RUN_SMOKE=1 to enable");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("demo");
    let home = tmp.path().join("home");
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
        .unwrap();

    // Introduce a syntax error BEFORE starting dev
    std::fs::write(proj.join("src/lib.rs"), "not rust").unwrap();

    let mut child = Command::new(gtdx_bin())
        .env("GREENTIC_HOME", &home)
        .arg("dev")
        .arg("--watch")
        .arg("--manifest")
        .arg(proj.join("Cargo.toml"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    // Wait briefly for ready + first failure, then check the child is still alive.
    std::thread::sleep(Duration::from_secs(5));

    // Still alive?
    let status = child.try_wait().expect("try_wait");
    assert!(status.is_none(), "dev exited prematurely on build failure");

    let _ = child.kill();
    let _ = child.wait();
}
