//! Typed `contributions` block. Nine children, each its own typed list, plus
//! the optional `connection_test` self-test descriptor.

use serde::{Deserialize, Serialize};

pub mod connection_test;
pub mod dw_provider;
pub mod guardrail;
pub mod knowledge;
pub mod node_type;
pub mod prompt;
pub mod recipe;
pub mod schema;
pub mod tool;
pub mod view;

pub use connection_test::ConnectionTest;
pub use dw_provider::DwProvider;
pub use guardrail::Guardrail;
pub use knowledge::Knowledge;
pub use node_type::{NodeType, OutputPort};
pub use prompt::Prompt;
pub use recipe::Recipe;
pub use schema::Schema;
pub use tool::Tool;
pub use view::{Placement, Surface, View, Visibility};

// NOTE: no `Eq` here — `ConnectionTest.args` is a `serde_json::Value`, which
// only implements `PartialEq` (its `Number` variant can hold a float).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
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
    /// UI pages contributed to a host surface. Rendered by the Designer or the
    /// Admin from assets shipped under `assets/views/<id>/` in the pack.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<View>,
    /// Optional self-test descriptor: which contributed tool (by name) a
    /// consumer should invoke to verify a live connection/credential.
    /// `snake_case` on the wire — matches how extensions and the designer
    /// already read `contributions.connection_test`.
    #[serde(
        rename = "connection_test",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub connection_test: Option<ConnectionTest>,
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
