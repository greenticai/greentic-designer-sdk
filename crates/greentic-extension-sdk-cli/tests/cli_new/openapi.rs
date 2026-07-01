use std::process::Command;
use super::fixtures::{gtdx_bin, run};

#[test]
fn yes_flag_skips_wizard_and_scaffolds_noninteractively() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("q");
    // No stdin attached; -y must prevent any prompt and scaffold the echo mcp skeleton.
    let (ok, _o, e) = run(Command::new(gtdx_bin())
        .arg("new").arg("q")
        .arg("--kind").arg("mcp")
        .arg("--dir").arg(&proj)
        .arg("-y").arg("--no-git").arg("--force"));
    assert!(ok, "stderr:\n{e}");
    assert!(proj.join("describe.json").exists());
}

#[test]
fn from_openapi_requires_kind_mcp() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("demo");
    // --kind design + --from-openapi must be rejected.
    let (ok, _o, e) = run(Command::new(gtdx_bin())
        .arg("new").arg("demo")
        .arg("--kind").arg("design")
        .arg("--from-openapi").arg("api.yaml")
        .arg("--dir").arg(&proj)
        .arg("-y").arg("--no-git").arg("--force"));
    assert!(!ok, "expected failure");
    assert!(e.contains("--from-openapi"), "stderr should explain the flag constraint:\n{e}");
}

#[cfg(unix)]
#[test]
fn from_openapi_generates_and_authors_describe() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    // Stub greentic-mcp-gen: writes <stem>.component.wasm + <stem>.component-meta.json into --output-dir.
    let stub = tmp.path().join("greentic-mcp-gen");
    std::fs::write(&stub, r#"#!/bin/sh
# args: --spec <spec> --output-dir <dir>
OUT=""
while [ $# -gt 0 ]; do case "$1" in --output-dir) OUT="$2"; shift 2;; *) shift;; esac; done
printf '(module)' > "$OUT/petstore.component.wasm"
cat > "$OUT/petstore.component-meta.json" <<JSON
{"servers":["https://petstore.example.com"],"secret_requirements":[{"key":"PETSTORE_KEY","required":true}],"oauth_scopes":[]}
JSON
"#).unwrap();
    let mut perms = std::fs::metadata(&stub).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&stub, perms).unwrap();

    let spec = tmp.path().join("petstore.yaml");
    std::fs::write(&spec, "openapi: 3.0.0\n").unwrap();
    let proj = tmp.path().join("petstore-ext");

    let (ok, _o, e) = run(Command::new(gtdx_bin())
        .env("GTDX_MCP_GEN_BIN", &stub)
        .arg("new").arg("petstore-ext")
        .arg("--kind").arg("mcp")
        .arg("--from-openapi").arg(&spec)
        .arg("--dir").arg(&proj)
        .arg("-y").arg("--no-git").arg("--force"));
    assert!(ok, "stderr:\n{e}");

    // generated wasm present
    assert!(proj.join("petstore.component.wasm").exists());
    // describe.json authored with network + secrets from the meta sidecar
    let describe: serde_json::Value =
        serde_json::from_slice(&std::fs::read(proj.join("describe.json")).unwrap()).unwrap();
    assert_eq!(describe["kind"], "wasix:mcp/router");
    assert_eq!(describe["runtime"]["permissions"]["network"], serde_json::json!(["https://petstore.example.com"]));
    assert_eq!(describe["secret_requirements"][0]["key"], "PETSTORE_KEY");
}
