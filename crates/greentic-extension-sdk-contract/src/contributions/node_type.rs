use std::{collections::HashSet, sync::LazyLock};

use jsonschema::{Draft, Validator};
use serde::{Deserialize, Serialize};

use crate::error::ContractError;

const NODE_TYPES_SCHEMA_JSON: &str = include_str!("../../schemas/designer-node-types.schema.json");

static NODE_TYPES_SCHEMA: LazyLock<Validator> = LazyLock::new(|| {
    let schema: serde_json::Value =
        serde_json::from_str(NODE_TYPES_SCHEMA_JSON).expect("embedded schema must parse");
    jsonschema::options()
        .with_draft(Draft::Draft202012)
        .build(&schema)
        .expect("embedded node type schema must compile")
});

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Contributions {
    #[serde(rename = "nodeTypes", default, skip_serializing_if = "Vec::is_empty")]
    pub node_types: Vec<DesignerNodeType>,

    #[serde(flatten)]
    pub other: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DesignerNodeType {
    #[serde(rename = "type_id")]
    pub type_id: String,
    pub label: String,
    pub category: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub complexity: Option<String>,

    #[serde(
        rename = "config_schema",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub config_schema: Option<String>,

    #[serde(
        rename = "output_ports",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub output_ports: Vec<NodeOutputPort>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NodeOutputPort {
    pub name: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

pub fn validate_contributions(value: &serde_json::Value) -> Result<Contributions, ContractError> {
    validate_contributions_schema(value)?;
    let contributions: Contributions = serde_json::from_value(value.clone())?;
    let mut seen = HashSet::new();
    for node in &contributions.node_types {
        validate_node_type(node)?;
        if !seen.insert(node.type_id.as_str()) {
            return Err(ContractError::NodeTypeInvalid(format!(
                "duplicate node type id: {}",
                node.type_id
            )));
        }
    }
    Ok(contributions)
}

pub fn validate_contributions_schema(value: &serde_json::Value) -> Result<(), ContractError> {
    let errors: Vec<String> = NODE_TYPES_SCHEMA
        .iter_errors(value)
        .map(|e| format!("{}: {}", e.instance_path(), e))
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ContractError::NodeTypeInvalid(errors.join("; ")))
    }
}

pub fn validate_node_type(node: &DesignerNodeType) -> Result<(), ContractError> {
    if node.type_id.trim().is_empty() {
        return Err(ContractError::NodeTypeInvalid(
            "type_id must not be empty".into(),
        ));
    }
    if node.label.trim().is_empty() {
        return Err(ContractError::NodeTypeInvalid(format!(
            "label must not be empty for node type {}",
            node.type_id
        )));
    }
    if node.category.trim().is_empty() {
        return Err(ContractError::NodeTypeInvalid(format!(
            "category must not be empty for node type {}",
            node.type_id
        )));
    }

    let mut ports = HashSet::new();
    for port in &node.output_ports {
        if port.name.trim().is_empty() {
            return Err(ContractError::NodeTypeInvalid(format!(
                "output port name must not be empty for node type {}",
                node.type_id
            )));
        }
        if port.label.trim().is_empty() {
            return Err(ContractError::NodeTypeInvalid(format!(
                "output port label must not be empty for node type {}",
                node.type_id
            )));
        }
        if !ports.insert(port.name.as_str()) {
            return Err(ContractError::NodeTypeInvalid(format!(
                "duplicate output port name '{}' for node type {}",
                port.name, node.type_id
            )));
        }
    }

    if let Some(config_schema) = &node.config_schema {
        let schema: serde_json::Value = serde_json::from_str(config_schema).map_err(|e| {
            ContractError::NodeTypeInvalid(format!(
                "config_schema for node type {} is invalid JSON: {e}",
                node.type_id
            ))
        })?;
        jsonschema::options()
            .with_draft(Draft::Draft202012)
            .build(&schema)
            .map_err(|e| {
                ContractError::NodeTypeInvalid(format!(
                    "config_schema for node type {} is not a valid JSON Schema: {e}",
                    node.type_id
                ))
            })?;
        reject_secret_like_values(
            &schema,
            &format!("node type {} config_schema", node.type_id),
        )?;
    }

    reject_secret_like_str(&node.type_id, "type_id")?;
    reject_secret_like_str(&node.label, "label")?;
    reject_secret_like_str(&node.category, "category")?;
    if let Some(icon) = &node.icon {
        reject_secret_like_str(icon, "icon")?;
    }
    if let Some(color) = &node.color {
        reject_secret_like_str(color, "color")?;
    }
    if let Some(complexity) = &node.complexity {
        reject_secret_like_str(complexity, "complexity")?;
    }
    for port in &node.output_ports {
        reject_secret_like_str(&port.name, "output port name")?;
        reject_secret_like_str(&port.label, "output port label")?;
        if let Some(description) = &port.description {
            reject_secret_like_str(description, "output port description")?;
        }
    }

    Ok(())
}

fn reject_secret_like_values(
    value: &serde_json::Value,
    context: &str,
) -> Result<(), ContractError> {
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                if is_secret_like(key) {
                    return Err(ContractError::NodeTypeInvalid(format!(
                        "{context} contains secret-looking key: {key}"
                    )));
                }
                reject_secret_like_values(value, context)?;
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                reject_secret_like_values(value, context)?;
            }
        }
        serde_json::Value::String(value) => reject_secret_like_str(value, context)?,
        _ => {}
    }
    Ok(())
}

fn reject_secret_like_str(value: &str, context: &str) -> Result<(), ContractError> {
    if is_secret_like(value) {
        return Err(ContractError::NodeTypeInvalid(format!(
            "{context} contains secret-looking value"
        )));
    }
    Ok(())
}

fn is_secret_like(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase().replace(['-', ' '], "_");
    normalized.contains("secret")
        || normalized.contains("password")
        || normalized.contains("passwd")
        || normalized.contains("api_key")
        || normalized.contains("apikey")
        || normalized.contains("access_token")
        || normalized.contains("refresh_token")
        || normalized == "token"
        || normalized.ends_with("_token")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_node() -> DesignerNodeType {
        DesignerNodeType {
            type_id: "example.echo".into(),
            label: "Echo".into(),
            category: "tools".into(),
            icon: Some("puzzle".into()),
            color: Some("#0d9488".into()),
            complexity: Some("simple".into()),
            config_schema: Some(r#"{"type":"object"}"#.into()),
            output_ports: vec![NodeOutputPort {
                name: "success".into(),
                label: "Success".into(),
                description: None,
            }],
        }
    }

    #[test]
    fn valid_node_type_passes() {
        validate_node_type(&valid_node()).unwrap();
    }

    #[test]
    fn invalid_config_schema_json_is_rejected() {
        let mut node = valid_node();
        node.config_schema = Some("{".into());
        let err = validate_node_type(&node).unwrap_err();
        assert!(err.to_string().contains("invalid JSON"));
    }

    #[test]
    fn duplicate_output_ports_are_rejected() {
        let mut node = valid_node();
        node.output_ports.push(NodeOutputPort {
            name: "success".into(),
            label: "Again".into(),
            description: None,
        });
        let err = validate_node_type(&node).unwrap_err();
        assert!(err.to_string().contains("duplicate output port"));
    }

    #[test]
    fn duplicate_node_types_are_rejected() {
        let value = serde_json::json!({
            "nodeTypes": [
                valid_node(),
                valid_node(),
            ]
        });
        let err = validate_contributions(&value).unwrap_err();
        assert!(err.to_string().contains("duplicate node type id"));
    }
}
