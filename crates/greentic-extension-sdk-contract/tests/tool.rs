use greentic_extension_sdk_contract::ComponentId;
use greentic_extension_sdk_contract::describe::Tool;
use std::str::FromStr;

#[test]
fn parses_minimal_tool() {
    let v = serde_json::json!({
        "name": "validate_card",
        "export": "greentic:extension-design/validation.validate-content"
    });
    let t: Tool = serde_json::from_value(v).unwrap();
    assert_eq!(t.name, "validate_card");
    assert!(t.runtime_ref.is_none());
}

#[test]
fn parses_tool_with_runtime_ref() {
    let v = serde_json::json!({
        "name": "analyze_card",
        "export": "greentic:extension-design/tools.invoke-tool",
        "runtime_ref": "adaptive-card"
    });
    let t: Tool = serde_json::from_value(v).unwrap();
    assert_eq!(
        t.runtime_ref.unwrap(),
        ComponentId::from_str("adaptive-card").unwrap()
    );
}

#[test]
fn unknown_field_rejected() {
    let v = serde_json::json!({
        "name": "x",
        "export": "y",
        "lol": "z"
    });
    let r: Result<Tool, _> = serde_json::from_value(v);
    assert!(r.is_err());
}

#[test]
fn v2_tool_carries_capabilities_and_secret_requirements() {
    let json = r#"{
        "name":"search","export":"greentic:extension-design/tools.invoke-tool",
        "capabilities":["agentic_worker"],
        "secret_requirements":[{"key":"tavily/api_key","required":true,"description":"Tavily key"}]
    }"#;
    let t: greentic_extension_sdk_contract::describe::Tool = serde_json::from_str(json).unwrap();
    assert_eq!(t.capabilities, Some(vec!["agentic_worker".to_string()]));
    assert_eq!(t.secret_requirements.len(), 1);
    assert!(t.secret_requirements[0].required);

    // SecretKey serializes as transparent string
    assert_eq!(
        serde_json::to_value(&t.secret_requirements[0].key).unwrap(),
        serde_json::json!("tavily/api_key")
    );

    // backward compat: a minimal tool parses + round-trips byte-identical (no empty fields emitted)
    let min: greentic_extension_sdk_contract::describe::Tool =
        serde_json::from_str(r#"{"name":"x","export":"e"}"#).unwrap();
    assert_eq!(min.capabilities, None);
    assert!(min.secret_requirements.is_empty());
    assert_eq!(
        serde_json::to_string(&min).unwrap(),
        r#"{"name":"x","export":"e"}"#
    );
}
