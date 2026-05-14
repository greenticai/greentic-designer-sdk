use std::sync::LazyLock;

use jsonschema::{Draft, Validator};

use crate::error::ContractError;

const SCHEMA_V1: &str = include_str!("../schemas/describe-v1.json");
const SCHEMA_V2: &str = include_str!("../schemas/describe-v2.json");

static VALIDATOR_V1: LazyLock<Validator> = LazyLock::new(|| {
    let schema: serde_json::Value =
        serde_json::from_str(SCHEMA_V1).expect("embedded v1 schema must parse");
    jsonschema::options()
        .with_draft(Draft::Draft202012)
        .build(&schema)
        .expect("embedded v1 schema must compile")
});

static VALIDATOR_V2: LazyLock<Validator> = LazyLock::new(|| {
    let schema: serde_json::Value =
        serde_json::from_str(SCHEMA_V2).expect("embedded v2 schema must parse");
    jsonschema::options()
        .with_draft(Draft::Draft202012)
        .build(&schema)
        .expect("embedded v2 schema must compile")
});

/// Validate a raw describe.json `Value` against the appropriate schema version.
///
/// Dispatches to the v2 schema when `apiVersion == "greentic.ai/v2"`, and
/// falls back to the v1 schema otherwise (for legacy descriptors).
pub fn validate_describe_json(value: &serde_json::Value) -> Result<(), ContractError> {
    let is_v2 = value.get("apiVersion").and_then(|v| v.as_str()) == Some("greentic.ai/v2");

    let validator = if is_v2 {
        &*VALIDATOR_V2
    } else {
        &*VALIDATOR_V1
    };

    let errors: Vec<String> = validator
        .iter_errors(value)
        .map(|e| format!("{}: {}", e.instance_path, e))
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ContractError::SchemaInvalid(errors.join("; ")))
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
