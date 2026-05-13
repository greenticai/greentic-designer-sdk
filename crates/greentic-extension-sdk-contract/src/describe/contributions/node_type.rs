//! `NodeType` — palette entry contributed by a design extension. Optional
//! `runtime_ref` is the slot that unblocks the AC `component_ref` decouple:
//! the designer's flow compiler reads this rather than the pinned ref in
//! `flow_generator/catalog.baseline.yaml`.

use serde::{Deserialize, Serialize};

use crate::component_id::ComponentId;
use crate::deprecated::Deprecated;
use crate::localization::LocalizedString;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NodeType {
    pub type_id: String,
    pub label: LocalizedString,
    pub category: String,
    pub icon: String,
    pub color: String,
    pub complexity: String,
    pub config_schema: String,
    #[serde(default)]
    pub output_ports: Vec<OutputPort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_ref: Option<ComponentId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<Deprecated>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OutputPort {
    pub name: String,
    pub label: LocalizedString,
}
