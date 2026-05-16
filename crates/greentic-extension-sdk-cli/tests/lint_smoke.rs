//! Integration coverage for `gtdx lint`:
//! - smoke: subcommand listed in `--help`, clean fixture exits zero
//! - per-violation: each bad fixture exits non-zero and stderr names the rule
//!
//! Audit-driven rules covered here:
//!   `E_VERSION_SEMVER` — `metadata.version` not parseable as semver
//!   `E_RUNTIME_REF` — `nodeTypes` `runtime_ref` points at unknown runtime
//!                     component key
//!   `E_CAP_CYCLE` — extension requires a capability it itself offers

use std::process::Command;

fn run_lint(fixture: &str) -> (bool, String, String) {
    let gtdx = env!("CARGO_BIN_EXE_gtdx");
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/lint")
        .join(fixture);
    let out = Command::new(gtdx)
        .args(["lint", "--dir"])
        .arg(&dir)
        .output()
        .expect("run gtdx lint");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn lint_subcommand_listed_in_help() {
    let gtdx = env!("CARGO_BIN_EXE_gtdx");
    let out = Command::new(gtdx)
        .arg("--help")
        .output()
        .expect("run --help");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("lint"),
        "gtdx --help missing `lint`:\n{stdout}",
    );
}

#[test]
fn lint_clean_fixture_exits_zero() {
    let (ok, stdout, stderr) = run_lint("clean");
    assert!(
        ok,
        "lint on clean fixture failed:\nstdout: {stdout}\nstderr: {stderr}",
    );
    assert!(
        stdout.contains("lint clean"),
        "expected success marker in stdout: {stdout}",
    );
}

#[test]
fn invalid_semver_fails_with_e_version_semver() {
    let (ok, _stdout, stderr) = run_lint("bad_semver");
    assert!(!ok, "lint should fail for bad_semver");
    assert!(
        stderr.contains("E_VERSION_SEMVER"),
        "stderr missing E_VERSION_SEMVER: {stderr}",
    );
    assert!(
        stderr.contains("not valid semver"),
        "stderr missing 'not valid semver': {stderr}",
    );
}

#[test]
fn dangling_runtime_ref_fails_with_e_runtime_ref() {
    let (ok, _stdout, stderr) = run_lint("dangling_runtime_ref");
    assert!(!ok, "lint should fail for dangling_runtime_ref");
    assert!(
        stderr.contains("E_RUNTIME_REF"),
        "stderr missing E_RUNTIME_REF: {stderr}",
    );
    assert!(
        stderr.contains("not declared in runtime.components"),
        "stderr missing reason: {stderr}",
    );
}

#[test]
fn capability_self_cycle_fails_with_e_cap_cycle() {
    let (ok, _stdout, stderr) = run_lint("cap_cycle");
    assert!(!ok, "lint should fail for cap_cycle");
    assert!(
        stderr.contains("E_CAP_CYCLE"),
        "stderr missing E_CAP_CYCLE: {stderr}",
    );
    assert!(
        stderr.contains("self-cycle"),
        "stderr missing 'self-cycle': {stderr}",
    );
}
