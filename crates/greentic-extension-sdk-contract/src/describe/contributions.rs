//! Typed `contributions` block. Eight children, each its own typed list.

use serde::{Deserialize, Serialize};

pub mod dw_provider;
pub mod guardrail;
pub mod knowledge;
pub mod node_type;
pub mod prompt;
pub mod recipe;
pub mod schema;
pub mod tool;

pub use dw_provider::DwProvider;
pub use guardrail::Guardrail;
pub use knowledge::Knowledge;
pub use node_type::{NodeType, OutputPort};
pub use prompt::Prompt;
pub use recipe::Recipe;
pub use schema::Schema;
pub use tool::Tool;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct Contributions {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub node_types: Vec<NodeType>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<Tool>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub recipes: Vec<Recipe>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub knowledge: Vec<Knowledge>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub prompts: Vec<Prompt>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub schemas: Vec<Schema>,
    #[serde(rename = "dwProviders", default, skip_serializing_if = "Vec::is_empty")]
    pub dw_providers: Vec<DwProvider>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub guardrails: Vec<Guardrail>,
}

#[cfg(test)]
mod tests {
    #[test]
    fn contributions_omits_empty_dw_providers() {
        let c = super::Contributions::default();
        let s = serde_json::to_string(&c).unwrap();
        assert!(
            !s.contains("dwProviders"),
            "empty dwProviders must not serialize"
        );
    }
}
