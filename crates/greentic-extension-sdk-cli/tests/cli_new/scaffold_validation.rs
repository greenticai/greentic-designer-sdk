//! Conflict handling + generated-project validation.

#[allow(unused_imports)]
use std::process::Command;

#[allow(unused_imports)]
use crate::fixtures::{gtdx_bin, run};

#[test]
fn target_dir_conflict_without_force_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("demo");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(proj.join("something"), "x").unwrap();

    let (ok, _o, e) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("demo")
        .arg("--dir")
        .arg(&proj)
        .arg("-y")
        .arg("--no-git"));
    assert!(!ok);
    assert!(
        e.contains("--force") || e.contains("already exists"),
        "stderr:\n{e}"
    );

    // Pre-existing file must remain untouched.
    let kept = std::fs::read_to_string(proj.join("something")).unwrap();
    assert_eq!(kept, "x");
}

#[test]
fn target_dir_conflict_with_force_succeeds() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("demo");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(proj.join("something"), "x").unwrap();

    let (ok, _o, e) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("demo")
        .arg("--dir")
        .arg(&proj)
        .arg("--force")
        .arg("-y")
        .arg("--no-git"));
    assert!(ok, "stderr:\n{e}");
    assert!(
        !proj.join("something").exists(),
        "old file should be gone after --force"
    );
    assert!(proj.join("Cargo.toml").exists());
}

/// Slow smoke test: generate a project and confirm `cargo check --quiet`
/// succeeds. Needs network for dep resolution.
///
/// `#[ignore]` rather than an env-var gate: the gate returned early and the
/// test reported **PASS**, so the suite looked green while executing nothing.
/// An ignored test is reported as ignored. Run with `cargo test -- --ignored`.
#[test]
#[ignore = "needs network for dep resolution; run with `cargo test -- --ignored`"]
fn generated_project_passes_cargo_check() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("demo");
    let (ok, _o, e) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("demo")
        .arg("--dir")
        .arg(&proj)
        .arg("-y")
        .arg("--no-git"));
    assert!(ok, "gtdx new failed: {e}");

    // `cargo component check`: the scaffold declares `mod bindings;` and only
    // cargo-component generates `src/bindings.rs` from the WIT world. Plain
    // `cargo check` cannot succeed here — a defect the env-var gate hid,
    // because a skipped gate reported PASS.
    let (ok, stdout, stderr) = run(Command::new("cargo")
        .arg("component")
        .arg("check")
        .arg("--quiet")
        .current_dir(&proj));
    assert!(
        ok,
        "cargo component check failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

#[test]
fn scaffolded_describe_json_validates_against_schema() {
    // Validate scaffolded describes against the v2 schema — the templates
    // emit v2 shape after the v1->v2 ecosystem migration (see
    // greentic-biz/greentic-designer-extensions#58 + sibling PRs).
    let schema_path = {
        let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.pop();
        p.push("greentic-extension-sdk-contract/schemas/describe-v2.json");
        p
    };
    let schema_bytes = std::fs::read(&schema_path)
        .unwrap_or_else(|e| panic!("read schema at {}: {e}", schema_path.display()));
    let schema: serde_json::Value = serde_json::from_slice(&schema_bytes).unwrap();
    let compiled = jsonschema::validator_for(&schema).expect("compile schema");

    // `mcp` is intentionally excluded: it scaffolds a `wasix:mcp/router`
    // component whose describe.json carries `kind: "wasix:mcp/router"` — that
    // is a distinct artifact, not a greentic v2 design extension, so it does
    // not (and must not) validate against the greentic `describe-v2.json`
    // schema or deserialize into the greentic `DescribeJson` contract type.
    for (kind_flag, scaffold_name) in [
        ("design", "design-demo"),
        ("bundle", "bundle-demo"),
        ("deploy", "deploy-demo"),
        ("provider", "provider-demo"),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let proj = tmp.path().join(scaffold_name);
        let (ok, stdout, stderr) = run(Command::new(gtdx_bin())
            .arg("new")
            .arg(scaffold_name)
            .arg("--kind")
            .arg(kind_flag)
            .arg("--dir")
            .arg(&proj)
            .arg("-y")
            .arg("--no-git"));
        assert!(
            ok,
            "gtdx new --kind {kind_flag} failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );

        let describe_bytes = std::fs::read(proj.join("describe.json")).unwrap();
        let describe: serde_json::Value = serde_json::from_slice(&describe_bytes).unwrap();
        if !compiled.is_valid(&describe) {
            let details: Vec<String> = compiled
                .iter_errors(&describe)
                .map(|e| format!("- {e}"))
                .collect();
            panic!(
                "describe.json for kind={kind_flag} failed schema validation:\n{}",
                details.join("\n")
            );
        }
    }
}

#[test]
fn scaffolded_describe_json_deserializes_into_v2_contract_types() {
    // Audit P0-1: the v1->v2 regression was that a scaffolded describe.json
    // would pass JSON-schema validation yet fail to deserialize into the
    // v2-only `DescribeJson` contract struct (`compat` required,
    // `deny_unknown_fields`, `runtime.components` map). Schema validation and
    // contract-type deserialization are distinct checks; this test exercises
    // the latter per kind so a future template regression to v1 shape is
    // caught here instead of silently breaking `gtdx dev` / `gtdx publish`.
    use greentic_extension_sdk_contract::describe::DescribeJson;
    use greentic_extension_sdk_contract::kind::ExtensionKind;

    // `mcp` is intentionally excluded: it scaffolds a `wasix:mcp/router`
    // component (`kind: "wasix:mcp/router"`), which is not a greentic v2
    // extension and does not deserialize into the greentic `DescribeJson`
    // contract type. See `scaffold_kinds::scaffolds_mcp_extension_as_wasix_mcp_router`.
    for (kind_flag, scaffold_name, expected_kind) in [
        ("design", "design-rt", ExtensionKind::Design),
        ("bundle", "bundle-rt", ExtensionKind::Bundle),
        ("deploy", "deploy-rt", ExtensionKind::Deploy),
        ("provider", "provider-rt", ExtensionKind::Provider),
        ("wasm-component", "greentic.wc-rt", ExtensionKind::Design),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let proj = tmp.path().join(scaffold_name);
        let (ok, stdout, stderr) = run(Command::new(gtdx_bin())
            .arg("new")
            .arg(scaffold_name)
            .arg("--kind")
            .arg(kind_flag)
            .arg("--dir")
            .arg(&proj)
            .arg("-y")
            .arg("--no-git"));
        assert!(
            ok,
            "gtdx new --kind {kind_flag} failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );

        let describe_bytes = std::fs::read(proj.join("describe.json")).unwrap();
        let describe: DescribeJson = serde_json::from_slice(&describe_bytes).unwrap_or_else(|e| {
            let raw = String::from_utf8_lossy(&describe_bytes);
            panic!(
                "scaffolded describe.json for kind={kind_flag} did not \
                 deserialize into the v2 contract type: {e}\n{raw}"
            )
        });

        assert_eq!(
            describe.api_version, "greentic.ai/v2",
            "kind={kind_flag} must scaffold apiVersion greentic.ai/v2"
        );
        assert_eq!(
            describe.kind, expected_kind,
            "kind={kind_flag} scaffolded an unexpected ExtensionKind"
        );
        assert!(
            !describe.runtime.components.is_empty(),
            "kind={kind_flag} must scaffold at least one runtime.components entry"
        );
    }
}

/// A freshly scaffolded project must pass the CLI's own gates.
///
/// It did not: five templates emitted the `engine` block their own
/// `E_ENGINE_DEPRECATED` rule forbids, and the `llm` describe carried a `tools[].id`
/// the contract does not define plus an `export` in the wrong form. `gtdx publish`
/// runs validation, so this failed later regardless — and the generated
/// `AGENTS.md` tells authors to run these commands before every commit.
///
/// Fast (no cargo build), so it runs in the default suite.
#[test]
fn every_kind_passes_validate_and_lint() {
    const KINDS: &[&str] = &[
        "design",
        "bundle",
        "deploy",
        "provider",
        "llm",
        "mcp",
        "wasm-component",
    ];
    let tmp = tempfile::tempdir().unwrap();
    let mut failures = Vec::new();

    for kind in KINDS {
        let slug = kind.replace('-', "");
        let proj = tmp.path().join(format!("v-{kind}"));
        let (ok, _o, e) = run(Command::new(gtdx_bin())
            .arg("new")
            .arg(format!("v-{kind}"))
            .arg("--kind")
            .arg(kind)
            .arg("--id")
            .arg(format!("greentic.v{slug}"))
            .arg("--dir")
            .arg(&proj)
            .arg("--no-git")
            .arg("--yes"));
        assert!(ok, "gtdx new {kind} failed: {e}");

        let (valid, _o, verr) = run(Command::new(gtdx_bin()).arg("validate").arg(&proj));
        if !valid {
            failures.push(format!("{kind}: validate — {}", verr.trim()));
        }
        let (linted, lout, lerr) =
            run(Command::new(gtdx_bin()).arg("lint").arg("--dir").arg(&proj));
        if !linted {
            let detail = if lerr.trim().is_empty() { lout } else { lerr };
            failures.push(format!("{kind}: lint — {}", detail.trim()));
        }
    }

    assert!(
        failures.is_empty(),
        "scaffolded projects fail the CLI's own gates:\n  {}",
        failures.join("\n  ")
    );
}
