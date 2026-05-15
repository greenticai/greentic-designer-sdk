use greentic_extension_sdk_contract::{
    Contributions, DescribeJson, validate_contributions, validate_contributions_schema,
};

fn valid_contributions() -> serde_json::Value {
    serde_json::json!({
        "nodeTypes": [
            {
                "type_id": "example.echo",
                "label": "Echo",
                "category": "tools",
                "icon": "puzzle",
                "color": "#0d9488",
                "complexity": "simple",
                "config_schema": "{\"type\":\"object\"}",
                "output_ports": [
                    { "name": "success", "label": "Success" },
                    { "name": "error", "label": "Error" }
                ]
            }
        ],
        "otherContribution": {
            "still": "allowed"
        }
    })
}

fn describe_with_contributions(contributions: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "apiVersion": "greentic.ai/v1",
        "kind": "DesignExtension",
        "metadata": {
            "id": "greentic.node-test",
            "name": "Node Test",
            "version": "0.1.0",
            "summary": "fixture",
            "author": { "name": "Greentic" },
            "license": "MIT"
        },
        "engine": { "greenticDesigner": "*", "extRuntime": "^0.1.0" },
        "capabilities": { "offered": [], "required": [] },
        "runtime": {
            "component": "extension.wasm",
            "memoryLimitMB": 32,
            "permissions": { "network": [], "secrets": [], "callExtensionKinds": [] }
        },
        "contributions": contributions
    })
}

#[test]
fn valid_node_type_contribution_passes() {
    let contributions = validate_contributions(&valid_contributions()).unwrap();
    assert_eq!(contributions.node_types.len(), 1);
    assert_eq!(contributions.node_types[0].type_id, "example.echo");
    assert!(contributions.other.contains_key("otherContribution"));
}

#[test]
fn node_type_schema_validates_contributions() {
    validate_contributions_schema(&valid_contributions()).unwrap();
}

#[test]
fn typed_rust_model_roundtrips_to_json() {
    let typed = validate_contributions(&valid_contributions()).unwrap();
    let value = serde_json::to_value(&typed).unwrap();
    let parsed: Contributions = serde_json::from_value(value).unwrap();
    assert_eq!(parsed, typed);
}

#[test]
fn duplicate_node_type_ids_are_rejected() {
    let value = serde_json::json!({
        "nodeTypes": [
            {
                "type_id": "example.echo",
                "label": "Echo",
                "category": "tools"
            },
            {
                "type_id": "example.echo",
                "label": "Echo again",
                "category": "tools"
            }
        ]
    });
    let err = validate_contributions(&value).unwrap_err();
    assert!(err.to_string().contains("duplicate node type id"));
}

#[test]
fn schema_rejects_unknown_node_type_field() {
    let value = serde_json::json!({
        "nodeTypes": [
            {
                "type_id": "example.echo",
                "label": "Echo",
                "category": "tools",
                "unexpected": true
            }
        ]
    });
    let err = validate_contributions_schema(&value).unwrap_err();
    assert!(err.to_string().contains("unexpected"));
}

#[test]
fn invalid_config_schema_json_is_rejected() {
    let value = serde_json::json!({
        "nodeTypes": [
            {
                "type_id": "example.echo",
                "label": "Echo",
                "category": "tools",
                "config_schema": "{"
            }
        ]
    });
    let err = validate_contributions(&value).unwrap_err();
    assert!(err.to_string().contains("invalid JSON"));
}

#[test]
fn invalid_config_schema_is_rejected() {
    let value = serde_json::json!({
        "nodeTypes": [
            {
                "type_id": "example.echo",
                "label": "Echo",
                "category": "tools",
                "config_schema": "{\"type\":1}"
            }
        ]
    });
    let err = validate_contributions(&value).unwrap_err();
    assert!(err.to_string().contains("valid JSON Schema"));
}

#[test]
fn duplicate_output_port_names_are_rejected() {
    let value = serde_json::json!({
        "nodeTypes": [
            {
                "type_id": "example.echo",
                "label": "Echo",
                "category": "tools",
                "output_ports": [
                    { "name": "success", "label": "Success" },
                    { "name": "success", "label": "Again" }
                ]
            }
        ]
    });
    let err = validate_contributions(&value).unwrap_err();
    assert!(err.to_string().contains("duplicate output port"));
}

#[test]
fn old_raw_contributions_still_deserialize() {
    let contributions = serde_json::json!({
        "schemas": [],
        "unknownFutureContribution": { "enabled": true }
    });
    let describe: DescribeJson =
        serde_json::from_value(describe_with_contributions(&contributions)).unwrap();
    assert_eq!(
        describe.contributions["unknownFutureContribution"]["enabled"],
        true
    );
}

#[test]
fn describe_typed_contributions_works() {
    let describe: DescribeJson =
        serde_json::from_value(describe_with_contributions(&valid_contributions())).unwrap();
    let typed = describe.typed_contributions().unwrap();
    assert_eq!(typed.node_types[0].label, "Echo");
}

#[test]
fn design_extension_with_runtime_gtpack_still_requires_node_types() {
    let mut raw = describe_with_contributions(&serde_json::json!({}));
    raw["runtime"]["gtpack"] = serde_json::json!({
        "file": "runtime/x.gtpack",
        "sha256": "a".repeat(64),
        "pack_id": "test.pack",
        "component_version": "0.6.0"
    });
    let err = serde_json::from_value::<DescribeJson>(raw).unwrap_err();
    assert!(err.to_string().contains("nodeTypes"), "got: {err}");
}
