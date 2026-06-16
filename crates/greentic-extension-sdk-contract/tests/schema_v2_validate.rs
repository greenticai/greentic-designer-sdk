use greentic_extension_sdk_contract::describe::DescribeJson;
use greentic_extension_sdk_contract::schema::validate_describe_v2;

const VALID: &str = r#"{
  "$schema": "https://store.greentic.cloud/schemas/describe-v2.json",
  "apiVersion": "greentic.ai/v2",
  "kind": "DesignExtension",
  "compat": {
    "min_designer_version": ">=1.2.0",
    "min_runner_version": "^0.12.0",
    "contract_version": "1.2.0"
  },
  "metadata": {
    "id": "greentic.x",
    "name": "X",
    "version": "0.1.0",
    "summary": "Plain",
    "author": { "name": "Greentic" },
    "license": "MIT"
  },
  "engine": { "greenticDesigner": ">=1.2.0", "extRuntime": "^0.12.0" },
  "capabilities": { "offered": [], "required": [] },
  "runtime": {
    "memoryLimitMB": 64,
    "permissions": { "network": [], "secrets": [], "callExtensionKinds": [] },
    "components": {
      "x": {
        "oci_ref": "oci://x:1",
        "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
        "world": "x:y@1"
      }
    }
  },
  "contributions": {}
}"#;

#[test]
fn valid_v2_passes_schema() {
    let v: serde_json::Value = serde_json::from_str(VALID).unwrap();
    validate_describe_v2(&v).expect("schema validation failed");
}

#[test]
fn missing_compat_fails_schema() {
    let mut v: serde_json::Value = serde_json::from_str(VALID).unwrap();
    v.as_object_mut().unwrap().remove("compat");
    assert!(validate_describe_v2(&v).is_err());
}

#[test]
fn unknown_contributions_key_fails_schema() {
    let mut v: serde_json::Value = serde_json::from_str(VALID).unwrap();
    v["contributions"]["lol"] = serde_json::json!([]);
    assert!(validate_describe_v2(&v).is_err());
}

#[test]
fn manifest_sha256_valid_hex_passes_schema() {
    let mut v: serde_json::Value = serde_json::from_str(VALID).unwrap();
    let valid_sha256 = "a".repeat(64);
    v["manifestSha256"] = serde_json::json!(valid_sha256);
    validate_describe_v2(&v).expect("64-char lowercase hex manifestSha256 should be valid");
}

#[test]
fn manifest_sha256_invalid_value_fails_schema() {
    let mut v: serde_json::Value = serde_json::from_str(VALID).unwrap();
    // Uppercase hex violates the pattern "^[0-9a-f]{64}$"
    let uppercase_sha256 = "A".repeat(64);
    v["manifestSha256"] = serde_json::json!(uppercase_sha256);
    assert!(
        validate_describe_v2(&v).is_err(),
        "uppercase hex manifestSha256 should be rejected by the pattern ^[0-9a-f]{{64}}$"
    );
}

const WITH_TOOL_SECRETS: &str = r#"{
  "$schema": "https://store.greentic.cloud/schemas/describe-v2.json",
  "apiVersion": "greentic.ai/v2",
  "kind": "DesignExtension",
  "compat": {
    "min_designer_version": ">=1.2.0",
    "min_runner_version": "^0.12.0",
    "contract_version": "1.2.0"
  },
  "metadata": {
    "id": "greentic.tavily-search",
    "name": "Tavily Search",
    "version": "0.1.0",
    "summary": "Web search via Tavily",
    "author": { "name": "Greentic" },
    "license": "MIT"
  },
  "engine": { "greenticDesigner": ">=1.2.0", "extRuntime": "^0.12.0" },
  "capabilities": { "offered": [], "required": [] },
  "runtime": {
    "memoryLimitMB": 64,
    "permissions": { "network": [], "secrets": ["tavily/api_key"], "callExtensionKinds": [] },
    "components": {
      "search": {
        "oci_ref": "oci://ghcr.io/greenticai/tavily-search:0.1.0",
        "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
        "world": "greentic:tavily/search@0.1.0"
      }
    }
  },
  "contributions": {
    "tools": [
      {
        "name": "search",
        "export": "greentic:extension-design/tools.invoke-tool",
        "capabilities": ["agentic_worker"],
        "secret_requirements": [
          { "key": "tavily/api_key", "required": true }
        ]
      }
    ]
  }
}"#;

#[test]
fn tool_with_capabilities_and_secrets_passes_schema_and_deserializes() {
    // (a) validates against the describe-v2 schema (tools.items is open {})
    let v: serde_json::Value = serde_json::from_str(WITH_TOOL_SECRETS).unwrap();
    validate_describe_v2(&v)
        .expect("schema validation must pass for tool with secret_requirements");

    // (b) deserializes correctly into the typed DescribeJson
    let doc: DescribeJson = serde_json::from_str(WITH_TOOL_SECRETS).unwrap();
    let tools = &doc.contributions.tools;
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].secret_requirements.len(), 1);
    assert!(tools[0].secret_requirements[0].required);
    let caps = tools[0].capabilities.as_deref().unwrap_or_default();
    assert_eq!(caps, ["agentic_worker"]);
}
