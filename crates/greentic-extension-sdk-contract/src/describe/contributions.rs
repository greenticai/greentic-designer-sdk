//! Typed `contributions` block. Six children, each its own typed list.

use serde::{Deserialize, Serialize};

pub mod knowledge;
pub mod node_type;
pub mod prompt;
pub mod recipe;
pub mod schema;
pub mod tool;

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
}
