//! `contributions.tools[]` is the ONLY tool surface a `greentic.ai/v2`
//! extension has. `ExtensionRuntime::list_tools` short-circuits on the contract
//! version and never calls the wasm's `list-tools` export, so any field missing
//! from this struct is unreachable in production no matter what the extension
//! declares in WIT. These tests pin the full set.

use greentic_extension_sdk_contract::describe::Tool;
use greentic_extension_sdk_contract::{AgenticWorkerMetadata, Cost, SideEffects};

fn full_tool_json() -> serde_json::Value {
    serde_json::json!({
        "name": "run_telco_playbook",
        "export": "greentic:extension-design/tools.invoke-tool",
        "runtime_ref": "telco-x",
        "capabilities": ["flow", "agentic_worker"],
        "description": "Run a telco-x playbook with inputs; returns a status card.",
        "input_schema": r#"{"type":"object","properties":{"playbook_id":{"type":"string"}}}"#,
        "output_schema": r#"{"type":"object","properties":{"card":{"type":"object"}}}"#,
        "agentic_worker_metadata": r#"{"usage_hint":"Execute a playbook","side_effects":"external","cost":"medium","confirmation_required":false}"#,
    })
}

#[test]
fn full_tool_declaration_parses() {
    let t: Tool = serde_json::from_value(full_tool_json()).expect("parses");
    assert_eq!(t.name, "run_telco_playbook");
    assert_eq!(
        t.capabilities.as_deref(),
        Some(["flow".to_string(), "agentic_worker".to_string()].as_slice())
    );
    assert!(t.description.is_some());
    assert!(t.input_schema.is_some());
    assert!(t.output_schema.is_some());
    assert!(t.agentic_worker_metadata.is_some());
}

/// The metadata rides as an opaque string so this crate never blocks on a
/// field it does not know yet, but what a well-formed extension puts there
/// must decode into the typed contract — that is the whole point of the field.
#[test]
fn agentic_worker_metadata_decodes_into_the_typed_contract() {
    let t: Tool = serde_json::from_value(full_tool_json()).expect("parses");
    let meta =
        AgenticWorkerMetadata::decode(t.agentic_worker_metadata.as_deref().expect("present"))
            .expect("decodes");
    assert_eq!(meta.usage_hint.as_deref(), Some("Execute a playbook"));
    assert_eq!(meta.side_effects, Some(SideEffects::External));
    assert_eq!(meta.cost, Some(Cost::Medium));
    assert_eq!(meta.confirmation_required, Some(false));
}

/// Additive means additive: a describe written before these fields existed
/// must still parse, and must not gain them on the way back out.
#[test]
fn older_describes_parse_and_round_trip_without_the_new_fields() {
    let minimal = serde_json::json!({
        "name": "t",
        "export": "greentic:extension-design/tools.invoke-tool",
    });
    let t: Tool = serde_json::from_value(minimal.clone()).expect("legacy tool parses");
    assert!(t.output_schema.is_none());
    assert!(t.agentic_worker_metadata.is_none());

    let back = serde_json::to_value(&t).expect("serializes");
    assert_eq!(
        back, minimal,
        "absent fields must stay absent — a re-serialized legacy describe must \
         not sprout nulls the store's schema never saw"
    );
}

#[test]
fn round_trip_preserves_every_field() {
    let original = full_tool_json();
    let t: Tool = serde_json::from_value(original.clone()).expect("parses");
    let back = serde_json::to_value(&t).expect("serializes");
    assert_eq!(back, original);
}

/// `deny_unknown_fields` is on `Tool`, so a typo in a describe is a hard parse
/// error rather than a silently ignored field. Worth pinning: the failure mode
/// this whole struct exists to prevent is "declared it, nothing happened".
#[test]
fn unknown_tool_field_is_rejected() {
    let typo = serde_json::json!({
        "name": "t",
        "export": "greentic:extension-design/tools.invoke-tool",
        "agentic_worker_meta": "{}",
    });
    let err = serde_json::from_value::<Tool>(typo).unwrap_err();
    assert!(
        err.to_string().contains("agentic_worker_meta"),
        "the rejected field should be named: {err}"
    );
}
