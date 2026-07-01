use std::process::Command;
use super::fixtures::{gtdx_bin, run};

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
