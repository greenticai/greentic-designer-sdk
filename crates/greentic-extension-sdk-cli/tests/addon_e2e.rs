//! End-to-end coverage for `contributions.addons` through the real `gtdx`
//! binary.
//!
//! `contributions.addons` shipped in 1.2.11 with unit tests at every layer —
//! the struct (`describe_addons_invariants.rs`), the JSON Schema
//! (`schema_v2_addons.rs`), and the four lint rules
//! (`commands/lint/tests.rs`). Nothing has ever driven the real `gtdx`
//! binary against a real extension that declares an addon. Each layer is
//! tested; the path through them — `gtdx validate` reaching the schema and
//! the deserializer, `gtdx lint` reaching the rules — is not. That is what
//! this file pins.
//!
//! Follows the fixture/assertion pattern in `cli_e2e.rs`: `CARGO_BIN_EXE_gtdx`
//! for the binary under test, `ExtensionFixtureBuilder` + `tempfile::TempDir`
//! for the workspace. `ExtensionFixtureBuilder` has no addon-specific
//! support, so each test builds an ordinary fixture and then patches
//! `contributions.addons` into the written `describe.json` directly — safe
//! because `manifest_sha256` binds to `manifest.json`'s bytes, not
//! `describe.json`'s, and neither `validate` nor `lint` checks the manifest
//! binding.

use std::process::Command;

use greentic_extension_sdk_contract::ExtensionKind;
use greentic_extension_sdk_testing::ExtensionFixtureBuilder;
use tempfile::TempDir;

fn gtdx_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_gtdx"))
}

/// A well-formed addon: id, family, `display_name`, description, both
/// schemas, one plain output and one `sensitive` output.
fn well_formed_addon() -> serde_json::Value {
    serde_json::json!({
        "id": "qdrant",
        "family": "vector-db",
        "display_name": "Qdrant",
        "description": "Vector database.",
        "config_schema": "{\"type\":\"object\"}",
        "desired_state_schema": "{\"type\":\"object\",\"properties\":{\"collections\":{\"type\":\"array\"}}}",
        "outputs": [
            { "name": "QDRANT_URL", "type": "text" },
            { "name": "QDRANT_API_KEY", "type": "text", "sensitive": true }
        ]
    })
}

/// Builds a fresh extension fixture, then overwrites its `describe.json`'s
/// `contributions.addons` with `addons`. Returns the fixture (kept alive so
/// its `TempDir` isn't dropped) alongside the directory the describe lives
/// in.
///
/// Also sets the canonical `$schema` and drops the (unrelated, deprecated)
/// `engine` block `ExtensionFixtureBuilder` fills in by default: `gtdx lint`
/// flags both (`E_SCHEMA_HOST`, `E_ENGINE_DEPRECATED`), and case 2 needs a
/// fixture that is clean under `lint` overall, not merely clean of the four
/// addon codes, so those two unrelated defaults would otherwise fail the
/// "no violations" extension for reasons that have nothing to do with
/// addons.
fn fixture_with_addons(
    addons: serde_json::Value,
) -> greentic_extension_sdk_testing::ExtensionFixture {
    let fixture =
        ExtensionFixtureBuilder::new(ExtensionKind::Design, "greentic.addon-test", "0.1.0")
            .offer("greentic:addon-test/y", "1.0.0")
            .with_wasm(b"wasm".to_vec())
            .build()
            .expect("fixture must build");

    let bytes = std::fs::read(&fixture.describe_path).expect("describe.json must be readable");
    let mut describe: serde_json::Value =
        serde_json::from_slice(&bytes).expect("describe.json must be valid JSON");
    describe["contributions"]["addons"] = addons;
    describe["$schema"] =
        serde_json::json!("https://store.greentic.cloud/schemas/describe-v2.json");
    if let Some(obj) = describe.as_object_mut() {
        obj.remove("engine");
    }
    std::fs::write(
        &fixture.describe_path,
        serde_json::to_vec_pretty(&describe).expect("describe.json must re-serialize"),
    )
    .expect("describe.json must be writable");

    fixture
}

/// A `--home` pointed at an empty tmp dir, so `lint`'s
/// `E_DESCRIBE_DIFF_BREAKING` check (which reads `<home>/extensions/...`)
/// finds nothing installed and neither command touches the real
/// `~/.greentic`.
fn empty_home() -> TempDir {
    TempDir::new().expect("tmp home must create")
}

/// Case 1: a well-formed addon passes `gtdx validate`.
#[test]
fn a_well_formed_addon_passes_validate() {
    let fixture = fixture_with_addons(serde_json::json!([well_formed_addon()]));
    let home = empty_home();

    let output = Command::new(gtdx_bin())
        .arg("--home")
        .arg(home.path())
        .arg("validate")
        .arg(fixture.root())
        .output()
        .expect("gtdx validate must run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Case 2: the same extension passes `gtdx lint` clean — exit success, and
/// none of the four addon codes named on stderr.
#[test]
fn a_well_formed_addon_passes_lint_with_no_addon_violations() {
    let fixture = fixture_with_addons(serde_json::json!([well_formed_addon()]));
    let home = empty_home();

    let output = Command::new(gtdx_bin())
        .arg("--home")
        .arg(home.path())
        .arg("lint")
        .arg("--dir")
        .arg(fixture.root())
        .output()
        .expect("gtdx lint must run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    for code in [
        "E_ADDON_ID_PATTERN",
        "E_ADDON_OUTPUT_NAME",
        "E_ADDON_SECRET_IN_DESIRED_STATE",
        "W_ADDON_FAMILY_UNKNOWN",
    ] {
        assert!(
            !stderr.contains(code),
            "a well-formed addon must not trip {code}: {stderr}"
        );
    }
}

/// Case 3: a secret at the top level of `desired_state_schema` fails
/// `gtdx lint` with `E_ADDON_SECRET_IN_DESIRED_STATE`. This is the rule
/// that turns spec D16 into something that fails at the gate — if it does
/// not fire through the real binary, it does not fire.
#[test]
fn a_top_level_secret_in_desired_state_fails_lint() {
    let mut addon = well_formed_addon();
    addon["desired_state_schema"] = serde_json::json!(
        "{\"type\":\"object\",\"properties\":{\"collections\":{\"type\":\"array\"},\"admin_password\":{\"type\":\"string\"}}}"
    );
    let fixture = fixture_with_addons(serde_json::json!([addon]));
    let home = empty_home();

    let output = Command::new(gtdx_bin())
        .arg("--home")
        .arg(home.path())
        .arg("lint")
        .arg("--dir")
        .arg(fixture.root())
        .output()
        .expect("gtdx lint must run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "a top-level secret in desired_state_schema must fail lint"
    );
    assert!(
        stderr.contains("E_ADDON_SECRET_IN_DESIRED_STATE"),
        "stderr must name the rule: {stderr}"
    );
}

/// Case 4: a secret nested inside `items` also fails. The rule was
/// originally blind to this shape — the exact shape the design decision
/// (Redis ACL users, `rediscloud_acl_user`) cites — so this pins it end to
/// end, not just in the unit test `a_secret_nested_under_items_properties_
/// is_caught_with_a_path` in `commands/lint/tests.rs`.
#[test]
fn a_secret_nested_inside_items_fails_lint() {
    let mut addon = well_formed_addon();
    addon["desired_state_schema"] = serde_json::json!(
        "{\"type\":\"object\",\"properties\":{\"acl_users\":{\"type\":\"array\",\"items\":{\"type\":\"object\",\"properties\":{\"password\":{}}}}}}"
    );
    let fixture = fixture_with_addons(serde_json::json!([addon]));
    let home = empty_home();

    let output = Command::new(gtdx_bin())
        .arg("--home")
        .arg(home.path())
        .arg("lint")
        .arg("--dir")
        .arg(fixture.root())
        .output()
        .expect("gtdx lint must run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "a secret nested inside items must fail lint"
    );
    assert!(
        stderr.contains("E_ADDON_SECRET_IN_DESIRED_STATE"),
        "stderr must name the rule: {stderr}"
    );
}

/// Case 5: an id the JSON Schema rejects fails `gtdx validate`. The pattern
/// (`^[a-z0-9][a-z0-9-]*$`) lives in `describe-v2.json`, and the schema is
/// the layer that gates publish and install, so this proves the guard is
/// reachable through the real binary and not only through `check_addons`'s
/// own unit-tested copy of the same pattern.
#[test]
fn an_id_the_schema_rejects_fails_validate() {
    let mut addon = well_formed_addon();
    addon["id"] = serde_json::json!("Qdrant/Primary");
    let fixture = fixture_with_addons(serde_json::json!([addon]));
    let home = empty_home();

    let output = Command::new(gtdx_bin())
        .arg("--home")
        .arg(home.path())
        .arg("validate")
        .arg(fixture.root())
        .output()
        .expect("gtdx validate must run");

    assert!(
        !output.status.success(),
        "an id containing '/' and uppercase must fail validate"
    );
}

/// Case 6: two addons sharing an id fail `gtdx validate`. Enforced at
/// deserialize (`validate_addons` in `describe/mod.rs`), not by the schema
/// or by lint, so this proves that third layer is reachable through the
/// real binary too.
#[test]
fn a_duplicate_addon_id_fails_validate() {
    let mut second = well_formed_addon();
    second["display_name"] = serde_json::json!("Qdrant (again)");
    let fixture = fixture_with_addons(serde_json::json!([well_formed_addon(), second]));
    let home = empty_home();

    let output = Command::new(gtdx_bin())
        .arg("--home")
        .arg(home.path())
        .arg("validate")
        .arg(fixture.root())
        .output()
        .expect("gtdx validate must run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "a duplicate addon id must fail validate"
    );
    assert!(
        stderr.contains("qdrant"),
        "error should name the offending id: {stderr}"
    );
}
