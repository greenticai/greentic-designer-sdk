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

/// Every `--kind` value the compiled `gtdx` binary accepts, read back out of
/// its own `--help` (clap's `[possible values: ...]` line) rather than hand-
/// listed here. This crate ships no `lib` target, so integration tests can't
/// `use` `scaffold::Kind::value_variants()` directly the way `template.rs`'s
/// in-crate unit tests do — this is the equivalent source of truth reachable
/// from outside the crate: the compiled binary's own clap definition. A hand
/// list here would silently go stale exactly the way `gtdx uninstall`'s did.
pub fn all_kind_strs() -> Vec<String> {
    let (ok, stdout, stderr) = run(Command::new(gtdx_bin()).arg("new").arg("--help"));
    assert!(
        ok,
        "gtdx new --help failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let line = stdout
        .lines()
        .find(|l| l.trim_start().starts_with("[possible values:"))
        .unwrap_or_else(|| {
            panic!("gtdx new --help: no `--kind` possible-values line found:\n{stdout}")
        });
    let kinds: Vec<String> = line
        .trim()
        .trim_start_matches("[possible values:")
        .trim_end_matches(']')
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();
    // A derived-but-empty list makes every caller's `for kind in ...` loop
    // pass vacuously — checking nothing while reporting green, exactly the
    // failure mode this whole derivation exists to avoid.
    assert!(
        !kinds.is_empty(),
        "gtdx new --help: parsed zero `--kind` values from:\n{line}"
    );
    kinds
}
