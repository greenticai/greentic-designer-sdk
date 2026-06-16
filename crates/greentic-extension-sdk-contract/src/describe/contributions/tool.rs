//! `Tool` — design-time tool exposed by an extension. `runtime_ref` slots
//! a single `ComponentId` to dispatch through; if absent the runtime selects
//! the only component declared in `Runtime.components`.

use greentic_types::secrets::SecretRequirement;
use serde::{Deserialize, Serialize};

use crate::component_id::ComponentId;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Tool {
    pub name: String,
    pub export: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_ref: Option<ComponentId>,
    /// Runtime contexts the tool supports (e.g. `["agentic_worker"]`).
    /// Absent → consumers default to `["flow"]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<String>>,
    /// Secrets/credentials this tool needs. Each `key` is a path
    /// (e.g. `tavily/api_key`); the host URI is `secret://<key>`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secret_requirements: Vec<SecretRequirement>,
}
