//! Shared helpers for the `cli_new` integration tests.

use std::process::Command;

/// Path to the `gtdx` binary under test.
///
/// `CARGO_BIN_EXE_*` resolves to the binary cargo built for *this* test run.
/// The previous hardcoded `target/debug/gtdx` silently tested a stale binary
/// under `--release` or a custom `CARGO_TARGET_DIR`.
pub fn gtdx_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_gtdx"))
}

pub fn run(cmd: &mut Command) -> (bool, String, String) {
    let out = cmd.output().expect("spawn gtdx");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}
