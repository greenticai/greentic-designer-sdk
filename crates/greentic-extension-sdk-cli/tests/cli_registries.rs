//! `gtdx registries` add/remove semantics.

use std::process::Command;

use tempfile::TempDir;

fn gtdx_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_gtdx"))
}

fn run(home: &std::path::Path, args: &[&str]) -> (bool, String, String) {
    let out = Command::new(gtdx_bin())
        .arg("--home")
        .arg(home)
        .args(args)
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// The README documents `gtdx registries add <name> <url>` as the way to
/// override a registry's URL. Appending instead of replacing left two entries
/// with the same name, and every lookup takes the first — so the override
/// silently kept the old URL.
#[test]
fn adding_an_existing_name_replaces_it() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    assert!(run(home, &["registries", "add", "r", "https://one.example"]).0);
    assert!(run(home, &["registries", "add", "r", "https://two.example"]).0);

    let (ok, stdout, stderr) = run(home, &["registries", "list"]);
    assert!(ok, "stderr: {stderr}");
    assert_eq!(
        stdout.matches("  r  ").count(),
        1,
        "exactly one entry named r: {stdout}"
    );
    assert!(stdout.contains("https://two.example"), "stdout: {stdout}");
    assert!(!stdout.contains("https://one.example"), "stdout: {stdout}");
}

/// A URL that the Store client will refuse at request time must be refused
/// when it is saved, not silently written into config.toml.
#[test]
fn rejects_a_url_the_client_would_refuse() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    for bad in ["", "not-a-url", "ftp://x", "http://insecure.example"] {
        let (ok, _, _) = run(home, &["registries", "add", "bad", bad]);
        assert!(!ok, "should have rejected {bad:?}");
    }

    let (_, stdout, _) = run(home, &["registries", "list"]);
    assert!(!stdout.contains("bad"), "nothing should be saved: {stdout}");
}

/// Loopback stays allowed — the rule is "HTTPS, or HTTP only to localhost",
/// and local development depends on the second half.
#[test]
fn accepts_https_and_loopback_http() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    assert!(
        run(
            home,
            &["registries", "add", "secure", "https://store.example"]
        )
        .0
    );
    assert!(
        run(
            home,
            &["registries", "add", "local", "http://localhost:8080"]
        )
        .0
    );
}

/// Removing a name that was never configured is a failed removal, not a quiet
/// success printing "✓ removed".
#[test]
fn removing_an_unknown_registry_fails() {
    let tmp = TempDir::new().unwrap();
    let (ok, stdout, _) = run(tmp.path(), &["registries", "remove", "never-added"]);
    assert!(!ok, "must exit non-zero");
    assert!(!stdout.contains("removed"), "stdout: {stdout}");
}
