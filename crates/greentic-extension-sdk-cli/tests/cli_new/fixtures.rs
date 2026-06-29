//! Shared helpers for the `cli_new` integration tests.

use std::process::Command;

pub fn gtdx_bin() -> std::path::PathBuf {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("target/debug/gtdx");
    p
}

pub fn run(cmd: &mut Command) -> (bool, String, String) {
    let out = cmd.output().expect("spawn gtdx");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}
