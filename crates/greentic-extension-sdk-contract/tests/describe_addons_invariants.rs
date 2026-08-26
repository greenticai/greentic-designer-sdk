//! Invariants the `Addon` type cannot state, enforced at deserialize time so
//! a bad descriptor fails on load rather than as a broken form in the
//! Designer.
//!
//! The fixture shape here is copied from `describe_views_invariants.rs`
//! (known-good) rather than hand-written, varying only `contributions.addons`.

use greentic_extension_sdk_contract::describe::DescribeJson;

fn describe_with(addons: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "apiVersion": "greentic.ai/v2",
        "kind": "DesignExtension",
        "compat": {
            "min_designer_version": ">=1.2.0",
            "min_runner_version": "^1.2.0",
            "contract_version": "1.2.0"
        },
        "metadata": {
            "id": "greentic.addon-test",
            "name": "addon-test",
            "version": "0.1.0",
            "summary": "s",
            "author": { "name": "a" },
            "license": "Apache-2.0"
        },
        "capabilities": { "offered": [], "required": [] },
        "runtime": {
            "components": {
                "main": {
                    "gtpack": {
                        "file": "extension.wasm",
                        "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
                        "pack_id": "greentic.addon-test",
                        "component_version": "0.1.0"
                    },
                    "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
                    "world": "greentic:example/extension@1.0.0"
                }
            },
            "permissions": { "network": [], "secrets": [], "callExtensionKinds": [] }
        },
        "contributions": { "addons": addons }
    })
}

fn addon(id: &str, outputs: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "family": "vector-db",
        "display_name": "Test",
        "description": "Fixture addon.",
        "config_schema": "{\"type\":\"object\"}",
        "desired_state_schema": "{\"type\":\"object\"}",
        "outputs": outputs
    })
}

fn parse(v: serde_json::Value) -> Result<DescribeJson, serde_json::Error> {
    serde_json::from_value(v)
}

#[test]
fn valid_addons_accepted() {
    let d = describe_with(&serde_json::json!([
        addon(
            "qdrant",
            &serde_json::json!([{ "name": "url", "type": "text" }])
        ),
        addon(
            "redis",
            &serde_json::json!([{ "name": "url", "type": "text" }])
        ),
    ]));
    assert!(parse(d).is_ok(), "two distinct addons must parse");
}

/// Ids namespace to `<extension_id>/<addon_id>` on the platform, so a
/// duplicate would make one of the two unaddressable.
#[test]
fn duplicate_addon_id_rejected() {
    let d = describe_with(&serde_json::json!([
        addon("qdrant", &serde_json::json!([])),
        addon("qdrant", &serde_json::json!([])),
    ]));
    let err = parse(d).expect_err("duplicate id must be rejected");
    assert!(
        err.to_string().contains("qdrant"),
        "error should name the id: {err}"
    );
}

/// Outputs are addressed by name; two with the same name means a binding
/// resolves to whichever entry the platform happened to see last.
#[test]
fn duplicate_output_name_rejected() {
    let d = describe_with(&serde_json::json!([addon(
        "qdrant",
        &serde_json::json!([
            { "name": "url", "type": "text" },
            { "name": "url", "type": "text" }
        ])
    )]));
    let err = parse(d).expect_err("duplicate output name must be rejected");
    assert!(
        err.to_string().contains("url"),
        "error should name the output: {err}"
    );
}

/// The same name in two different addons is fine — they are separate
/// namespaces.
#[test]
fn the_same_output_name_in_two_addons_is_fine() {
    let d = describe_with(&serde_json::json!([
        addon(
            "qdrant",
            &serde_json::json!([{ "name": "url", "type": "text" }])
        ),
        addon(
            "redis",
            &serde_json::json!([{ "name": "url", "type": "text" }])
        ),
    ]));
    assert!(parse(d).is_ok(), "output names are scoped per addon");
}

/// A schema string that is not JSON renders as an empty form with no error,
/// which is the worst way to discover it.
#[test]
fn a_config_schema_that_is_not_json_is_rejected() {
    let mut a = addon("qdrant", &serde_json::json!([]));
    a["config_schema"] = serde_json::json!("not json at all");
    let d = describe_with(&serde_json::json!([a]));
    let err = parse(d).expect_err("unparseable config_schema must be rejected");
    assert!(
        err.to_string().contains("config_schema"),
        "error should name the field: {err}"
    );
}

/// `Addon.schema_version` is `u32` in the struct (so `0` deserializes) but
/// `describe-v2.json` declares `"minimum": 1`. Reject `0` here too, so the
/// two layers agree instead of the schema being the only enforcement point.
#[test]
fn a_schema_version_of_zero_is_rejected() {
    let mut a = addon("qdrant", &serde_json::json!([]));
    a["schema_version"] = serde_json::json!(0);
    let d = describe_with(&serde_json::json!([a]));
    let err = parse(d).expect_err("schema_version 0 must be rejected");
    assert!(
        err.to_string().contains("qdrant"),
        "error should name the addon: {err}"
    );
}

#[test]
fn a_desired_state_schema_that_is_not_json_is_rejected() {
    let mut a = addon("qdrant", &serde_json::json!([]));
    a["desired_state_schema"] = serde_json::json!("{ unclosed");
    let d = describe_with(&serde_json::json!([a]));
    let err = parse(d).expect_err("unparseable desired_state_schema must be rejected");
    assert!(
        err.to_string().contains("desired_state_schema"),
        "error should name the field: {err}"
    );
}
