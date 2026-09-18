//! `gtdx doctor` output volume: failures must not be buried under one ✓ line
//! per installed extension.

use std::process::Command;

use tempfile::TempDir;

fn gtdx_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_gtdx"))
}

/// Install `healthy` v2 extensions plus `broken` v1 ones, which the contract
/// rejects — the shape of a real machine that has accumulated legacy installs.
fn seed(home: &std::path::Path, healthy: usize, broken: usize) {
    let dir = home.join("extensions/design");
    std::fs::create_dir_all(&dir).unwrap();
    for i in 0..healthy {
        let d = dir.join(format!("greentic.ok{i}-1.0.0"));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(
            d.join("describe.json"),
            serde_json::to_vec(&serde_json::json!({
                "apiVersion": "greentic.ai/v2",
                "kind": "DesignExtension",
                "compat": {"min_designer_version": ">=1.2.0", "min_runner_version": "^0.12.0", "contract_version": "1.2.4"},
                "metadata": {"id": format!("greentic.ok{i}"), "name": format!("ok{i}"), "version": "1.0.0", "summary": "x", "author": {"name": "a"}, "license": "MIT"},
                "capabilities": {"offered": [], "required": []},
                "runtime": {
                    "components": {
                        format!("ok{i}"): {
                            "gtpack": {"file": "extension.wasm", "sha256": "ab".repeat(32), "pack_id": format!("greentic.ok{i}"), "component_version": "1.0.0"},
                            "sha256": "ab".repeat(32),
                            "world": format!("greentic:ok{i}/extension@1.0.0")
                        }
                    },
                    "permissions": {"network": [], "secrets": [], "callExtensionKinds": []}
                },
                "contributions": {}
            }))
            .unwrap(),
        )
        .unwrap();
    }
    for i in 0..broken {
        let d = dir.join(format!("greentic.old{i}-1.0.0"));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(
            d.join("describe.json"),
            format!(
                r#"{{"apiVersion":"greentic.ai/v1","kind":"DesignExtension","metadata":{{"id":"greentic.old{i}","version":"1.0.0"}}}}"#
            ),
        )
        .unwrap();
    }
}

fn doctor(home: &std::path::Path, extra: &[&str]) -> (bool, String) {
    let out = Command::new(gtdx_bin())
        .arg("--home")
        .arg(home)
        .arg("doctor")
        .args(extra)
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

/// The default run must stay readable as installs accumulate. Every failing
/// extension is still named; the passing ones collapse to a count.
#[test]
fn default_output_does_not_grow_with_healthy_extensions() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    seed(&home, 60, 3);

    let (ok, stdout) = doctor(&home, &[]);
    assert!(!ok, "three v1 extensions are real problems");

    let lines = stdout.lines().count();
    assert!(
        lines < 40,
        "60 healthy extensions should not each get a line; got {lines}:\n{stdout}"
    );
    for i in 0..3 {
        assert!(
            stdout.contains(&format!("greentic.old{i}")),
            "every failing extension must still be named: {stdout}"
        );
    }
    assert!(
        !stdout.contains("greentic.ok0"),
        "passing extensions are summarised, not listed: {stdout}"
    );
}

/// `--verbose` is the escape hatch: the full per-extension listing is still
/// available for anyone who wants to audit it.
#[test]
fn verbose_lists_every_extension() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    seed(&home, 5, 1);

    let (_, stdout) = doctor(&home, &["--verbose"]);
    for i in 0..5 {
        assert!(
            stdout.contains(&format!("greentic.ok{i}")),
            "verbose must name every extension: {stdout}"
        );
    }
}

/// A clean machine still says so, and still exits 0.
#[test]
fn healthy_home_still_passes() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    seed(&home, 4, 0);

    let (ok, stdout) = doctor(&home, &[]);
    assert!(ok, "no problems: {stdout}");
    assert!(stdout.contains("all checks passed"), "got: {stdout}");
}

/// `doctor` walked a kind list that omitted `Provider`, so a broken provider
/// extension was invisible to the one command whose job is finding broken
/// extensions.
#[test]
fn doctor_checks_provider_extensions() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    let dir = home.join("extensions/provider/greentic.bad-provider-1.0.0");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("describe.json"),
        r#"{"apiVersion":"greentic.ai/v1","kind":"ProviderExtension","metadata":{"id":"greentic.bad-provider","version":"1.0.0"}}"#,
    )
    .unwrap();

    let (ok, stdout) = doctor(&home, &[]);
    assert!(!ok, "a v1 provider describe is a problem: {stdout}");
    assert!(
        stdout.contains("greentic.bad-provider"),
        "provider extensions must be checked: {stdout}"
    );
}
