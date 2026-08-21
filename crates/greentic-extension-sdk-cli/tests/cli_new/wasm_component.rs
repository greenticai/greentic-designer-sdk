//! `gtdx new wasm-component` scaffolding.

#[allow(unused_imports)]
use std::process::Command;

#[allow(unused_imports)]
use crate::fixtures::{gtdx_bin, run};

#[test]
fn new_wasm_component_accepts_node_type_id_and_label() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("greentic.test-tool");
    let (ok, stdout, stderr) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("greentic.test-tool")
        .arg("--kind")
        .arg("wasm-component")
        .arg("--node-type-id")
        .arg("test-tool")
        .arg("--label")
        .arg("Test Tool")
        .arg("--dir")
        .arg(&proj)
        .arg("-y")
        .arg("--no-git"));
    assert!(ok, "gtdx new failed\nstdout:\n{stdout}\nstderr:\n{stderr}");
    assert!(proj.join("describe.json").exists());
}

#[test]
fn new_wasm_component_produces_expected_tree() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("greentic.snap-test");
    let (ok, stdout, stderr) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("greentic.snap-test")
        .arg("--kind")
        .arg("wasm-component")
        .arg("--id")
        .arg("greentic.snap-test")
        .arg("--author")
        .arg("Test Author")
        .arg("--node-type-id")
        .arg("snap")
        .arg("--label")
        .arg("Snap")
        .arg("--dir")
        .arg(&proj)
        .arg("-y")
        .arg("--no-git"));
    assert!(ok, "gtdx new failed\nstdout:\n{stdout}\nstderr:\n{stderr}");

    for rel in [
        "Cargo.toml",
        "describe.json",
        "README.md",
        ".gitignore",
        "rust-toolchain.toml",
        // Single crate at the root, exactly like `--kind design`. The old
        // two-crate workspace (`extension/` + `runtime/`) is gone: it
        // duplicated the design crate, drifted against the contract, and put
        // the vendored WIT deps outside the crate's target path, so nothing it
        // generated ever built.
        "src/lib.rs",
        "wit/world.wit",
        "wit/deps/greentic/extension-design/world.wit",
    ] {
        assert!(
            proj.join(rel).exists(),
            "missing expected file: {rel}\nstdout:\n{stdout}"
        );
    }

    let describe_bytes = std::fs::read(proj.join("describe.json")).unwrap();
    let describe: serde_json::Value = serde_json::from_slice(&describe_bytes).unwrap();

    assert_eq!(
        describe
            .get("metadata")
            .and_then(|m| m.get("id"))
            .and_then(|v| v.as_str()),
        Some("greentic.snap-test"),
        "describe.json metadata.id mismatch: {describe}"
    );
    assert_eq!(
        describe
            .get("metadata")
            .and_then(|m| m.get("author"))
            .and_then(|a| a.get("name"))
            .and_then(|v| v.as_str()),
        Some("Test Author"),
        "describe.json metadata.author.name mismatch: {describe}"
    );

    let node_types = describe
        .get("contributions")
        .and_then(|c| c.get("nodeTypes"))
        .and_then(|n| n.as_array())
        .expect("contributions.nodeTypes must be an array");
    let first = node_types.first().expect("nodeTypes must have one entry");
    assert_eq!(
        first.get("type_id").and_then(|v| v.as_str()),
        Some("snap"),
        "nodeTypes[0].type_id mismatch: {first}"
    );
    assert_eq!(
        first.get("label").and_then(|v| v.as_str()),
        Some("Snap"),
        "nodeTypes[0].label mismatch: {first}"
    );
}

/// Smoke test: scaffold a wasm-component extension and confirm the generated
/// extension crate compiles to `wasm32-wasip2`. Gated with `#[ignore]` because
/// it needs the `wasm32-wasip2` rustup target and network access for cargo
/// dependency resolution. Run explicitly with:
/// `cargo test -p greentic-extension-sdk-cli -- --ignored new_wasm_component_compiles_to_wasi_p2`.
#[test]
#[ignore = "requires wasm32-wasip2 toolchain; run with `cargo test -- --ignored`"]
fn new_wasm_component_compiles_to_wasi_p2() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("greentic.compile-test");
    let (ok, stdout, stderr) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("greentic.compile-test")
        .arg("--kind")
        .arg("wasm-component")
        .arg("--dir")
        .arg(&proj)
        .arg("-y")
        .arg("--no-git"));
    assert!(ok, "gtdx new failed\nstdout:\n{stdout}\nstderr:\n{stderr}");

    let manifest = proj.join("extension/Cargo.toml");
    assert!(
        manifest.exists(),
        "extension/Cargo.toml missing after scaffold"
    );

    let (ok, build_stdout, build_stderr) = run(Command::new("cargo")
        .arg("build")
        .arg("--target")
        .arg("wasm32-wasip2")
        .arg("--manifest-path")
        .arg(&manifest));
    assert!(
        ok,
        "cargo build --target wasm32-wasip2 failed\nstdout:\n{build_stdout}\nstderr:\n{build_stderr}"
    );
}

/// The node's `runtime_ref` must name a component that carries an `oci_ref`.
///
/// This scaffold pointed it at its own design-time component, which cannot
/// execute a node at all — and the failure is silent at every layer that could
/// have caught it. The designer's flow compiler reads
/// `runtime.components.<runtime_ref>.oci_ref` and skips a `gtpack`-only
/// component (falling through to the catalog pin), and the install path
/// relocates a nested `.gtpack` into the runner's pack directory only for a
/// `ProviderExtension`. So the generated node built, installed, and ran
/// nothing.
#[test]
fn wasm_component_node_points_at_a_component_with_an_oci_ref() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("greentic.node-test");
    let (ok, stdout, stderr) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("greentic.node-test")
        .arg("--kind")
        .arg("wasm-component")
        .arg("--id")
        .arg("greentic.node-test")
        .arg("--dir")
        .arg(&proj)
        .arg("-y")
        .arg("--no-git"));
    assert!(ok, "gtdx new failed\nstdout:\n{stdout}\nstderr:\n{stderr}");

    let describe: serde_json::Value =
        serde_json::from_slice(&std::fs::read(proj.join("describe.json")).unwrap()).unwrap();

    let node = describe["contributions"]["nodeTypes"]
        .as_array()
        .and_then(|a| a.first())
        .expect("one nodeTypes entry");
    let runtime_ref = node["runtime_ref"].as_str().expect(
        "nodeTypes[0].runtime_ref must be set — absent means the runtime picks the sole \
                 declared component, which here is the design-time wasm",
    );
    let component = &describe["runtime"]["components"][runtime_ref];
    assert!(
        !component.is_null(),
        "runtime_ref {runtime_ref:?} names no component: {describe}"
    );
    assert!(
        component["oci_ref"].as_str().is_some_and(|r| !r.is_empty()),
        "nodeTypes[0].runtime_ref points at {runtime_ref:?}, which has no oci_ref. \
         The designer's flow compiler skips a gtpack-only component, so this node \
         would resolve to nothing: {component}"
    );
    assert!(
        component["gtpack"].is_null(),
        "the node component must not also be an in-pack gtpack: {component}"
    );
    assert!(
        node["operation"].as_str().is_some_and(|o| !o.is_empty()),
        "nodeTypes[0].operation must be set — the runner refuses a node without one, \
         at execution time, after the palette and the pack build both report success: {node}"
    );
}

/// No describe template may emit the deprecated `engine` block: `gtdx lint`
/// errors on its presence (`E_ENGINE_DEPRECATED`), so every scaffold shipped
/// failing the project's own linter.
#[test]
fn scaffolded_describes_carry_no_deprecated_engine_block() {
    for kind in [
        "design",
        "bundle",
        "deploy",
        "provider",
        "llm",
        "wasm-component",
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let proj = tmp.path().join("demo");
        let (ok, stdout, stderr) = run(Command::new(gtdx_bin())
            .arg("new")
            .arg("demo")
            .arg("--kind")
            .arg(kind)
            .arg("--dir")
            .arg(&proj)
            .arg("-y")
            .arg("--no-git"));
        assert!(ok, "gtdx new --kind {kind} failed\n{stdout}\n{stderr}");
        let describe: serde_json::Value =
            serde_json::from_slice(&std::fs::read(proj.join("describe.json")).unwrap()).unwrap();
        assert!(
            describe.get("engine").is_none(),
            "{kind}: describe.json still carries the deprecated `engine` block; \
             compat is the sole source of version constraints"
        );
    }
}
