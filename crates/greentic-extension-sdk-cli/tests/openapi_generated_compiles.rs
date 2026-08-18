//! Task 3 (EPIC-F v1) acceptance gate: prove the connector `gtdx openapi`
//! generates from a real spec actually compiles as Rust, not just that the
//! codegen strings look plausible.
//!
//! Pipeline exercised here mirrors what a user runs by hand:
//!   1. `gtdx openapi <spec> --out <dir> --name petstore` (the CLI binary,
//!      not the library — this crate has no `[lib]` target, see Cargo.toml).
//!   2. `cargo component bindings` in the generated crate — materializes
//!      `src/bindings.rs` from `wit/world.wit` (this crate's `Cargo.toml`
//!      declares `[package.metadata.component]`, so plain `cargo` alone
//!      never emits `bindings.rs`; only `cargo component <cmd>` does).
//!   3. `cargo check` (host target) on the generated crate. Host-checkable
//!      because the generated `dispatch.rs`/`lib.rs` mirror
//!      `component-http-ext`: the `bindings::export!(...)` call — the one
//!      symbol that is genuinely wasm-only — is behind
//!      `#[cfg(target_family = "wasm")]`, so the pure `tool_meta`/`dispatch`
//!      logic plus the wit-bindgen-generated import stubs host-compile.
//!
//! Gated behind `#[ignore]` (like `cli_new::wasm_component::
//! new_wasm_component_compiles_to_wasi_p2`): it needs the `cargo-component`
//! subcommand installed and network access to resolve crates.io deps for a
//! brand-new crate. Run explicitly with:
//! `cargo test -p greentic-extension-sdk-cli --test openapi_generated_compiles -- --ignored`.

use std::path::PathBuf;
use std::process::Command;

fn gtdx_bin() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("target/debug/gtdx");
    p
}

fn fixture_spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/commands/openapi/fixtures/petstore-min.json")
}

fn run(cmd: &mut Command) -> (bool, String, String) {
    let out = cmd.output().expect("spawn command");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

#[test]
#[ignore = "requires `cargo component` + network to resolve a fresh crate's deps; run with `cargo test -- --ignored`"]
fn generated_openapi_connector_compiles() {
    let tmp = tempfile::tempdir().unwrap();
    let out_dir = tmp.path().join("petstore");

    let (ok, stdout, stderr) = run(Command::new(gtdx_bin())
        .arg("openapi")
        .arg(fixture_spec_path())
        .arg("--out")
        .arg(&out_dir)
        .arg("--name")
        .arg("petstore"));
    assert!(
        ok,
        "gtdx openapi failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        out_dir.join("Cargo.toml").is_file(),
        "generated connector missing Cargo.toml"
    );

    // `cargo component bindings`: materializes `src/bindings.rs` from
    // `wit/world.wit` — plain `cargo check`/`cargo build` never generates it
    // (that's cargo-component's job), so this step is required before any
    // host or wasm compile of the generated crate can succeed.
    let (ok, stdout, stderr) = run(Command::new("cargo")
        .arg("component")
        .arg("bindings")
        .current_dir(&out_dir));
    assert!(
        ok,
        "cargo component bindings failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        out_dir.join("src/bindings.rs").is_file(),
        "cargo component bindings did not write src/bindings.rs"
    );

    // Host `cargo check`: proves the generated `tool_meta.rs`/`dispatch.rs`/
    // `lib.rs` are valid Rust that type-checks against the real
    // wit-bindgen-generated bindings, without needing the wasm32-wasip2
    // target or a full release build.
    let (ok, stdout, stderr) = run(Command::new("cargo")
        .arg("check")
        .env("CARGO_BUILD_JOBS", "2")
        .current_dir(&out_dir));
    assert!(
        ok,
        "cargo check (host) failed on the generated connector\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
