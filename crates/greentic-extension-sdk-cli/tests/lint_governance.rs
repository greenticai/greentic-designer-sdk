use std::process::Command;

fn gtdx_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_gtdx"))
}

// A describe.json that violates E_SCHEMA_HOST, E_EXPORT_FORM, E_ENGINE_DEPRECATED.
const DIRTY_DESCRIBE: &str = r#"{
  "$schema": "https://store.greentic.ai/schemas/describe-v2.json",
  "apiVersion": "greentic.ai/v2",
  "kind": "DesignExtension",
  "compat": { "min_designer_version": ">=1.2.0", "min_runner_version": "^1.2.0", "contract_version": "1.2.0" },
  "metadata": { "id": "greentic.dirty", "name": "Dirty", "version": "0.1.0", "summary": "x", "author": { "name": "G" }, "license": "MIT" },
  "engine": { "greenticDesigner": ">=1.2.0", "extRuntime": "^1.2.0" },
  "capabilities": { "offered": [], "required": [] },
  "runtime": { "memoryLimitMB": 32, "permissions": {}, "components": { "main": { "oci_ref": "ghcr.io/x:1", "sha256": "0000000000000000000000000000000000000000000000000000000000000000", "world": "greentic:x/y" } } },
  "contributions": { "tools": [ { "name": "do_thing", "export": "invoke-tool", "runtime_ref": "main" } ] }
}"#;

#[test]
fn lint_fails_on_governance_violations() {
    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("describe.json"), DIRTY_DESCRIBE).unwrap();

    let output = Command::new(gtdx_bin())
        .arg("lint")
        .arg("--dir")
        .arg(tmp.path())
        .output()
        .unwrap();

    assert!(!output.status.success(), "lint must fail on dirty describe");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E_SCHEMA_HOST"), "stderr: {stderr}");
    assert!(stderr.contains("E_EXPORT_FORM"), "stderr: {stderr}");
    assert!(stderr.contains("E_ENGINE_DEPRECATED"), "stderr: {stderr}");
}
