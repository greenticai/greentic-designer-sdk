//! The top-level `configSchema` — the extension-wide, non-secret operator
//! configuration the admin console renders as a form.
//!
//! The fixture shape is copied from `describe_addons_invariants.rs`
//! (known-good), varying only `configSchema`.
//!
//! Three things are pinned here, in the order they bite:
//! 1. a describe *without* the field still parses and still round-trips
//!    without growing a `configSchema` key (the compatibility direction that
//!    matters for every already-published extension);
//! 2. a describe *with* it round-trips, value intact;
//! 3. a malformed value is rejected at deserialize time, rather than
//!    reaching the console and rendering as an empty form with no error.

use greentic_extension_sdk_contract::describe::DescribeJson;
use greentic_extension_sdk_contract::schema::validate_describe_json;

fn base() -> serde_json::Value {
    serde_json::json!({
        "apiVersion": "greentic.ai/v2",
        "kind": "DesignExtension",
        "compat": {
            "min_designer_version": ">=1.2.0",
            "min_runner_version": "^1.2.0",
            "contract_version": "1.2.0"
        },
        "metadata": {
            "id": "greentic.config-test",
            "name": "config-test",
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
                        "pack_id": "greentic.config-test",
                        "component_version": "0.1.0"
                    },
                    "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
                    "world": "greentic:example/extension@1.0.0"
                }
            },
            "permissions": { "network": [], "secrets": [], "callExtensionKinds": [] }
        },
        "contributions": {}
    })
}

const SCHEMA: &str = r#"{"type":"object","properties":{"service_url":{"type":"string","format":"uri","title":"Service URL"},"collection":{"type":"string"}},"required":["service_url"]}"#;

#[test]
fn a_describe_without_config_schema_still_parses() {
    let d = base();
    let parsed: DescribeJson =
        serde_json::from_value(d).expect("a describe omitting configSchema must still parse");
    assert!(
        parsed.config_schema.is_none(),
        "an omitted configSchema must stay absent, not default to a string"
    );
}

/// The other half of the omitted case: re-serializing must not *introduce*
/// the key. A `null` or `""` appearing in a re-signed describe would change
/// the canonical bytes of every already-published extension.
#[test]
fn an_omitted_config_schema_does_not_appear_on_reserialize() {
    let parsed: DescribeJson = serde_json::from_value(base()).expect("fixture must parse");
    let out = serde_json::to_value(&parsed).expect("must serialize");
    assert!(
        out.get("configSchema").is_none(),
        "an omitted configSchema must not be emitted, got: {out:#?}"
    );
}

#[test]
fn a_describe_with_config_schema_round_trips() {
    let mut d = base();
    d["configSchema"] = serde_json::json!(SCHEMA);

    let parsed: DescribeJson =
        serde_json::from_value(d.clone()).expect("a describe carrying configSchema must parse");
    assert_eq!(parsed.config_schema.as_deref(), Some(SCHEMA));

    let out = serde_json::to_value(&parsed).expect("must serialize");
    assert_eq!(
        out.get("configSchema").and_then(|v| v.as_str()),
        Some(SCHEMA),
        "configSchema must survive a serialize round-trip verbatim"
    );

    let reparsed: DescribeJson = serde_json::from_value(out).expect("round-trip must reparse");
    assert_eq!(reparsed.config_schema.as_deref(), Some(SCHEMA));
}

#[test]
fn a_config_schema_that_is_not_json_is_rejected() {
    let mut d = base();
    d["configSchema"] = serde_json::json!("{\"type\":\"object\",}");

    let err = serde_json::from_value::<DescribeJson>(d)
        .expect_err("a configSchema that is not valid JSON must be rejected");
    assert!(
        err.to_string().contains("configSchema"),
        "the error must name the field, got: {err}"
    );
}

/// `"42"` and `"null"` parse as JSON but render as an empty form with no
/// error at all — the worst place to discover the typo, because the operator
/// is told the extension needs no configuration.
#[test]
fn a_config_schema_that_is_json_but_not_an_object_is_rejected() {
    for bad in ["42", "null", "\"a string\"", "[]"] {
        let mut d = base();
        d["configSchema"] = serde_json::json!(bad);

        let err = serde_json::from_value::<DescribeJson>(d)
            .expect_err("a configSchema that is not a JSON object must be rejected")
            .to_string();
        assert!(
            err.contains("not a JSON object"),
            "the error for {bad:?} must say why, got: {err}"
        );
    }
}

/// What an *older* contract crate does when it meets a describe carrying
/// this field, pinned as behaviour rather than left to a changelog note.
///
/// `DescribeJson` is `#[serde(deny_unknown_fields)]`, so a top-level key the
/// crate does not know is a hard parse error naming the key — not a silently
/// dropped field. This test stands in for "an older contract meets
/// `configSchema`" by using a key no contract version will ever know; the
/// mechanism is identical, and unlike a pinned old dependency it cannot rot.
/// The fail-closed direction is the wanted one: the extension does not load,
/// rather than loading with the operator's configuration silently discarded
/// and the form never rendered. See `compat::MIN_DESIGNER_VERSION`.
#[test]
fn an_unknown_top_level_key_is_rejected_cleanly_not_half_parsed() {
    let mut d = base();
    d["configSchemaFromTheFuture"] = serde_json::json!("{\"type\":\"object\"}");

    let err = serde_json::from_value::<DescribeJson>(d)
        .expect_err("an unknown top-level key must be rejected, not ignored")
        .to_string();
    assert!(
        err.contains("unknown field") && err.contains("configSchemaFromTheFuture"),
        "the rejection must name the offending key, got: {err}"
    );
}

/// The field must also be declared in `describe-v2.json`, whose root is
/// `additionalProperties: false`. Without this the describe deserializes
/// perfectly and still fails `gtdx validate` — the trap
/// `schema_v2_views.rs` was written for.
#[test]
fn a_describe_with_config_schema_validates_against_the_v2_schema() {
    let mut d = base();
    d["configSchema"] = serde_json::json!(SCHEMA);
    validate_describe_json(&d).expect("a describe carrying configSchema must validate");
}

/// The schema types it `"string"`, so an inline object is a schema-layer
/// error as well as a serde-layer one.
#[test]
fn an_inline_object_config_schema_fails_schema_validation() {
    let mut d = base();
    d["configSchema"] = serde_json::json!({ "type": "object" });
    assert!(
        validate_describe_json(&d).is_err(),
        "configSchema must be stringly-encoded, not an inline object"
    );
}
