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
/// succeeds. Gated behind `GTDX_RUN_CARGO_CHECK=1` because it needs network
/// for dep resolution (unless an offline lockfile exists).
#[test]
fn generated_project_passes_cargo_check() {
    if std::env::var("GTDX_RUN_CARGO_CHECK").ok().as_deref() != Some("1") {
        eprintln!("skip: set GTDX_RUN_CARGO_CHECK=1 to run this test");
        return;
    }
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

    let (ok, stdout, stderr) = run(Command::new("cargo")
        .arg("check")
        .arg("--quiet")
        .current_dir(&proj));
    assert!(
        ok,
        "cargo check failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
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
        // `llm` and `wasm-component` are DesignExtension-shaped and were both
        // absent from this list, which is why `llm` shipped a describe with an
        // `id` key on its tool entry — a field `Tool` does not model, so
        // `deny_unknown_fields` failed the whole describe and every `--kind llm`
        // scaffold was unusable from the moment it was generated.
        ("llm", "llm-demo"),
        ("wasm-component", "wasm-component-demo"),
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
        // `llm` was in neither this list nor the schema list above.
        ("llm", "llm-rt", ExtensionKind::Design),
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

/// `gtdx new provider-3aigent` scaffolded a project that could not be built:
/// the *id* derived from the name becomes the WIT package name, and WIT
/// requires every dash-separated word to start with a letter. The failure
/// surfaced two commands later, in `cargo component build`, as `invalid label:
/// dash-separated words must begin with an ASCII lowercase letter` — naming
/// neither the project nor the field.
///
/// The name itself is a fine cargo package name, so the message has to explain
/// that it is the derived id that fails, and point at `--id`.
#[test]
fn a_digit_led_word_in_the_name_is_rejected_up_front() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("provider-3aigent");

    let (ok, _o, e) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("provider-3aigent")
        .arg("--dir")
        .arg(&proj)
        .arg("-y")
        .arg("--no-git"));

    assert!(!ok, "expected failure, stderr:\n{e}");
    assert!(e.contains("3aigent"), "should name the bad word:\n{e}");
    assert!(
        e.contains("WIT"),
        "should say where the rule comes from:\n{e}"
    );
    assert!(
        e.contains("--id"),
        "the name is valid but the derived id is not — say how to fix it:\n{e}"
    );
    assert!(
        !proj.exists(),
        "a rejected name must not leave a half-scaffolded directory behind"
    );
}

/// The same name is fine once the author supplies an id that is a valid WIT
/// package name — the crate keeps its digit-led word, only the id may not.
#[test]
fn a_digit_led_name_scaffolds_with_an_explicit_valid_id() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("provider-3aigent");

    let (ok, _o, e) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("provider-3aigent")
        .arg("--id")
        .arg("greentic.provider-aigent3")
        .arg("--dir")
        .arg(&proj)
        .arg("-y")
        .arg("--no-git"));

    assert!(ok, "stderr:\n{e}");
    let cargo = std::fs::read_to_string(proj.join("Cargo.toml")).unwrap();
    assert!(cargo.contains("name = \"provider-3aigent\""), "{cargo}");
    assert!(
        cargo.contains("package = \"greentic:provider-aigent3\""),
        "{cargo}"
    );
}

/// Cargo rejects this one before WIT ever sees it (`invalid character `3` in
/// package name`), which is the same class of unbuildable scaffold.
#[test]
fn a_name_starting_with_a_digit_is_rejected_up_front() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("3aigent-designer");

    let (ok, _o, e) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("3aigent-designer")
        .arg("--dir")
        .arg(&proj)
        .arg("-y")
        .arg("--no-git"));

    assert!(!ok, "expected failure, stderr:\n{e}");
    assert!(e.contains("3aigent"), "stderr:\n{e}");
}

/// The rule must not start rejecting names that always worked — digits are fine
/// once a word has started.
#[test]
fn a_digit_after_the_first_letter_of_a_word_still_scaffolds() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("provider-aigent3");

    let (ok, _o, e) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("provider-aigent3")
        .arg("--dir")
        .arg(&proj)
        .arg("-y")
        .arg("--no-git"));

    assert!(ok, "stderr:\n{e}");
    assert!(proj.join("Cargo.toml").exists());
}

/// A dotted project name is folded to `-` for the cargo package name
/// (`build_context`'s `name_cargo`), because cargo rejects `.` outright:
/// `invalid character `.` in package name`. Six of the eight templates rendered
/// the raw `{{name}}` instead, so a dotted name scaffolded a project cargo
/// would not even read — and the tests never noticed, because they check the
/// file tree and never build.
#[test]
fn a_dotted_name_yields_a_cargo_package_name_without_dots() {
    for kind in crate::fixtures::all_kind_strs() {
        let tmp = tempfile::tempdir().unwrap();
        let proj = tmp.path().join("dotted");
        let (ok, o, e) = run(Command::new(gtdx_bin())
            .arg("new")
            .arg("greentic.dot-test")
            .arg("--kind")
            .arg(&kind)
            .arg("--dir")
            .arg(&proj)
            .arg("-y")
            .arg("--no-git"));
        assert!(ok, "gtdx new --kind {kind} failed\n{o}\n{e}");

        let cargo_toml = proj.join("Cargo.toml");
        if !cargo_toml.exists() {
            continue; // not every kind ships a crate
        }
        let cargo = std::fs::read_to_string(&cargo_toml).unwrap();
        let name_line = cargo
            .lines()
            .find(|l| l.trim_start().starts_with("name = "))
            .unwrap_or_else(|| panic!("{kind}: no [package] name line:\n{cargo}"));
        assert!(
            !name_line.contains('.'),
            "{kind}: cargo package name still carries a '.', which cargo refuses: {name_line}"
        );
        assert_eq!(
            name_line.trim(),
            "name = \"greentic-dot-test\"",
            "{kind}: unexpected package name"
        );
    }
}
