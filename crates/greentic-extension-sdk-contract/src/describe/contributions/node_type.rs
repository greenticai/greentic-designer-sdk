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
    /// Named operation to invoke on the component named by `runtime_ref`.
    ///
    /// A runtime component may expose several operations, which is what lets
    /// one component back many palette entries: an integration ships a single
    /// component and one `NodeType` per operation, rather than one component
    /// per operation.
    ///
    /// The designer compiles a node type into a `component.exec` body of
    /// `{ component, operation, input }`, and the runner REQUIRES `operation`
    /// — it refuses the node with "expected node.component.operation to be
    /// set" rather than assuming a default. Leaving it unset on a
    /// multi-operation component therefore fails at execution time only:
    /// the palette, the flow builder and the pack build all report success
    /// first.
    ///
    /// `None` for a component that exposes exactly one operation and needs no
    /// selector.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<Deprecated>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OutputPort {
    pub name: String,
    pub label: LocalizedString,
}
