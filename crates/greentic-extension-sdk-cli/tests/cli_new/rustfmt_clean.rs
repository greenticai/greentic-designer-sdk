//! Every scaffold template that emits Rust must render to code that is
//! itself rustfmt-clean, or a freshly scaffolded extension fails its own
//! quality gate (`ci/local_check.sh`) on the *very first* run, on a file the
//! author never touched — src/lib.rs, not the generated src/bindings.rs
//! `tests/ci_local_check_gate.rs` already guards. The fix for the generated
//! file is worthless if the template itself ships misformatted.
//!
//! Renders each kind through the real CLI — `gtdx new --kind <kind>` for the
//! six scaffoldable kinds, `gtdx openapi <spec>` for the generated connector
//! — the exact path an author goes through, with realistic values rather
//! than placeholders stripped to junk: the scaffold's own author/id/version
//! defaults, and a real minimal `OpenAPI` 3.0 spec
//! (`src/commands/openapi/fixtures/petstore-min.json`, the same fixture
//! `tests/openapi_generated_compiles.rs` uses) for the connector.
//!
//! `openapi-connector` ships `src/tool_meta.rs` and `src/dispatch.rs` too,
//! but those come from `src/commands/openapi/codegen.rs` (Rust code
//! building strings), not from a `.rs.tmpl` template — out of scope here.

use super::fixtures::{gtdx_bin, run};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The rendered `src/lib.rs` declares `mod bindings;`, and `src/bindings.rs`
/// is only ever produced by a real `cargo component build` (not run here).
/// Stand in an empty file so rustfmt can resolve the module and report on
/// the file actually under test. A real, non-canonically-formatted
/// `src/bindings.rs` is exactly what `ci/local_check.sh.tmpl`'s own fix
/// (`tests/ci_local_check_gate.rs`) makes harmless — irrelevant here.
fn stub_bindings(project: &Path) {
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::write(project.join("src/bindings.rs"), "").unwrap();
}

fn assert_rendered_rs_is_rustfmt_clean(project: &Path, kind: &str, rel: &str) {
    stub_bindings(project);
    let out = Command::new("rustfmt")
        .arg("--check")
        .arg("--edition")
        .arg("2024")
        .arg(project.join(rel))
        .output()
        .unwrap_or_else(|e| panic!("{kind}: spawn rustfmt: {e}"));
    if out.status.success() {
        return;
    }
    // `rel` (e.g. src/lib.rs) resolves its own `mod` declarations, so
    // rustfmt's report can include diffs on sibling/child files this test
    // does not care about: the stub src/bindings.rs (an empty file is not
    // itself canonically formatted — irrelevant, see `stub_bindings`), and
    // for openapi-connector, src/dispatch.rs / src/tool_meta.rs (codegen
    // output, not a `.rs.tmpl` template — out of scope, see the report).
    // Only a diff reported *on `rel` itself* is this test's concern.
    let stdout = String::from_utf8_lossy(&out.stdout);
    let needle = format!("/{rel}:");
    let flagged = stdout
        .lines()
        .any(|l| l.starts_with("Diff in ") && l.contains(&needle));
    assert!(
        !flagged,
        "{kind}: templates/{kind}/{rel}.tmpl renders to code that is not \
         rustfmt-clean on this repo's pinned toolchain — a freshly scaffolded \
         extension would fail its own fmt gate on this file, which the author \
         never touched:\n{stdout}"
    );
}

#[test]
fn every_kind_scaffold_renders_rustfmt_clean_rust() {
    for kind in ["design", "bundle", "deploy", "provider", "llm", "mcp"] {
        let tmp = tempfile::tempdir().unwrap();
        let proj = tmp.path().join("demo");
        let (ok, stdout, stderr) = run(Command::new(gtdx_bin())
            .arg("new")
            .arg("demo")
            .arg("--kind")
            .arg(kind)
            .arg("--dir")
            .arg(&proj)
            .arg("--author")
            .arg("tester")
            .arg("-y")
            .arg("--no-git"));
        assert!(
            ok,
            "{kind}: gtdx new failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
        assert_rendered_rs_is_rustfmt_clean(&proj, kind, "src/lib.rs");
    }
}

#[test]
fn openapi_connector_shell_template_renders_rustfmt_clean_rust() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("petstore");
    let spec = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/commands/openapi/fixtures/petstore-min.json");

    let (ok, stdout, stderr) = run(Command::new(gtdx_bin())
        .arg("openapi")
        .arg(&spec)
        .arg("--out")
        .arg(&proj)
        .arg("--name")
        .arg("petstore"));
    assert!(
        ok,
        "gtdx openapi failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    assert_rendered_rs_is_rustfmt_clean(&proj, "openapi-connector", "src/lib.rs");
}
