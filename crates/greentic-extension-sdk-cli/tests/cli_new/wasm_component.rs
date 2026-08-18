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
        "extension/Cargo.toml",
        "extension/src/lib.rs",
        "extension/wit/world.wit",
        "runtime/README.md",
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

    // `cargo component build`, not plain `cargo build`: the scaffold declares
    // `mod bindings;` and cargo-component is what generates `src/bindings.rs`
    // from the WIT world. It is also what `gtdx dev` shells out to, so this is
    // the build path a user actually takes. Plain `cargo build` cannot succeed
    // here and testing it proved nothing.
    let (ok, build_stdout, build_stderr) = run(Command::new("cargo")
        .arg("component")
        .arg("build")
        .arg("--target")
        .arg("wasm32-wasip2")
        .arg("--manifest-path")
        .arg(&manifest));
    assert!(
        ok,
        "cargo component build --target wasm32-wasip2 failed\nstdout:\n{build_stdout}\nstderr:\n{build_stderr}"
    );
}
