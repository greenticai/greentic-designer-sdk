use greentic_extension_sdk_contract::{
    AgenticWorkerMetadata, Cost, SideEffects, ToolCapability, UsageExample,
};

#[test]
fn round_trip_fully_populated() {
    let meta = AgenticWorkerMetadata {
        usage_hint: Some("Call after the user pastes a card.".into()),
        examples: Some(vec![UsageExample {
            when: "user pasted JSON".into(),
            input: serde_json::json!({ "card": { "type": "AdaptiveCard" } }),
        }]),
        side_effects: Some(SideEffects::None),
        cost: Some(Cost::Low),
        confirmation_required: Some(false),
    };

    let json = serde_json::to_string(&meta).expect("encode");
    let back: AgenticWorkerMetadata = serde_json::from_str(&json).expect("decode");
    assert_eq!(back, meta);
}

#[test]
fn round_trip_default_is_empty() {
    let meta = AgenticWorkerMetadata::default();
    let json = serde_json::to_string(&meta).expect("encode");
    // All fields skip_serializing_if = None → empty object.
    assert_eq!(json, "{}");
    let back: AgenticWorkerMetadata = serde_json::from_str(&json).expect("decode");
    assert_eq!(back, meta);
}

#[test]
fn round_trip_partial_only_usage_hint() {
    let meta = AgenticWorkerMetadata {
        usage_hint: Some("hint".into()),
        ..Default::default()
    };
    let json = serde_json::to_string(&meta).expect("encode");
    assert_eq!(json, r#"{"usage_hint":"hint"}"#);
    let back: AgenticWorkerMetadata = serde_json::from_str(&json).expect("decode");
    assert_eq!(back, meta);
}

#[test]
fn rejects_unknown_field() {
    let json = r#"{"usage_hint":"x","bogus_field":1}"#;
    let err = serde_json::from_str::<AgenticWorkerMetadata>(json).unwrap_err();
    assert!(
        err.to_string().contains("unknown field"),
        "expected deny_unknown_fields error, got: {err}"
    );
}

#[test]
fn rejects_unknown_field_in_usage_example() {
    let json = r#"{
        "examples": [{ "when": "x", "input": {}, "extra": true }]
    }"#;
    let err = serde_json::from_str::<AgenticWorkerMetadata>(json).unwrap_err();
    assert!(
        err.to_string().contains("unknown field"),
        "expected deny_unknown_fields error in UsageExample, got: {err}"
    );
}

#[test]
fn side_effects_wire_format_snake_case() {
    let meta = AgenticWorkerMetadata {
        side_effects: Some(SideEffects::External),
        ..Default::default()
    };
    let json = serde_json::to_string(&meta).unwrap();
    assert!(
        json.contains(r#""side_effects":"external""#),
        "expected snake_case wire format, got: {json}"
    );
}

#[test]
fn cost_wire_format_snake_case() {
    let meta = AgenticWorkerMetadata {
        cost: Some(Cost::Medium),
        ..Default::default()
    };
    let json = serde_json::to_string(&meta).unwrap();
    assert!(
        json.contains(r#""cost":"medium""#),
        "expected snake_case wire format, got: {json}"
    );
}

#[test]
fn tool_capability_wire_strings() {
    assert_eq!(ToolCapability::Flow.as_wire_str(), "flow");
    assert_eq!(
        ToolCapability::AgenticWorker.as_wire_str(),
        "agentic_worker"
    );
}

#[test]
fn tool_capability_serde_round_trip() {
    let caps = vec![ToolCapability::Flow, ToolCapability::AgenticWorker];
    let json = serde_json::to_string(&caps).unwrap();
    assert_eq!(json, r#"["flow","agentic_worker"]"#);
    let back: Vec<ToolCapability> = serde_json::from_str(&json).unwrap();
    assert_eq!(back, caps);
}

#[test]
fn encode_decode_round_trip() {
    let meta = AgenticWorkerMetadata {
        usage_hint: Some("hi".into()),
        ..Default::default()
    };
    let blob = meta.encode().unwrap();
    let back = AgenticWorkerMetadata::decode(&blob).unwrap();
    assert_eq!(back, meta);
}

#[test]
fn conservative_defaults_only_fill_missing() {
    let meta = AgenticWorkerMetadata {
        side_effects: Some(SideEffects::None),
        cost: Some(Cost::Low),
        confirmation_required: Some(false),
        ..Default::default()
    };
    let filled = meta.clone().with_conservative_defaults();
    assert_eq!(filled, meta, "no defaults should overwrite explicit values");
}

#[test]
fn conservative_defaults_fill_when_all_missing() {
    let filled = AgenticWorkerMetadata::default().with_conservative_defaults();
    assert_eq!(filled.side_effects, Some(SideEffects::External));
    assert_eq!(filled.confirmation_required, Some(true));
    assert_eq!(filled.cost, Some(Cost::Medium));
    // usage_hint + examples remain None — no safe default.
    assert!(filled.usage_hint.is_none());
    assert!(filled.examples.is_none());
}
