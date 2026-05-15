#[must_use]
pub fn load_node_type_fixture(name: &str) -> serde_json::Value {
    let json = match name {
        "valid-wasm-component-node" | "valid-wasm-component-node.json" => {
            include_str!("../fixtures/node-types/valid-wasm-component-node.json")
        }
        "invalid-bad-config-schema" | "invalid-bad-config-schema.json" => {
            include_str!("../fixtures/node-types/invalid-bad-config-schema.json")
        }
        "invalid-duplicate-node-type-id" | "invalid-duplicate-node-type-id.json" => {
            include_str!("../fixtures/node-types/invalid-duplicate-node-type-id.json")
        }
        "valid-business-action-node" | "valid-business-action-node.json" => {
            include_str!("../fixtures/node-types/valid-business-action-node.json")
        }
        _ => panic!("unknown node type fixture: {name}"),
    };
    serde_json::from_str(json).expect("embedded node type fixture must parse")
}

pub fn assert_valid_node_type_contributions(value: &serde_json::Value) {
    greentic_extension_sdk_contract::validate_contributions(value)
        .expect("expected valid node type contributions");
}

pub fn assert_invalid_node_type_contributions(value: &serde_json::Value) {
    greentic_extension_sdk_contract::validate_contributions(value)
        .expect_err("expected invalid node type contributions");
}
