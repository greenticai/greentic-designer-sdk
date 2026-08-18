use super::fixtures::{gtdx_bin, run};
use std::process::Command;

#[test]
fn yes_flag_skips_wizard_and_scaffolds_noninteractively() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("q");
    // No stdin attached; -y must prevent any prompt and scaffold the echo mcp skeleton.
    let (ok, _o, e) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("q")
        .arg("--kind")
        .arg("mcp")
        .arg("--dir")
        .arg(&proj)
        .arg("-y")
        .arg("--no-git")
        .arg("--force"));
    assert!(ok, "stderr:\n{e}");
    assert!(proj.join("describe.json").exists());
}

#[test]
fn from_openapi_requires_kind_mcp() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("demo");
    // --kind design + --from-openapi must be rejected.
    let (ok, _o, e) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("demo")
        .arg("--kind")
        .arg("design")
        .arg("--from-openapi")
        .arg("api.yaml")
        .arg("--dir")
        .arg(&proj)
        .arg("-y")
        .arg("--no-git")
        .arg("--force"));
    assert!(!ok, "expected failure");
    assert!(
        e.contains("--from-openapi"),
        "stderr should explain the flag constraint:\n{e}"
    );
}

#[cfg(unix)]
#[test]
fn from_openapi_generates_and_authors_describe() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();

    // Stub greentic-mcp-gen: mimics the real generator's side-effects.
    //
    // It:
    //   1. Parses --spec and --output-dir from its argument list.
    //   2. Creates input/done/error/uploaded dirs in its OWN cwd (as the real
    //      generator does), then moves the --spec file into done/.
    //   3. Writes <stem>.component.wasm + <stem>.component-meta.json to
    //      --output-dir so the caller can locate the artifacts.
    //
    // Before the hermetic fix, steps 1-2 polluted the user's cwd and destroyed
    // the original spec file.  After the fix, those side-effects are confined
    // to a scratch directory inside the process's own temp space.
    let stub = tmp.path().join("greentic-mcp-gen");
    std::fs::write(
        &stub,
        r#"#!/bin/sh
# Parse --spec and --output-dir from args
SPEC=""
OUT=""
while [ $# -gt 0 ]; do
    case "$1" in
        --spec)       SPEC="$2"; shift 2;;
        --output-dir) OUT="$2";  shift 2;;
        *)            shift;;
    esac
done

# Real-generator side-effect: create bookkeeping dirs in cwd then move spec
mkdir -p input done error uploaded
if [ -n "$SPEC" ] && [ -f "$SPEC" ]; then
    mv "$SPEC" done/
fi

# Emit artifacts into --output-dir
mkdir -p "$OUT"
printf '(module)' > "$OUT/petstore.component.wasm"
cat > "$OUT/petstore.component-meta.json" <<JSON
{"servers":["https://petstore.example.com"],"secret_requirements":[{"key":"PETSTORE_KEY","required":true}],"oauth_scopes":[]}
JSON
"#,
    )
    .unwrap();
    let mut perms = std::fs::metadata(&stub).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&stub, perms).unwrap();

    // Create the spec in a separate "user cwd" dir so we can verify it is
    // untouched and that no junk dirs appear there.
    let user_cwd = tempfile::tempdir().unwrap();
    let spec = user_cwd.path().join("petstore.yaml");
    std::fs::write(&spec, "openapi: 3.0.0\n").unwrap();

    let proj = tmp.path().join("petstore-ext");

    let (ok, _o, e) = run(Command::new(gtdx_bin())
        .env("GTDX_MCP_GEN_BIN", &stub)
        .current_dir(user_cwd.path()) // run as if the user is in their own cwd
        .arg("new")
        .arg("petstore-ext")
        .arg("--kind")
        .arg("mcp")
        .arg("--from-openapi")
        .arg(&spec)
        .arg("--dir")
        .arg(&proj)
        .arg("-y")
        .arg("--no-git")
        .arg("--force"));
    assert!(ok, "stderr:\n{e}");

    // generated wasm present in project dir
    assert!(
        proj.join("petstore.component.wasm").exists(),
        "expected wasm in project dir"
    );

    // describe.json authored with network + secrets from the meta sidecar
    let describe: serde_json::Value =
        serde_json::from_slice(&std::fs::read(proj.join("describe.json")).unwrap()).unwrap();
    assert_eq!(describe["kind"], "wasix:mcp/router");
    assert_eq!(
        describe["runtime"]["permissions"]["network"],
        serde_json::json!(["https://petstore.example.com"])
    );
    assert_eq!(describe["secret_requirements"][0]["key"], "PETSTORE_KEY");

    // --- Hermetic assertions ---

    // The user's original spec file must still exist (not moved/deleted).
    assert!(
        spec.exists(),
        "original spec file was destroyed — generator polluted the user's workspace"
    );

    // No junk dirs in the user's cwd.
    for junk in &["input", "done", "error", "uploaded"] {
        assert!(
            !user_cwd.path().join(junk).exists(),
            "junk dir '{junk}' appeared in user cwd — generator not hermetic"
        );
    }

    // No junk dirs leaked into the generated project dir.
    for junk in &["input", "done", "error", "uploaded"] {
        assert!(
            !proj.join(junk).exists(),
            "junk dir '{junk}' appeared in project dir"
        );
    }
}
