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
fn guardrails_contribution_passes_schema_and_typed_parse() {
    let mut v: serde_json::Value = serde_json::from_str(VALID).unwrap();
    v["contributions"]["guardrails"] = serde_json::json!([
        { "export": "greentic:extension-design/guardrail.evaluate", "runtime_ref": "x" }
    ]);

    // JSON-schema layer must allow the guardrails contribution.
    validate_describe_v2(&v).expect("guardrails contribution should pass describe-v2 schema");

    // Typed (deny_unknown_fields) layer must parse it into the struct.
    let parsed: DescribeJson =
        serde_json::from_value(v).expect("guardrails contribution should parse typed");
    assert_eq!(parsed.contributions.guardrails.len(), 1);
    assert_eq!(
        parsed.contributions.guardrails[0].export,
        "greentic:extension-design/guardrail.evaluate"
    );
}

#[test]
fn connection_test_contribution_passes_schema_and_roundtrips() {
    let mut v: serde_json::Value = serde_json::from_str(VALID).unwrap();
    v["contributions"]["connection_test"] = serde_json::json!({
        "tool": "get_server_version",
        "args": { "cluster": "default" }
    });

    // JSON-schema layer must allow the connection_test contribution.
    validate_describe_v2(&v).expect("connection_test contribution should pass describe-v2 schema");

    // Typed (deny_unknown_fields) layer must parse it into the struct.
    let parsed: DescribeJson =
        serde_json::from_value(v).expect("connection_test contribution should parse typed");
    let connection_test = parsed
        .contributions
        .connection_test
        .as_ref()
        .expect("connection_test should be Some");
    assert_eq!(connection_test.tool, "get_server_version");
    assert_eq!(
        connection_test.args,
        serde_json::json!({ "cluster": "default" })
    );

    // Roundtrip: serialize back out and re-parse, no data loss, still snake_case on the wire.
    let serialized = serde_json::to_string(&parsed).unwrap();
    assert!(
        serialized.contains("\"connection_test\":"),
        "connection_test must serialize snake_case on the wire; got: {serialized}"
    );
    let reparsed: DescribeJson = serde_json::from_str(&serialized).unwrap();
    assert_eq!(
        reparsed.contributions.connection_test,
        parsed.contributions.connection_test
    );
}

#[test]
fn connection_test_missing_tool_fails_schema() {
    let mut v: serde_json::Value = serde_json::from_str(VALID).unwrap();
    v["contributions"]["connection_test"] = serde_json::json!({
        "args": { "cluster": "default" }
    });
    assert!(
        validate_describe_v2(&v).is_err(),
        "connection_test without required `tool` should fail schema"
    );
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

/// The two layers that can disagree about a contribution, asserted together:
/// the JSON schema (which `gtdx` and the store validate against) and the typed
/// `deny_unknown_fields` struct (which the runtime decodes with). A field added
/// to one and not the other fails in opposite directions — the schema alone
/// admits a describe the runtime then refuses to load, and the struct alone
/// parses a describe `gtdx` refuses to publish — so neither half is evidence
/// on its own.
#[test]
fn messaging_channel_contribution_passes_schema_and_roundtrips() {
    let mut v: serde_json::Value = serde_json::from_str(VALID).unwrap();
    v["contributions"]["messaging_channel"] = serde_json::json!({
        "id": "messaging-3aigent-gui",
        "ref": "oci://ghcr.io/greenticai/packs/messaging/messaging-3aigent-gui@sha256:36a0c547",
        "label": "3AIgent GUI"
    });

    validate_describe_v2(&v)
        .expect("messaging_channel contribution should pass describe-v2 schema");

    let parsed: DescribeJson =
        serde_json::from_value(v).expect("messaging_channel contribution should parse typed");
    let channel = parsed
        .contributions
        .messaging_channel
        .as_ref()
        .expect("messaging_channel should be Some");
    assert_eq!(channel.id, "messaging-3aigent-gui");
    assert_eq!(channel.label.as_deref(), Some("3AIgent GUI"));

    // Snake_case on the wire, matching `connection_test` rather than the
    // block's camelCase siblings.
    let serialized = serde_json::to_string(&parsed).unwrap();
    assert!(
        serialized.contains("\"messaging_channel\":"),
        "messaging_channel must serialize snake_case on the wire; got: {serialized}"
    );
    // And the reference keeps the key `providers-registry.json` already uses.
    assert!(serialized.contains("\"ref\":\"oci://ghcr.io/greenticai/"));
}

/// `label` is the only optional member; a channel without one is the common
/// case, since `metadata.name` is already a display name.
#[test]
fn a_messaging_channel_without_a_label_is_valid() {
    let mut v: serde_json::Value = serde_json::from_str(VALID).unwrap();
    v["contributions"]["messaging_channel"] = serde_json::json!({
        "id": "messaging-x",
        "ref": "oci://ghcr.io/x@sha256:ab"
    });
    validate_describe_v2(&v).expect("a label-less channel should pass the schema");
    let parsed: DescribeJson = serde_json::from_value(v).expect("should parse typed");
    assert!(
        parsed
            .contributions
            .messaging_channel
            .unwrap()
            .label
            .is_none()
    );
}

/// The schema refuses a reference that is not an OCI URI. Everything
/// downstream treats this string as one, and a bare name fails deep inside a
/// bundle build naming neither the extension nor the channel.
#[test]
fn a_messaging_channel_ref_must_be_an_oci_uri() {
    let mut v: serde_json::Value = serde_json::from_str(VALID).unwrap();
    v["contributions"]["messaging_channel"] = serde_json::json!({
        "id": "messaging-x",
        "ref": "ghcr.io/x:latest"
    });
    assert!(
        validate_describe_v2(&v).is_err(),
        "a non-oci:// ref must be refused by the schema"
    );
}
