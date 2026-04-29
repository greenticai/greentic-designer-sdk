use std::sync::LazyLock;

use jsonschema::{Draft, Validator};

use crate::error::ContractError;

const SCHEMA_V1: &str = include_str!("../schemas/describe-v1.json");

static SCHEMA: LazyLock<Validator> = LazyLock::new(|| {
    let schema: serde_json::Value =
        serde_json::from_str(SCHEMA_V1).expect("embedded schema must parse");
    jsonschema::options()
        .with_draft(Draft::Draft202012)
        .build(&schema)
        .expect("embedded schema must compile")
});

pub fn validate_describe_json(value: &serde_json::Value) -> Result<(), ContractError> {
    let errors: Vec<String> = SCHEMA
        .iter_errors(value)
        .map(|e| format!("{}: {}", e.instance_path, e))
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ContractError::SchemaInvalid(errors.join("; ")))
    }
}
