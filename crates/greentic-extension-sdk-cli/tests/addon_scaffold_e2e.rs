//! End-to-end coverage for `gtdx new --kind addon`.
//!
//! `addon_e2e.rs` drives `contributions.addons` through `gtdx validate`/`gtdx
//! lint` on a hand-patched fixture — it proves the schema and lint rules are
//! reachable, but it says nothing about whether `gtdx new --kind addon`
//! itself produces something that passes them. That gap is this file: the
//! scaffold template shipped in `templates/addon/` is what makes `--kind
//! addon` a real, consumable command rather than groundwork — everything
//! before it (the WIT contract, `ExtensionKind::Addon`, the WIT-only
//! plumbing in `scaffold::embedded`) only got the pieces in place.
//!
//! Follows the fixture/binary pattern `addon_e2e.rs` already established:
//! `CARGO_BIN_EXE_gtdx` for the binary under test, `tempfile::TempDir` for
//! the workspace, and a `--home` pointed at an empty tmp dir so `lint`'s
//! `E_DESCRIBE_DIFF_BREAKING` check (which reads `<home>/extensions/...`)
//! never touches the real `~/.greentic`.

use std::process::Command;

use tempfile::TempDir;

fn gtdx_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_gtdx"))
}

fn empty_home() -> TempDir {
    TempDir::new().expect("tmp home must create")
}

/// `gtdx new --kind addon` scaffolds a project that passes `gtdx validate`
/// and `gtdx lint` untouched, and its `describe.json` declares the
/// `AddonExtension` kind plus a `contributions.addons[]` entry — the two
/// things that make it recognisable as an addon at all, not just an
/// arbitrary extension that happens to share the name.
#[test]
fn a_freshly_scaffolded_addon_passes_validate_and_lint() {
    let workspace = TempDir::new().expect("workspace tmp dir must create");
    let proj = workspace.path().join("example-cache");
    let home = empty_home();

    let new_output = Command::new(gtdx_bin())
        .arg("new")
        .arg("example-cache")
        .arg("--kind")
        .arg("addon")
        .arg("--dir")
        .arg(&proj)
        .arg("-y")
        .arg("--no-git")
        .output()
        .expect("gtdx new must run");
    assert!(
        new_output.status.success(),
        "gtdx new --kind addon failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&new_output.stdout),
        String::from_utf8_lossy(&new_output.stderr),
    );

    let describe_path = proj.join("describe.json");
    assert!(
        describe_path.exists(),
        "gtdx new --kind addon did not write describe.json"
    );
    let describe: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&describe_path).expect("describe.json readable"))
            .expect("describe.json must be valid JSON");
    assert_eq!(
        describe["kind"], "AddonExtension",
        "scaffolded describe.json must declare kind: AddonExtension: {describe}"
    );
    let addons = describe["contributions"]["addons"]
        .as_array()
        .expect("contributions.addons must be an array");
    assert!(
        !addons.is_empty(),
        "scaffolded describe.json must declare at least one contributions.addons[] entry: {describe}"
    );

    let validate_output = Command::new(gtdx_bin())
        .arg("--home")
        .arg(home.path())
        .arg("validate")
        .arg(&proj)
        .output()
        .expect("gtdx validate must run");
    assert!(
        validate_output.status.success(),
        "gtdx validate failed on a freshly scaffolded addon\nstderr:\n{}",
        String::from_utf8_lossy(&validate_output.stderr),
    );

    let lint_output = Command::new(gtdx_bin())
        .arg("--home")
        .arg(home.path())
        .arg("lint")
        .arg("--dir")
        .arg(&proj)
        .output()
        .expect("gtdx lint must run");
    assert!(
        lint_output.status.success(),
        "gtdx lint failed on a freshly scaffolded addon\nstderr:\n{}",
        String::from_utf8_lossy(&lint_output.stderr),
    );
}

/// The addon world selects `addon-extension` — the world WITHOUT `backup` —
/// and `describe.json` declares `supports_backup: false` to match. Claiming
/// `addon-extension-with-backup` (or `supports_backup: true`) without
/// implementing `backup` would be a capability this scaffold does not have.
#[test]
fn the_scaffolded_world_selects_addon_extension_without_backup() {
    let workspace = TempDir::new().expect("workspace tmp dir must create");
    let proj = workspace.path().join("example-cache");

    let new_output = Command::new(gtdx_bin())
        .arg("new")
        .arg("example-cache")
        .arg("--kind")
        .arg("addon")
        .arg("--dir")
        .arg(&proj)
        .arg("-y")
        .arg("--no-git")
        .output()
        .expect("gtdx new must run");
    assert!(
        new_output.status.success(),
        "gtdx new --kind addon failed: {}",
        String::from_utf8_lossy(&new_output.stderr)
    );

    let world =
        std::fs::read_to_string(proj.join("wit/world.wit")).expect("wit/world.wit readable");
    assert!(
        world.contains("greentic:extension-addon/validation"),
        "world.wit must export validation:\n{world}"
    );
    assert!(
        world.contains("greentic:extension-addon/workload"),
        "world.wit must export workload:\n{world}"
    );
    assert!(
        world.contains("greentic:extension-addon/reconciler"),
        "world.wit must export reconciler:\n{world}"
    );
    assert!(
        !world.contains("greentic:extension-addon/backup"),
        "world.wit must NOT export backup — this scaffold cannot snapshot:\n{world}"
    );

    let describe =
        std::fs::read_to_string(proj.join("describe.json")).expect("describe.json readable");
    let describe_json: serde_json::Value =
        serde_json::from_str(&describe).expect("describe.json parses");
    assert_eq!(
        describe_json["contributions"]["addons"][0]["supports_backup"], false,
        "supports_backup must be false unless backup is actually implemented: {describe}"
    );

    for rel in ["Cargo.toml", "describe.json", "src/lib.rs", "wit/world.wit"] {
        let body = std::fs::read_to_string(proj.join(rel)).unwrap();
        assert!(
            !body.contains("{{"),
            "unrendered placeholder left in {rel}:\n{body}"
        );
    }
}

/// `cargo component build` is too slow and too network-dependent for the
/// default suite, but the claim that the scaffold actually compiles to a
/// wasm component has to be checked by *something* — this test is that
/// record. Run by hand: `cargo test --test addon_scaffold_e2e -- --ignored
/// addon_scaffold_builds_a_real_wasm_component`.
#[test]
#[ignore = "cargo component build is slow and needs network access to crates.io; run by hand"]
fn addon_scaffold_builds_a_real_wasm_component() {
    let workspace = TempDir::new().expect("workspace tmp dir must create");
    let proj = workspace.path().join("example-cache");

    let new_output = Command::new(gtdx_bin())
        .arg("new")
        .arg("example-cache")
        .arg("--kind")
        .arg("addon")
        .arg("--dir")
        .arg(&proj)
        .arg("-y")
        .arg("--no-git")
        .output()
        .expect("gtdx new must run");
    assert!(
        new_output.status.success(),
        "gtdx new --kind addon failed: {}",
        String::from_utf8_lossy(&new_output.stderr)
    );

    let build_output = Command::new("cargo")
        .arg("component")
        .arg("build")
        .current_dir(&proj)
        .output()
        .expect("cargo component build must run (requires cargo-component on PATH)");
    assert!(
        build_output.status.success(),
        "cargo component build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build_output.stdout),
        String::from_utf8_lossy(&build_output.stderr),
    );

    // Named by the crate name with `-` mapped to `_` by cargo, which this
    // test deliberately does not hardcode — the point is that *some* real
    // `.wasm` came out of `target/wasm32-wasip1/debug/`, not that this
    // scaffold's particular project name maps to a particular file name.
    let debug_dir = proj.join("target/wasm32-wasip1/debug");
    let wasm = std::fs::read_dir(&debug_dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", debug_dir.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|p| p.extension().is_some_and(|ext| ext == "wasm"))
        .unwrap_or_else(|| {
            panic!(
                "cargo component build reported success but no .wasm was produced in {}",
                debug_dir.display()
            )
        });
    let size = std::fs::metadata(&wasm)
        .expect("wasm metadata readable")
        .len();
    assert!(size > 0, "produced .wasm ({}) is empty", wasm.display());
}
