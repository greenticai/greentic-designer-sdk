//! Scaffolding per extension kind.

#[allow(unused_imports)]
use std::process::Command;

#[allow(unused_imports)]
use crate::fixtures::{gtdx_bin, run};

#[test]
fn scaffolds_design_extension_and_lock_file_matches_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("demo");
    let (ok, stdout, stderr) = run(Command::new(gtdx_bin())
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
    let (ok, _o, e) = run(Command::new(gtdx_bin())
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
    let (ok, _o, e) = run(Command::new(gtdx_bin())
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
    let (ok, _o, e) = run(Command::new(gtdx_bin())
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
    // v2 templates declare a `runtime.components` map whose entries each
    // carry a `gtpack` block. The legacy `REPLACE_WITH_YOUR.gtpack`
    // sentinel from the v1 template is gone — the file is now
    // `extension.wasm` directly, matching what the packer writes.
    assert!(describe.contains("\"gtpack\""));
    assert!(describe.contains("\"file\": \"extension.wasm\""));
}

#[test]
fn scaffolds_mcp_extension_as_wasix_mcp_router() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("greentic.mcp-demo");
    let (ok, _o, e) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("greentic.mcp-demo")
        .arg("--kind")
        .arg("mcp")
        .arg("--id")
        .arg("greentic.mcp-demo")
        .arg("--dir")
        .arg(&proj)
        .arg("-y")
        .arg("--no-git"));
    assert!(ok, "gtdx new mcp failed: {e}");

    // The wasix:mcp/router skeleton ships a single-file router guest plus the
    // bundled `wasix-mcp` WIT dep so `cargo component build` resolves the
    // exported router interface.
    for rel in [
        "Cargo.toml",
        "describe.json",
        "src/lib.rs",
        "wit/world.wit",
        // The wasix:mcp router contract is bundled as a WIT dep.
        "wit/deps/wasix-mcp/package.wit",
    ] {
        assert!(proj.join(rel).exists(), "missing expected file: {rel}");
    }
    // No greentic design-extension WIT bleed-through: a router exports the
    // wasix:mcp/router interface, not greentic:extension-design/tools.
    for rel in [
        "wit/deps/greentic/extension-design/world.wit",
        "wit/deps/greentic/extension-bundle/world.wit",
    ] {
        assert!(
            !proj.join(rel).exists(),
            "unexpected greentic WIT dep present: {rel}"
        );
    }

    // The generated world declares a `mcp-router` world exporting the
    // wasix:mcp router interface — NOT a greentic design extension.
    let world = std::fs::read_to_string(proj.join("wit/world.wit")).unwrap();
    assert!(
        world.contains("world mcp-router"),
        "world.wit must declare `world mcp-router`:\n{world}"
    );
    assert!(
        world.contains("wasix:mcp/router"),
        "world.wit must export the wasix:mcp router interface:\n{world}"
    );
    assert!(
        !world.contains("greentic:extension-design/tools"),
        "world.wit must no longer export the design-extension tools interface:\n{world}"
    );

    let describe = std::fs::read_to_string(proj.join("describe.json")).unwrap();

    // describe.json parses and reports kind wasix:mcp/router.
    let describe_json: serde_json::Value =
        serde_json::from_str(&describe).expect("describe.json parses");
    assert_eq!(
        describe_json.get("kind").and_then(|v| v.as_str()),
        Some("wasix:mcp/router")
    );

    // No leftover unrendered placeholders anywhere in the generated tree.
    for rel in ["Cargo.toml", "describe.json", "src/lib.rs", "wit/world.wit"] {
        let body = std::fs::read_to_string(proj.join(rel)).unwrap();
        assert!(
            !body.contains("{{"),
            "unrendered placeholder left in {rel}:\n{body}"
        );
    }
}

#[test]
fn scaffolds_llm_extension_as_design_extension_with_tool() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("greentic.llm-demo");
    let (ok, _o, e) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("greentic.llm-demo")
        .arg("--kind")
        .arg("llm")
        .arg("--id")
        .arg("greentic.llm-demo")
        .arg("--dir")
        .arg(&proj)
        .arg("-y")
        .arg("--no-git"));
    assert!(ok, "gtdx new llm failed: {e}");

    for rel in [
        "Cargo.toml",
        "describe.json",
        "src/lib.rs",
        "prompts/system.md",
        "wit/world.wit",
        // llm reuses the design WIT package set.
        "wit/deps/greentic/extension-base/world.wit",
        "wit/deps/greentic/extension-host/world.wit",
        "wit/deps/greentic/extension-design/world.wit",
    ] {
        assert!(proj.join(rel).exists(), "missing expected file: {rel}");
    }
    // No bundle/provider WIT bleed-through.
    assert!(
        !proj
            .join("wit/deps/greentic/extension-bundle/world.wit")
            .exists()
    );

    let describe = std::fs::read_to_string(proj.join("describe.json")).unwrap();
    let describe_json: serde_json::Value =
        serde_json::from_str(&describe).expect("describe.json parses");
    assert_eq!(
        describe_json.get("kind").and_then(|v| v.as_str()),
        Some("DesignExtension")
    );
    // The starter advertises a completion tool and the REST/secret skeleton.
    assert!(describe.contains("complete"));
    assert!(describe.contains("api_key"));

    // No leftover unrendered placeholders.
    for rel in ["Cargo.toml", "describe.json", "src/lib.rs", "wit/world.wit"] {
        let body = std::fs::read_to_string(proj.join(rel)).unwrap();
        assert!(
            !body.contains("{{"),
            "unrendered placeholder left in {rel}:\n{body}"
        );
    }
}

/// Every `@version` a scaffolded `wit/world.wit` references must equal the
/// version the vendored package under `wit/deps/` actually declares.
///
/// This is the guard that was missing when the world templates rendered a
/// single `{{contract_version}}` for every package. The packages are versioned
/// independently within a generation — `extension-host` is `@0.1.0` and
/// `extension-design` is `@0.3.0` while the base generation is `0.2.0` — so
/// every scaffold asked for `greentic:extension-host@0.2.0`, a package that has
/// never existed, and failed its first `cargo component build` with
/// `package '...' not found`.
///
/// Deliberately checks the rendered world against the **vendored bytes** rather
/// than against a hardcoded table: a table would have to be updated in lockstep
/// with the WIT files and could be updated wrongly in the same edit.
/// `tests/contract_version_consistency.rs` pins the versions themselves; this
/// pins that the scaffold agrees with them.
#[test]
fn scaffolded_worlds_reference_versions_the_vendored_packages_declare() {
    // `mcp` is excluded on purpose: its world imports no greentic package.
    for kind in ["design", "bundle", "deploy", "provider", "llm"] {
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

        let world = std::fs::read_to_string(proj.join("wit/world.wit"))
            .unwrap_or_else(|e| panic!("{kind}: read wit/world.wit: {e}"));

        let mut checked = 0usize;
        for line in world.lines() {
            let Some(at) = line.find("greentic:extension-") else {
                continue;
            };
            let rest = &line[at + "greentic:".len()..];
            let pkg: String = rest
                .chars()
                .take_while(|c| c.is_ascii_lowercase() || *c == '-')
                .collect();
            let Some(ver_at) = line.find('@') else {
                panic!("{kind}: reference without a version: {line}");
            };
            let want_line = &line[ver_at + 1..];
            let referenced = want_line
                .split(|c: char| c == ';' || c.is_whitespace())
                .next()
                .unwrap_or_default();

            let dep = proj.join(format!("wit/deps/greentic/{pkg}/world.wit"));
            let dep_text = std::fs::read_to_string(&dep)
                .unwrap_or_else(|e| panic!("{kind}: {pkg} referenced but not vendored: {e}"));
            let first = dep_text.lines().next().unwrap_or_default();
            let Some(declared) = first
                .split('@')
                .nth(1)
                .map(|s| s.trim_end_matches(';').trim())
            else {
                panic!("{kind}: {pkg} declares no @version: {first}");
            };

            assert_eq!(
                referenced, declared,
                "{kind}: wit/world.wit references greentic:{pkg}@{referenced}, but the \
                 vendored package declares @{declared}. cargo component build would fail \
                 with `package 'greentic:{pkg}@{referenced}' not found`.",
            );
            checked += 1;
        }
        assert!(
            checked > 0,
            "{kind}: no greentic package references found in wit/world.wit — the test \
             would pass vacuously",
        );
    }
}

/// A design scaffold used to implement every export as an empty stub, so a
/// fresh build contributed nothing: it installed cleanly and the designer had
/// nothing to show. It now ships one working tool, and the tool has to be
/// declared in both places or it is never offered — the guest implements it,
/// `contributions.tools` lists it.
#[test]
fn design_scaffold_ships_a_working_tool_declared_in_both_places() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("demo");
    let (ok, stdout, stderr) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("demo")
        .arg("--dir")
        .arg(&proj)
        .arg("-y")
        .arg("--no-git"));
    assert!(ok, "gtdx new failed\nstdout:\n{stdout}\nstderr:\n{stderr}");

    let lib = std::fs::read_to_string(proj.join("src/lib.rs")).unwrap();
    assert!(
        lib.contains(r#"name: "echo".to_string()"#),
        "the guest must implement a real tool, not an empty Vec: {lib}"
    );
    assert!(
        !lib.contains("fn list_tools() -> Vec<tools::ToolDefinition> {\n        Vec::new()"),
        "list_tools must not be an empty stub"
    );

    let describe: serde_json::Value =
        serde_json::from_slice(&std::fs::read(proj.join("describe.json")).unwrap()).unwrap();
    let tools = describe["contributions"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 1, "describe must declare the tool: {describe}");
    assert_eq!(tools[0]["name"], "echo");
    // Governance rule E_EXPORT_FORM: fully-qualified export reference.
    assert_eq!(
        tools[0]["export"],
        "greentic:extension-design/tools.invoke-tool"
    );
    // The runtime_ref must resolve to a declared component, or lint fails with
    // a dangling reference.
    let runtime_ref = tools[0]["runtime_ref"].as_str().unwrap();
    assert!(
        describe["runtime"]["components"].get(runtime_ref).is_some(),
        "runtime_ref {runtime_ref} does not resolve: {describe}"
    );
}

/// Every kind whose guest has a "what do you offer?" export must answer it
/// with something. Returning an empty list means a freshly scaffolded
/// extension builds, packs and installs cleanly and then contributes nothing —
/// no result to see, and nothing to copy when writing the first real one.
#[test]
fn every_guest_template_ships_a_working_example() {
    // (template dir, the listing export, the token proving it returns data)
    let cases = [
        ("design", "fn list_tools(", "vec!["),
        ("bundle", "fn list_recipes(", "vec!["),
        ("deploy", "fn list_targets(", "vec!["),
        ("provider", "fn list_channels(", "vec!["),
        ("llm", "fn list_tools(", "vec!["),
        ("mcp", "fn list_tools(", "vec!["),
    ];
    for (kind, export, proof) in cases {
        let path = format!(
            "{}/templates/{kind}/src/lib.rs.tmpl",
            env!("CARGO_MANIFEST_DIR")
        );
        let tmpl = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let body = tmpl
            .split_once(export)
            .unwrap_or_else(|| panic!("{kind}: no {export} in template"))
            .1;
        // Look only at the function body, up to the next item.
        let body = body.split("\n    fn ").next().unwrap_or(body);
        assert!(
            body.contains(proof),
            "{kind}: {export} returns nothing — a fresh scaffold would contribute no {kind} surface"
        );
    }
}

/// The scaffold renderer treats doubled braces as its own placeholder syntax
/// and refuses to write a file that still contains one. A Rust format string
/// escaping braces therefore cannot appear in a template — this guards the
/// design guest, where the temptation is strongest.
#[test]
fn design_template_has_no_stray_placeholder_braces() {
    let template = include_str!("../../templates/design/src/lib.rs.tmpl");
    let known = ["{{id}}", "{{version}}"];
    let mut rest = template.to_string();
    for k in known {
        rest = rest.replace(k, "");
    }
    assert!(
        !rest.contains("{{"),
        "template contains doubled braces that are not known placeholders — it will fail to render"
    );
}

/// `ci/local_check.sh` runs `cargo test`, so a scaffold with no tests makes
/// that step green while verifying nothing — and nothing told the author host
/// tests were even possible.
///
/// Covers the four kinds whose examples were written here. `llm` and `mcp`
/// ship working examples from earlier work but no tests yet; they belong in
/// this list once they have some.
#[test]
fn guest_templates_ship_example_tests() {
    for kind in ["design", "bundle", "deploy", "provider"] {
        let path = format!(
            "{}/templates/{kind}/src/lib.rs.tmpl",
            env!("CARGO_MANIFEST_DIR")
        );
        let tmpl = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert!(
            tmpl.contains("#[cfg(test)]") && tmpl.contains("#[test]"),
            "{kind}: no example tests — `cargo test` in the quality gate would verify nothing"
        );
        // A happy-path-only example teaches the wrong habit: the error paths
        // are where a silent `Ok` hides.
        assert!(
            tmpl.contains("is_err()"),
            "{kind}: example tests never assert a failure path"
        );
    }
}

/// The scaffolded AGENTS.md has to say how to test, including the one
/// prerequisite that otherwise reads as a broken project: `cargo test` needs
/// generated bindings, so a fresh clone must build once first.
#[test]
fn agents_md_explains_testing() {
    let tmpl = include_str!("../../templates/common/AGENTS.md.tmpl");
    assert!(
        tmpl.contains("## Testing"),
        "AGENTS.md has no Testing section"
    );
    assert!(
        tmpl.contains("cannot find export in bindings"),
        "AGENTS.md does not warn that cargo test needs a build first"
    );
}
