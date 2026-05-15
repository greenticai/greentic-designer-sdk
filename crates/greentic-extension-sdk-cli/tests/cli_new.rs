use std::process::Command;

fn gtdx_bin() -> std::path::PathBuf {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("target/debug/gtdx");
    p
}

fn gtdx_cmd() -> std::process::Command {
    let bin = gtdx_bin();
    // Integration tests execute the locally built gtdx binary from Cargo output.
    // foxguard: ignore[rs/no-command-injection]
    std::process::Command::new(bin)
}

fn run(cmd: &mut Command) -> (bool, String, String) {
    let out = cmd.output().expect("spawn gtdx");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

#[test]
fn scaffolds_design_extension_and_lock_file_matches_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("demo");
    let (ok, stdout, stderr) = run(gtdx_cmd()
        .arg("new")
        .arg("demo")
        .arg("--dir")
        .arg(&proj)
        .arg("-y")
        .arg("--no-git"));
    assert!(ok, "gtdx new failed\nstdout:\n{stdout}\nstderr:\n{stderr}");

    for rel in [
        "Cargo.toml",
        "describe.json",
        "src/lib.rs",
        ".gtdx-contract.lock",
        "wit/deps/greentic/extension-base/world.wit",
        "wit/deps/greentic/extension-host/world.wit",
        "wit/deps/greentic/extension-design/world.wit",
    ] {
        assert!(
            proj.join(rel).exists(),
            "missing expected file: {rel}\nstdout:\n{stdout}"
        );
    }

    let lock = std::fs::read_to_string(proj.join(".gtdx-contract.lock")).unwrap();
    assert!(lock.contains("contract_version"));
    assert!(lock.contains("wit/deps/greentic/extension-base/world.wit"));

    // Verify hash in lock matches actual bytes on disk.
    let base_bytes =
        std::fs::read(proj.join("wit/deps/greentic/extension-base/world.wit")).unwrap();
    let expected_sha = {
        use sha2::{Digest, Sha256};
        let d = Sha256::digest(&base_bytes);
        let mut s = String::new();
        for b in d {
            use std::fmt::Write as _;
            write!(&mut s, "{b:02x}").unwrap();
        }
        s
    };
    assert!(
        lock.contains(&format!("sha256:{expected_sha}")),
        "lock file hash did not match on-disk WIT bytes"
    );
}

#[test]
fn scaffolds_bundle_extension_with_correct_wit_deps() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("b");
    let (ok, _o, e) = run(gtdx_cmd()
        .arg("new")
        .arg("b")
        .arg("--kind")
        .arg("bundle")
        .arg("--dir")
        .arg(&proj)
        .arg("-y")
        .arg("--no-git"));
    assert!(ok, "gtdx new bundle failed: {e}");
    assert!(
        proj.join("wit/deps/greentic/extension-bundle/world.wit")
            .exists()
    );
    assert!(
        !proj
            .join("wit/deps/greentic/extension-design/world.wit")
            .exists()
    );
    let describe = std::fs::read_to_string(proj.join("describe.json")).unwrap();
    assert!(describe.contains("\"kind\": \"BundleExtension\""));
}

#[test]
fn scaffolds_deploy_extension_with_correct_wit_deps() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("d");
    let (ok, _o, e) = run(gtdx_cmd()
        .arg("new")
        .arg("d")
        .arg("--kind")
        .arg("deploy")
        .arg("--dir")
        .arg(&proj)
        .arg("-y")
        .arg("--no-git"));
    assert!(ok, "gtdx new deploy failed: {e}");
    assert!(
        proj.join("wit/deps/greentic/extension-deploy/world.wit")
            .exists()
    );
    assert!(
        !proj
            .join("wit/deps/greentic/extension-bundle/world.wit")
            .exists()
    );
    let describe = std::fs::read_to_string(proj.join("describe.json")).unwrap();
    assert!(describe.contains("\"kind\": \"DeployExtension\""));
}

#[test]
fn scaffolds_provider_extension_with_correct_wit_deps() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("p");
    let (ok, _o, e) = run(gtdx_cmd()
        .arg("new")
        .arg("p")
        .arg("--kind")
        .arg("provider")
        .arg("--dir")
        .arg(&proj)
        .arg("-y")
        .arg("--no-git"));
    assert!(ok, "gtdx new provider failed: {e}");
    assert!(
        proj.join("wit/deps/greentic/extension-provider/world.wit")
            .exists()
    );
    assert!(
        !proj
            .join("wit/deps/greentic/extension-design/world.wit")
            .exists()
    );
    assert!(
        !proj
            .join("wit/deps/greentic/extension-bundle/world.wit")
            .exists()
    );
    assert!(
        !proj
            .join("wit/deps/greentic/extension-deploy/world.wit")
            .exists()
    );

    let describe = std::fs::read_to_string(proj.join("describe.json")).unwrap();
    assert!(describe.contains("\"kind\": \"ProviderExtension\""));
    assert!(describe.contains("\"gtpack\""));
    assert!(describe.contains("REPLACE_WITH_YOUR.gtpack"));
}

#[test]
fn target_dir_conflict_without_force_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("demo");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(proj.join("something"), "x").unwrap();

    let (ok, _o, e) = run(gtdx_cmd()
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

    let (ok, _o, e) = run(gtdx_cmd()
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
    let (ok, _o, e) = run(gtdx_cmd()
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
    let schema_path = {
        let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.pop();
        p.push("greentic-extension-sdk-contract/schemas/describe-v1.json");
        p
    };
    let schema_bytes = std::fs::read(&schema_path)
        .unwrap_or_else(|e| panic!("read schema at {}: {e}", schema_path.display()));
    let schema: serde_json::Value = serde_json::from_slice(&schema_bytes).unwrap();
    let compiled = jsonschema::validator_for(&schema).expect("compile schema");

    for (kind_flag, scaffold_name) in [
        ("design", "design-demo"),
        ("design-artifact-producer", "artifact-producer-demo"),
        ("bundle", "bundle-demo"),
        ("deploy", "deploy-demo"),
        ("provider", "provider-demo"),
        ("wasm-component", "wasm-component-demo"),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let proj = tmp.path().join(scaffold_name);
        let (ok, stdout, stderr) = run(gtdx_cmd()
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
fn scaffolds_design_artifact_producer_fixture() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("artifact-demo");
    let (ok, stdout, stderr) = run(gtdx_cmd()
        .arg("new")
        .arg("artifact-demo")
        .arg("--kind")
        .arg("design-artifact-producer")
        .arg("--dir")
        .arg(&proj)
        .arg("-y")
        .arg("--no-git"));
    assert!(ok, "gtdx new failed\nstdout:\n{stdout}\nstderr:\n{stderr}");

    for rel in [
        "Cargo.toml",
        "README.md",
        "describe.json",
        "src/lib.rs",
        "schemas/generate-artifact.input.schema.json",
        "schemas/artifact-output.schema.json",
        "examples/artifact-output.json",
        "wit/deps/greentic/extension-design/world.wit",
    ] {
        assert!(
            proj.join(rel).exists(),
            "missing expected file: {rel}\nstdout:\n{stdout}"
        );
    }

    let describe_bytes = std::fs::read(proj.join("describe.json")).unwrap();
    let describe: serde_json::Value = serde_json::from_slice(&describe_bytes).unwrap();
    assert_eq!(describe["kind"], "DesignExtension");
    assert!(
        describe
            .get("runtime")
            .and_then(|runtime| runtime.get("gtpack"))
            .is_none(),
        "tool-generated artifact fixture must not use descriptor runtime.gtpack: {describe}"
    );

    for rel in [
        "schemas/generate-artifact.input.schema.json",
        "schemas/artifact-output.schema.json",
    ] {
        let schema_bytes = std::fs::read(proj.join(rel)).unwrap();
        let schema: serde_json::Value = serde_json::from_slice(&schema_bytes).unwrap();
        jsonschema::validator_for(&schema)
            .unwrap_or_else(|e| panic!("{rel} is not a valid JSON schema: {e}"));
    }

    let output = std::fs::read_to_string(proj.join("examples/artifact-output.json")).unwrap();
    greentic_extension_sdk_testing::assert_valid_artifact_output_json(&output);
    let output_json: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(
        output_json["preview_json"]["summary"], "Generated by fixture",
        "preview JSON should be preserved"
    );

    let rendered_tree = [
        std::fs::read_to_string(proj.join("README.md")).unwrap(),
        std::fs::read_to_string(proj.join("describe.json")).unwrap(),
        std::fs::read_to_string(proj.join("src/lib.rs")).unwrap(),
    ]
    .join("\n");
    assert!(
        !rendered_tree.to_lowercase().contains("sorla"),
        "fixture should stay generic"
    );
}

#[test]
fn new_wasm_component_accepts_node_type_id_and_label() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("greentic.test-tool");
    let (ok, stdout, stderr) = run(gtdx_cmd()
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
    let (ok, stdout, stderr) = run(gtdx_cmd()
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

#[test]
fn new_wasm_component_validates_and_doctor_reports_node_types() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("greentic.node-provider");
    let (ok, stdout, stderr) = run(gtdx_cmd()
        .arg("new")
        .arg("greentic.node-provider")
        .arg("--kind")
        .arg("wasm-component")
        .arg("--node-type-id")
        .arg("node-provider")
        .arg("--label")
        .arg("Node Provider")
        .arg("--dir")
        .arg(&proj)
        .arg("-y")
        .arg("--no-git"));
    assert!(ok, "gtdx new failed\nstdout:\n{stdout}\nstderr:\n{stderr}");

    let (ok, validate_stdout, validate_stderr) = run(gtdx_cmd().arg("validate").arg(&proj));
    assert!(
        ok,
        "gtdx validate failed\nstdout:\n{validate_stdout}\nstderr:\n{validate_stderr}"
    );
    assert!(validate_stdout.contains("nodeTypes: 1 valid"));

    let (ok, doctor_stdout, doctor_stderr) = run(gtdx_cmd().arg("doctor").arg(&proj));
    assert!(
        ok,
        "gtdx doctor failed\nstdout:\n{doctor_stdout}\nstderr:\n{doctor_stderr}"
    );
    assert!(doctor_stdout.contains("Node types:"));
    assert!(doctor_stdout.contains("count: 1"));
    assert!(doctor_stdout.contains("valid: true"));
}

#[test]
fn doctor_rejects_invalid_node_types() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("greentic.invalid-node-provider");
    let (ok, stdout, stderr) = run(gtdx_cmd()
        .arg("new")
        .arg("greentic.invalid-node-provider")
        .arg("--kind")
        .arg("wasm-component")
        .arg("--dir")
        .arg(&proj)
        .arg("-y")
        .arg("--no-git"));
    assert!(ok, "gtdx new failed\nstdout:\n{stdout}\nstderr:\n{stderr}");

    let describe_path = proj.join("describe.json");
    let bytes = std::fs::read(&describe_path).unwrap();
    let mut describe: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    describe["contributions"]["nodeTypes"][0]["config_schema"] = serde_json::json!("{");
    std::fs::write(
        &describe_path,
        serde_json::to_vec_pretty(&describe).unwrap(),
    )
    .unwrap();

    let (ok, doctor_stdout, doctor_stderr) = run(gtdx_cmd().arg("doctor").arg(&proj));
    assert!(
        !ok,
        "gtdx doctor should fail\nstdout:\n{doctor_stdout}\nstderr:\n{doctor_stderr}"
    );
    assert!(
        doctor_stderr.contains("node type contribution") || doctor_stderr.contains("config_schema"),
        "stderr should explain invalid node type, got: {doctor_stderr}"
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
    let (ok, stdout, stderr) = run(gtdx_cmd()
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
