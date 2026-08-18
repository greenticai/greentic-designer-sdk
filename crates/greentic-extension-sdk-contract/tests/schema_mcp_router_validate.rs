//! `wasix:mcp/router` artifacts must validate against the MCP schema, not the
//! design-extension one.
//!
//! These components share `apiVersion: greentic.ai/v2` with designer extensions
//! but are a different artifact shape: they carry no `engine`/`contributions`
//! (their tools are discovered at runtime via the router's `list-tools`). The
//! store already dispatches on `kind` before `apiVersion` for exactly this
//! reason; before this test existed, `gtdx publish` did not, so it rejected the
//! very describe.json that `gtdx new --kind mcp --from-openapi` had just
//! generated — making it impossible to publish a generated MCP connector.

use greentic_extension_sdk_contract::schema::validate_describe_json;

/// A real `gtdx new --kind mcp --from-openapi` output, trimmed of nothing that
/// matters. Note: no `engine`, no `contributions`, and it carries
/// `secret_requirements` — all three of which the design-extension v2 schema
/// rejects.
const GENERATED_MCP_ROUTER: &str = r#"{
  "$schema": "https://store.greentic.cloud/schemas/describe-v2.json",
  "apiVersion": "greentic.ai/v2",
  "kind": "wasix:mcp/router",
  "capabilities": { "offered": [], "required": [] },
  "compat": {
    "contract_version": "1.3.0-research.0",
    "min_designer_version": ">=1.3.0-research.0",
    "min_runner_version": "^1.3.0-research.0"
  },
  "metadata": {
    "author": { "name": "Greentic" },
    "description": "A wasix:mcp/router component scaffolded by gtdx.",
    "id": "com.greentic.connector.petstore",
    "license": "Apache-2.0",
    "name": "petstore",
    "summary": "A local-WASM MCP server exposing a wasix:mcp/router tool.",
    "version": "0.1.0"
  },
  "runtime": {
    "components": {
      "petstore": {
        "gtpack": {
          "component_version": "0.1.0",
          "file": "extension.wasm",
          "pack_id": "com.greentic.connector.petstore",
          "sha256": "0000000000000000000000000000000000000000000000000000000000000000"
        },
        "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
        "world": "com-greentic-connector:petstore/mcp-router@1.0.0"
      }
    },
    "memoryLimitMB": 64,
    "permissions": { "callExtensionKinds": [], "network": [], "secrets": [] },
    "world": "com-greentic-connector:petstore/mcp-router@1.0.0"
  },
  "secret_requirements": []
}"#;

#[test]
fn generated_mcp_router_describe_validates() {
    let value: serde_json::Value =
        serde_json::from_str(GENERATED_MCP_ROUTER).expect("fixture must parse");
    validate_describe_json(&value).expect(
        "a gtdx-generated wasix:mcp/router describe must validate; if this fails, \
         `gtdx publish` cannot ship a generated MCP connector",
    );
}

/// The MCP schema must still be a real gate, not a rubber stamp: `runtime` is
/// mandatory, so a router missing it has to be rejected.
#[test]
fn mcp_router_missing_runtime_is_rejected() {
    let mut value: serde_json::Value =
        serde_json::from_str(GENERATED_MCP_ROUTER).expect("fixture must parse");
    value
        .as_object_mut()
        .expect("object")
        .remove("runtime")
        .expect("fixture has runtime");
    let err =
        validate_describe_json(&value).expect_err("a router without `runtime` must be rejected");
    assert!(
        err.to_string().contains("runtime"),
        "error should name the missing field, got: {err}"
    );
}

/// Dispatch is on `kind`, so a design extension must keep going to the v2
/// schema — a router-shaped document (no `engine`/`contributions`) that claims
/// to be a `DesignExtension` must still fail.
#[test]
fn design_extension_still_validates_against_v2() {
    let mut value: serde_json::Value =
        serde_json::from_str(GENERATED_MCP_ROUTER).expect("fixture must parse");
    value.as_object_mut().expect("object").insert(
        "kind".to_string(),
        serde_json::Value::String("DesignExtension".to_string()),
    );
    validate_describe_json(&value).expect_err(
        "a router-shaped document claiming kind DesignExtension must still be \
         rejected by the v2 schema",
    );
}

/// Schema validation is only half the publish path: `gtdx publish` then
/// deserializes the document into [`DescribeJson`]. A `wasix:mcp/router` carries
/// a top-level `runtime.world` (the WIT world the router exports), which design
/// extensions do not — and `Runtime` is `deny_unknown_fields`. Before this was
/// modelled, publish passed schema validation and then died with
/// "unknown field `world`".
#[test]
fn generated_mcp_router_describe_deserializes() {
    use greentic_extension_sdk_contract::describe::DescribeJson;

    let parsed: DescribeJson = serde_json::from_str(GENERATED_MCP_ROUTER).expect(
        "a gtdx-generated wasix:mcp/router describe must deserialize; if this \
         fails, `gtdx publish` dies after schema validation passes",
    );
    assert_eq!(
        parsed.runtime.world.as_deref(),
        Some("com-greentic-connector:petstore/mcp-router@1.0.0"),
        "the router's WIT world must survive deserialization"
    );
}
