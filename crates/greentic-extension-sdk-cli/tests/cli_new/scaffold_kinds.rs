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
