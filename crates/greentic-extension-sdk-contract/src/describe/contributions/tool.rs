//! `Tool` — design-time tool exposed by an extension. `runtime_ref` slots
//! a single `ComponentId` to dispatch through; if absent the runtime selects
//! the only component declared in `Runtime.components`.

use serde::{Deserialize, Serialize};

use crate::component_id::ComponentId;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Tool {
    pub name: String,
    pub export: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_ref: Option<ComponentId>,
}
