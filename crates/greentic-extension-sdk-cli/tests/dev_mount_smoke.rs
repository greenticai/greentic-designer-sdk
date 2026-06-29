//! `gtdx dev --mount <path>` exists and is mutually exclusive with the
//! loop modes (`--watch` / `--once`).

use std::process::Command;

fn gtdx() -> &'static str {
    env!("CARGO_BIN_EXE_gtdx")
}

#[test]
fn dev_help_lists_mount_flag() {
    let out = Command::new(gtdx())
        .args(["dev", "--help"])
        .output()
        .expect("run gtdx dev --help");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("--mount"),
        "gtdx dev --help missing --mount: {stdout}",
    );
}

#[test]
fn mount_conflicts_with_once() {
    let out = Command::new(gtdx())
        .args(["dev", "--mount", "/tmp/does-not-matter", "--once"])
        .output()
        .expect("run gtdx dev");
    assert!(!out.status.success(), "expected non-zero exit on conflict");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("cannot be used with"),
        "expected conflict message, got: {stderr}",
    );
}

#[test]
fn mount_conflicts_with_watch() {
    let out = Command::new(gtdx())
        .args(["dev", "--mount", "/tmp/does-not-matter", "--watch"])
        .output()
        .expect("run gtdx dev");
    assert!(!out.status.success(), "expected non-zero exit on conflict");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("cannot be used with"),
        "expected conflict message, got: {stderr}",
    );
}

#[test]
fn mount_fails_cleanly_on_nonexistent_dir() {
    let out = Command::new(gtdx())
        .args(["dev", "--mount", "/this/path/does/not/exist"])
        .output()
        .expect("run gtdx dev --mount");
    assert!(
        !out.status.success(),
        "expected non-zero exit on missing dir",
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("canonicalize"),
        "expected canonicalize error message, got: {stderr}",
    );
}
