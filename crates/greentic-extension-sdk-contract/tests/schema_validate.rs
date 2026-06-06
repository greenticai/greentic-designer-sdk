use greentic_extension_sdk_contract::schema::validate_describe_json;

/// Audit P0-1: `validate_describe_json` is the publish / dev-install gate and
/// only accepts `greentic.ai/v2`. A `greentic.ai/v1` document must be rejected
/// outright (it was previously down-validated against the looser v1 schema,
/// reporting a false success before dying with a confusing serde error when
/// deserialized into the v2-only `DescribeJson`).
#[test]
fn rejects_v1_api_version_outright() {
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
    let err = validate_describe_json(&v).unwrap_err();
    assert!(
        err.to_string().contains("greentic.ai/v1"),
        "error should name the unsupported apiVersion, got: {err}"
    );
}

#[test]
fn rejects_missing_api_version() {
    let v = serde_json::json!({
      "kind": "DesignExtension",
      "metadata": {
        "id": "x.y", "name": "x", "version": "1.0.0",
        "summary": "x", "author": { "name": "x" }, "license": "MIT"
      }
    });
    assert!(validate_describe_json(&v).is_err());
}

const BASE_V2_OK: &str = r#"{
  "$schema": "https://store.greentic.cloud/schemas/describe-v2.json",
  "apiVersion": "greentic.ai/v2",
  "kind": "DesignExtension",
  "compat": {
    "min_designer_version": ">=1.2.0",
    "min_runner_version": "^1.2.0",
    "contract_version": "1.2.0"
  },
  "metadata": {
    "id": "greentic.x", "name": "X", "version": "1.0.0",
    "summary": "x", "author": { "name": "G" }, "license": "MIT"
  },
  "engine": { "greenticDesigner": ">=1.2.0", "extRuntime": "^1.2.0" },
  "capabilities": { "offered": [{ "id": "greentic:x/y", "version": "1.0.0" }], "required": [] },
  "runtime": {
    "components": {
      "main": {
        "gtpack": {
          "file": "extension.wasm",
          "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
          "pack_id": "greentic.x",
          "component_version": "1.0.0"
        },
        "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
        "world": "greentic-x/extension@1.0.0"
      }
    },
    "permissions": { "network": [], "secrets": [], "callExtensionKinds": [] }
  },
  "contributions": {}
}"#;

#[test]
fn accepts_valid_v2_design_ext() {
    let v: serde_json::Value = serde_json::from_str(BASE_V2_OK).unwrap();
    validate_describe_json(&v).unwrap();
}

#[test]
fn rejects_bad_capability_id() {
    let mut v: serde_json::Value = serde_json::from_str(BASE_V2_OK).unwrap();
    v["capabilities"]["offered"][0]["id"] = serde_json::json!("NO_COLON_HERE");
    assert!(validate_describe_json(&v).is_err());
}
