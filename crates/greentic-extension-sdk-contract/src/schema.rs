use std::sync::LazyLock;

use jsonschema::{Draft, Validator};

use crate::error::ContractError;

const SCHEMA_V2: &str = include_str!("../schemas/describe-v2.json");

static VALIDATOR_V2: LazyLock<Validator> = LazyLock::new(|| {
    let schema: serde_json::Value =
        serde_json::from_str(SCHEMA_V2).expect("embedded v2 schema must parse");
    jsonschema::options()
        .with_draft(Draft::Draft202012)
        .build(&schema)
        .expect("embedded v2 schema must compile")
});

/// Validate a raw describe.json `Value` for the publish / dev-install path.
///
/// Only `apiVersion == "greentic.ai/v2"` is accepted. Earlier versions are
/// rejected outright (audit P0-1): the publish and install paths immediately
/// deserialize the document into the v2-only [`crate::describe::DescribeJson`]
/// struct, so down-validating a `greentic.ai/v1` document against the looser v1
/// schema reports a false success and then dies with a confusing serde error.
/// Authors of legacy documents must run the v1->v2 migration first.
pub fn validate_describe_json(value: &serde_json::Value) -> Result<(), ContractError> {
    match value.get("apiVersion").and_then(|v| v.as_str()) {
        Some("greentic.ai/v2") => validate_describe_v2(value),
        Some(other) => Err(ContractError::UnsupportedApiVersion(format!(
            "{other} (expected greentic.ai/v2; run the v1->v2 migration first)"
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
