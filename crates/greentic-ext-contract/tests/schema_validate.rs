use greentic_ext_contract::schema::validate_describe_json;

#[test]
fn accepts_valid_design_ext() {
    let v = serde_json::json!({
      "apiVersion": "greentic.ai/v1",
      "kind": "DesignExtension",
      "metadata": {
        "id": "greentic.adaptive-cards",
        "name": "AC",
        "version": "1.6.0",
        "summary": "x",
        "author": { "name": "G" },
        "license": "MIT"
      },
      "engine": { "greenticDesigner": ">=0.1", "extRuntime": "^0.1" },
      "capabilities": {
        "offered": [{ "id": "greentic:ac/validate", "version": "1.0.0" }],
        "required": []
      },
      "runtime": {
        "component": "ext.wasm",
        "permissions": {}
      },
      "contributions": { "schemas": [] }
    });
    validate_describe_json(&v).unwrap();
}

#[test]
fn rejects_missing_kind() {
    let v = serde_json::json!({
      "apiVersion": "greentic.ai/v1",
      "metadata": {
        "id": "x.y", "name": "x", "version": "1.0.0",
        "summary": "x", "author": { "name": "x" }, "license": "MIT"
      },
      "engine": { "greenticDesigner": "*", "extRuntime": "*" },
      "capabilities": {},
      "runtime": { "component": "e.wasm", "permissions": {} },
      "contributions": {}
    });
    assert!(validate_describe_json(&v).is_err());
}

const BASE_OK: &str = r#"{
  "apiVersion": "greentic.ai/v1",
  "kind": "DesignExtension",
  "metadata": {
    "id": "greentic.x", "name": "X", "version": "1.0.0",
    "summary": "x", "author": { "name": "G" }, "license": "MIT"
  },
  "engine": { "greenticDesigner": "*", "extRuntime": "*" },
  "capabilities": { "offered": [{ "id": "greentic:x/y", "version": "1.0.0" }] },
  "runtime": { "component": "e.wasm", "permissions": {} },
  "contributions": {}
}"#;

#[test]
fn rejects_bad_capability_id() {
    let mut v: serde_json::Value = serde_json::from_str(BASE_OK).unwrap();
    v["capabilities"]["offered"][0]["id"] = serde_json::json!("NO_COLON_HERE");
    assert!(validate_describe_json(&v).is_err());
}
