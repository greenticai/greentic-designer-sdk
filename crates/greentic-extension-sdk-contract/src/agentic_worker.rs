//! Per-tool metadata and capability declarations for the `agentic_worker`
//! runtime context. See design spec
//! `greentic-designer/docs/superpowers/specs/2026-05-24-extension-capability-flag-design.md`.

use serde::{Deserialize, Serialize};

/// Runtime contexts a tool can be invoked from.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ToolCapability {
    Flow,
    AgenticWorker,
}

impl ToolCapability {
    /// Wire string used in `tool-definition.capabilities` JSON / WIT lists.
    #[must_use]
    pub fn as_wire_str(self) -> &'static str {
        match self {
            ToolCapability::Flow => "flow",
            ToolCapability::AgenticWorker => "agentic_worker",
        }
    }
}

/// Side-effect classification surfaced to the planning layer.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SideEffects {
    None,
    Read,
    Write,
    External,
}

/// Cost classification for ranking when multiple tools satisfy the same intent.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Cost {
    Low,
    Medium,
    High,
}

/// One few-shot example for the LLM.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UsageExample {
    pub when: String,
    pub input: serde_json::Value,
}

/// Decoded representation of `tool-definition.agentic-worker-metadata` JSON blob.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct AgenticWorkerMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub examples: Option<Vec<UsageExample>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side_effects: Option<SideEffects>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<Cost>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmation_required: Option<bool>,
}

impl AgenticWorkerMetadata {
    /// Encode this metadata as the JSON string format that flows over the
    /// WIT `tool-definition.agentic-worker-metadata` field.
    pub fn encode(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Decode the JSON string stored in `tool-definition.agentic-worker-metadata`.
    /// Returns the metadata as-stored — call [`Self::with_conservative_defaults`]
    /// if you want missing fields filled in with safe runtime defaults.
    pub fn decode(blob: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(blob)
    }

    /// Returns a new copy with conservative defaults applied to any field
    /// the extension left as `None`. Per spec: when a tool declares
    /// `agentic_worker` capability but ships no metadata, runtime treats
    /// it as `External` side-effects + `confirmation_required: true` until
    /// the extension declares otherwise.
    #[must_use]
    pub fn with_conservative_defaults(mut self) -> Self {
        if self.side_effects.is_none() {
            self.side_effects = Some(SideEffects::External);
        }
        if self.confirmation_required.is_none() {
            self.confirmation_required = Some(true);
        }
        if self.cost.is_none() {
            self.cost = Some(Cost::Medium);
        }
        self
    }
}
