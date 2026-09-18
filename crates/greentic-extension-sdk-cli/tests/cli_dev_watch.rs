//! Smoke test for `gtdx dev` watch mode.
//!
//! Gated behind `GTDX_RUN_SMOKE=1` because it:
//!   * spawns a long-lived gtdx process,
//!   * requires cargo-component on PATH,
//!   * is timing-sensitive on slow CI hardware.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
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

    // Drain stdout for the whole run. Reading only until "ready." leaves the
    // pipe undrained, and the child then blocks on a full pipe part-way
    // through a build — which looks exactly like "the rebuild never happened".
    let stdout = child.stdout.take().unwrap();
    let seen = Arc::new(Mutex::new(String::new()));
    let sink = Arc::clone(&seen);
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            eprintln!("child: {line}");
            if let Ok(mut buf) = sink.lock() {
                buf.push_str(&line);
                buf.push('\n');
            }
        }
    });

    let deadline = Instant::now() + Duration::from_secs(30);
    let mut saw_ready = false;
    while Instant::now() < deadline {
        if seen.lock().is_ok_and(|b| b.contains("ready.")) {
            saw_ready = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(saw_ready, "gtdx dev never emitted ready within 30s");

    // edit a file
    let src = proj.join("src/lib.rs");
    let orig = std::fs::read_to_string(&src).unwrap();
    std::fs::write(&src, format!("{orig}\n// bump")).unwrap();

    // the rebuild should land within 15s (generous)
    std::thread::sleep(Duration::from_secs(15));

    // Storage path uses describe.metadata.id ("greentic.demo"), not name. The
    // default id namespace moved from `com.example.` to `greentic.` when the
    // scaffold stopped emitting an id its own linter rejected (E_ID_PATTERN);
    // this assertion kept the old path and had been failing ever since —
    // invisibly, because the whole file is gated behind GTDX_RUN_SMOKE.
    let installed = home.join("extensions/design/greentic.demo-0.1.0");
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

/// `gtdx dev` used to feed itself: a build re-touches `src/bindings.rs`,
/// `wit/deps/` and `Cargo.toml`, the watcher fired on those, and the next
/// build did it again — roughly three builds a second on an untouched
/// scaffold, forever, each reporting "pack sha256 unchanged".
///
/// Left alone, the watcher must stay quiet.
#[test]
fn dev_watch_does_not_rebuild_itself() {
    if !gate() {
        eprintln!("skipped: set GTDX_RUN_SMOKE=1 to enable");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("quiet");
    let home = tmp.path().join("home");

    let status = Command::new(gtdx_bin())
        .arg("new")
        .arg("quiet")
        .arg("--dir")
        .arg(&proj)
        .arg("--author")
        .arg("tester")
        .arg("-y")
        .arg("--no-git")
        .status()
        .unwrap();
    assert!(status.success());

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

    // Count builds while it runs, not from the pipe afterwards: a regressed
    // watcher floods stdout, blocks on a full pipe, and would then report
    // *fewer* builds than it performed — this test has to fail closed.
    let stdout = child.stdout.take().unwrap();
    let builds = Arc::new(Mutex::new(0usize));
    let counter = Arc::clone(&builds);
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if line.contains("building (")
                && let Ok(mut n) = counter.lock()
            {
                *n += 1;
            }
        }
    });

    // Let it settle, touching nothing.
    std::thread::sleep(Duration::from_secs(20));
    let _ = child.kill();
    let _ = child.wait();

    let builds = *builds.lock().unwrap();

    assert!(
        builds <= 2,
        "watcher rebuilt {builds} times without any edit — it is retriggering on its own output"
    );
}
