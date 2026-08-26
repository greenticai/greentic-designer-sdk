//! `Addon` — a managed service an extension offers to an environment, e.g.
//! Qdrant or Redis.
//!
//! This is catalogue metadata only. It tells the Designer that an addon
//! exists, what its configuration form looks like, and what values it hands
//! back to the services that bind to it. Nothing here provisions anything:
//! the platform owns provisioning, and the addon declares only what it needs
//! (spec D6). That split is what lets one declaration serve both hosted and
//! bring-your-own-cloud placement.

use serde::{Deserialize, Serialize};

/// Scalar type of a value an addon hands back once it is running.
///
/// Deliberately three scalars and no object or array: an output is consumed
/// by string interpolation into another resource's configuration
/// (`${resources.qdrant.outputs.url}`), and a structured value has no
/// meaningful rendering there. An addon that wants to expose structure
/// exposes several outputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputType {
    Text,
    Number,
    Boolean,
}

/// One value an addon publishes once provisioned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputSpec {
    /// Referenced as `${resources.<resource_id>.outputs.<name>}`. Constrained
    /// by `gtdx lint` to characters that survive being turned into an
    /// environment variable, because that is what the platform does with it.
    pub name: String,

    /// `output_type` in Rust because `type` is a keyword; `type` on the wire.
    #[serde(rename = "type")]
    pub output_type: OutputType,

    /// A sensitive output never becomes a literal value. The platform
    /// resolves it to a secret reference — `valueFrom.secretKeyRef` on
    /// Kubernetes, a `sensitive` variable in generated `IaC` — so it never
    /// passes through a plan document, a plan UI, or a support bundle
    /// (spec §4.3). Getting this flag wrong is how a Redis password ends up
    /// in a log.
    #[serde(default)]
    pub sensitive: bool,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// One addon an extension offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Addon {
    /// Unique within the extension. The platform namespaces it as
    /// `<extension_id>/<id>`.
    pub id: String,

    /// What kind of thing this is — `vector-db`, `cache`, `sql`. A flow that
    /// needs a vector database asks for the family, not the vendor, so a
    /// deployment can substitute one implementation for another.
    ///
    /// An open string rather than a closed enum, for the same reason `View`
    /// keeps `slot` open: `describe.json` is signed and immutable once
    /// published, so a closed enum in it rots the way this project's
    /// hard-coded kind lists did. `gtdx lint` warns on an unfamiliar family
    /// instead.
    pub family: String,

    pub display_name: String,
    pub description: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,

    /// JSON Schema (Draft 2020-12) for the knobs a user sets per environment
    /// — size, replicas, version. The Designer renders this as a form.
    /// Stringly-encoded for the same reason `NodeType.config_schema` is:
    /// it is a payload passed through to a renderer, not host control data.
    pub config_schema: String,

    /// JSON Schema for the day-2 state the addon reconciles — Qdrant
    /// collections, Redis ACL users.
    ///
    /// **Secrets do not belong here** (spec D16). A password inside desired
    /// state can never be read back by `observe`, so it diffs forever and no
    /// plan is ever clean. Credentials reach the addon through its runtime
    /// binding instead. `gtdx lint` reports a secret-looking property here as
    /// an error.
    pub desired_state_schema: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<OutputSpec>,

    /// Whether the addon can snapshot before a destructive change. The
    /// platform offers to back up on the strength of this flag, so declare
    /// `true` only when a snapshot genuinely happens.
    #[serde(default)]
    pub supports_backup: bool,

    /// Version of THIS addon's `desired_state_schema`, not of the addon
    /// itself. It lets one extension migrate instances from a v1 shape to a
    /// v2 shape rather than breaking them (spec D17). Defaults to 1 so
    /// existing declarations stay valid.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
}

const fn default_schema_version() -> u32 {
    1
}
