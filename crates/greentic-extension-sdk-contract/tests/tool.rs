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
