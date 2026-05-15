use greentic_extension_sdk_contract::validate_contributions_schema;
use greentic_extension_sdk_testing::{
    assert_invalid_node_type_contributions, assert_valid_node_type_contributions,
    load_node_type_fixture,
};

#[test]
fn valid_node_type_fixtures_pass() {
    for name in ["valid-wasm-component-node", "valid-business-action-node"] {
        let fixture = load_node_type_fixture(name);
        assert_valid_node_type_contributions(&fixture);
    }
}

#[test]
fn invalid_node_type_fixtures_fail() {
    for name in [
        "invalid-bad-config-schema",
        "invalid-duplicate-node-type-id",
    ] {
        let fixture = load_node_type_fixture(name);
        assert_invalid_node_type_contributions(&fixture);
    }
}

#[test]
fn node_type_schema_validates_fixture_shape() {
    let fixture = load_node_type_fixture("valid-wasm-component-node");
    validate_contributions_schema(&fixture).unwrap();
}

#[test]
fn wasm_component_scaffold_node_type_shape_validates() {
    let scaffold_like = serde_json::json!({
        "nodeTypes": [
            {
                "type_id": "snap",
                "label": "Snap",
                "category": "tools",
                "icon": "puzzle",
                "color": "#0d9488",
                "complexity": "simple",
                "config_schema": "{}",
                "output_ports": [
                    { "name": "success", "label": "Success" },
                    { "name": "error", "label": "Error" }
                ]
            }
        ]
    });
    assert_valid_node_type_contributions(&scaffold_like);
}
