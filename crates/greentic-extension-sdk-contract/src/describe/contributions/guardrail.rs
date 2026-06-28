//! `Guardrail` — a content-moderation guardrail contributed by an extension.
//! `export` names the component's WIT export implementing
//! `greentic:extension-design/guardrail.evaluate`; `runtime_ref` selects which
//! component in `Runtime.components` provides it (if absent the runtime selects
//! the only component declared).

use serde::{Deserialize, Serialize};

use crate::component_id::ComponentId;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Guardrail {
    /// Fully-qualified WIT export, e.g.
    /// `greentic:extension-design/guardrail.evaluate`.
    pub export: String,
    /// The runtime component (by id) that provides the guardrail. Absent ⇒ the
    /// runtime selects the sole declared component.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_ref: Option<ComponentId>,
}
