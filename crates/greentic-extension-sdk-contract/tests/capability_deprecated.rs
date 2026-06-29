use greentic_extension_sdk_contract::CapabilityRef;

#[test]
fn capability_ref_without_deprecated_parses() {
    let v = serde_json::json!({
        "id": "greentic:adaptive-cards/validate",
        "version": "1.0.0"
    });
    let c: CapabilityRef = serde_json::from_value(v).unwrap();
    assert!(c.deprecated.is_none());
}

#[test]
fn capability_ref_with_deprecated_parses() {
    let v = serde_json::json!({
        "id": "greentic:adaptive-cards/legacy",
        "version": "0.9.0",
        "deprecated": { "since": "1.0.0", "replaced_by": "greentic:adaptive-cards/validate" }
    });
    let c: CapabilityRef = serde_json::from_value(v).unwrap();
    let d = c.deprecated.unwrap();
    assert_eq!(d.since.to_string(), "1.0.0");
}
