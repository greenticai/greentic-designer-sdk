use std::sync::LazyLock;

use jsonschema::{Draft, Validator};

use crate::error::ContractError;
use crate::kind::ExtensionKind;

const SCHEMA_V2: &str = include_str!("../schemas/describe-v2.json");
const SCHEMA_MCP_V1: &str = include_str!("../schemas/describe-mcp-v1.json");

/// `kind` value identifying a local-WASM MCP component. Delegates to
/// `ExtensionKind::WasixMcpRouter::wire_name()` (a `const fn`, so this stays
/// a `const`) rather than repeating the literal — a second copy would let
/// the wire string drift out of sync with the enum without either test
/// noticing.
const WASIX_MCP_ROUTER_KIND: &str = ExtensionKind::WasixMcpRouter.wire_name();

static VALIDATOR_V2: LazyLock<Validator> = LazyLock::new(|| {
    let schema: serde_json::Value =
        serde_json::from_str(SCHEMA_V2).expect("embedded v2 schema must parse");
    jsonschema::options()
        .with_draft(Draft::Draft202012)
        .build(&schema)
        .expect("embedded v2 schema must compile")
});

static VALIDATOR_MCP_V1: LazyLock<Validator> = LazyLock::new(|| {
    let schema: serde_json::Value =
        serde_json::from_str(SCHEMA_MCP_V1).expect("embedded mcp v1 schema must parse");
    jsonschema::options()
        .with_draft(Draft::Draft202012)
        .build(&schema)
        .expect("embedded mcp v1 schema must compile")
});

/// Validate a raw describe.json `Value` for the publish / dev-install path.
///
/// Only `apiVersion == "greentic.ai/v2"` is accepted. Earlier versions are
/// rejected outright (audit P0-1): the publish and install paths immediately
/// deserialize the document into the v2-only [`crate::describe::DescribeJson`]
/// struct, so down-validating a `greentic.ai/v1` document against the looser v1
/// schema reports a false success and then dies with a confusing serde error.
/// There is no in-place migration to point authors at: `gtdx` ships no
/// `migrate` command, so the only routes to a v2 document are installing a v2
/// build of the extension or re-scaffolding and re-publishing it. The error
/// text says exactly that rather than naming a command that does not exist.
pub fn validate_describe_json(value: &serde_json::Value) -> Result<(), ContractError> {
    // `wasix:mcp/router` artifacts share `apiVersion: greentic.ai/v2` with
    // designer extensions but are a distinct artifact shape, so they are
    // dispatched by `kind` BEFORE the apiVersion routing — otherwise they would
    // be (wrongly) validated against the design-extension v2 schema and
    // rejected for lacking `engine`/`contributions`. This mirrors the store's
    // own dispatch (greentic-store-server publish/schema.rs), so the CLI and
    // the server agree on what is publishable.
    if value.get("kind").and_then(|v| v.as_str()) == Some(WASIX_MCP_ROUTER_KIND) {
        return validate_describe_mcp_v1(value);
    }
    match value.get("apiVersion").and_then(|v| v.as_str()) {
        Some("greentic.ai/v2") => validate_describe_v2(value),
        Some(other) => Err(ContractError::UnsupportedApiVersion(format!(
            "{other} (expected greentic.ai/v2; there is no in-place migration — \
             install a v2 build of the extension, or re-scaffold and re-publish \
             it with a current gtdx)"
        ))),
        None => Err(ContractError::SchemaInvalid(
            "missing apiVersion (expected greentic.ai/v2)".into(),
        )),
    }
}

/// Validate a JSON value against the v2 describe schema explicitly.
///
/// Use when the caller already knows the value is v2-shaped and wants to
/// validate directly against the v2 schema, regardless of the `apiVersion` field.
pub fn validate_describe_v2(value: &serde_json::Value) -> Result<(), ContractError> {
    let errors: Vec<String> = VALIDATOR_V2
        .iter_errors(value)
        .map(|e| format!("{}: {}", e.instance_path, e))
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ContractError::SchemaInvalid(errors.join("; ")))
    }
}

/// Validate a `kind: wasix:mcp/router` describe against the MCP schema.
///
/// Local-WASM MCP components are not designer extensions: their tools are
/// discovered at runtime through the router's `list-tools`, so they carry no
/// `engine`/`contributions` blocks and only `apiVersion`/`kind`/`metadata`/
/// `runtime` are mandated. The embedded schema is a copy of the store's
/// `describe-mcp-v1.json`; keep the two in sync.
pub fn validate_describe_mcp_v1(value: &serde_json::Value) -> Result<(), ContractError> {
    let errors: Vec<String> = VALIDATOR_MCP_V1
        .iter_errors(value)
        .map(|e| format!("{}: {}", e.instance_path, e))
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ContractError::SchemaInvalid(errors.join("; ")))
    }
}
