//! `Addon` is catalogue metadata: it tells the Designer an addon exists and
//! what its configuration form looks like. Nothing here executes.

use greentic_extension_sdk_contract::describe::contributions::{Addon, OutputType};

fn qdrant_json() -> serde_json::Value {
    serde_json::json!({
        "id": "qdrant",
        "family": "vector-db",
        "display_name": "Qdrant",
        "description": "Vector database for similarity search.",
        "config_schema": "{\"type\":\"object\",\"properties\":{\"replicas\":{\"type\":\"integer\"}}}",
        "desired_state_schema": "{\"type\":\"object\",\"properties\":{\"collections\":{\"type\":\"array\"}}}",
        "outputs": [
            { "name": "url", "type": "text" },
            { "name": "api_key", "type": "text", "sensitive": true, "description": "Bearer token." }
        ],
        "supports_backup": true,
        "schema_version": 1
    })
}

#[test]
fn a_full_addon_round_trips() {
    let addon: Addon = serde_json::from_value(qdrant_json()).expect("addon deserializes");

    assert_eq!(addon.id, "qdrant");
    assert_eq!(addon.family, "vector-db");
    assert_eq!(addon.outputs.len(), 2);
    assert_eq!(addon.outputs[1].name, "api_key");
    assert!(
        addon.outputs[1].sensitive,
        "api_key must be marked sensitive"
    );
    assert!(!addon.outputs[0].sensitive, "sensitive defaults to false");
    assert!(addon.supports_backup);
    assert_eq!(addon.schema_version, 1);

    let back = serde_json::to_value(&addon).expect("addon serializes");
    let round: Addon = serde_json::from_value(back).expect("round-trips");
    assert_eq!(round, addon);
}

/// The wire key is `type`, not `output_type` — `output_type` only exists
/// because `type` is a Rust keyword.
#[test]
fn output_type_serialises_as_type() {
    let addon: Addon = serde_json::from_value(qdrant_json()).expect("deserializes");
    let v = serde_json::to_value(&addon).expect("serializes");
    let first = &v["outputs"][0];
    assert_eq!(first["type"], "text", "got: {first}");
    assert!(
        first.get("output_type").is_none(),
        "output_type must not reach the wire"
    );
}

#[test]
fn optional_fields_may_be_omitted() {
    let minimal = serde_json::json!({
        "id": "redis",
        "family": "cache",
        "display_name": "Redis",
        "description": "In-memory key-value store.",
        "config_schema": "{\"type\":\"object\"}",
        "desired_state_schema": "{\"type\":\"object\"}"
    });
    let addon: Addon = serde_json::from_value(minimal).expect("minimal addon deserializes");

    assert!(addon.icon.is_none());
    assert!(addon.outputs.is_empty());
    assert!(!addon.supports_backup, "supports_backup defaults to false");
    assert_eq!(addon.schema_version, 1, "schema_version defaults to 1");
}

/// `deny_unknown_fields` catches a typo'd key at parse time rather than
/// silently dropping the value the author meant to set.
#[test]
fn an_unknown_field_is_rejected() {
    let mut v = qdrant_json();
    v["supports_backups"] = serde_json::json!(true); // note the trailing s
    let r: Result<Addon, _> = serde_json::from_value(v);
    assert!(r.is_err(), "an unknown field must be rejected");
}

#[test]
fn every_output_type_parses() {
    for (wire, expected) in [
        ("text", OutputType::Text),
        ("number", OutputType::Number),
        ("boolean", OutputType::Boolean),
    ] {
        let v = serde_json::json!({ "name": "x", "type": wire });
        let spec: greentic_extension_sdk_contract::describe::contributions::OutputSpec =
            serde_json::from_value(v).unwrap_or_else(|e| panic!("{wire} should parse: {e}"));
        assert_eq!(spec.output_type, expected);
    }
}
