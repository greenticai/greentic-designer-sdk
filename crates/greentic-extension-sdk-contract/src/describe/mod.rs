use serde::{Deserialize, Serialize};

use crate::capability::CapabilityRef;
use crate::kind::ExtensionKind;

pub mod contributions;
pub mod localization_block;
pub mod provider;

pub use contributions::{
    Contributions, Knowledge, NodeType, OutputPort, Prompt, Recipe, Schema, Tool,
};
pub use localization_block::Localization;

/// Top-level descriptor for a Greentic extension (v2 shape).
///
/// Invariants enforced at deserialize time:
/// - `runtime.components` must have at least one entry
/// - `execution.is_some()` only when `kind == Bundle`
/// - every `runtime_ref` in `contributions.node_types` and `contributions.tools`
///   must reference a key that exists in `runtime.components`
#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DescribeJson {
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema_ref: Option<String>,
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: ExtensionKind,
    pub compat: crate::compat::Compat,
    pub metadata: Metadata,
    pub engine: Engine,
    pub capabilities: Capabilities,
    pub runtime: Runtime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution: Option<serde_json::Value>,
    pub contributions: contributions::Contributions,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub localization: Option<localization_block::Localization>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<Signature>,
    /// SHA-256 (lowercase hex) of the canonical `manifest.json`. Binds the
    /// whole-archive ledger into the signed describe (audit C2/H7). Optional
    /// only for backward compat during migration; production packs MUST set it.
    #[serde(
        rename = "manifestSha256",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub manifest_sha256: Option<String>,
}

/// Private intermediate for deserialization — identical shape to `DescribeJson`.
/// `TryFrom` validates the invariants before constructing the real type.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DescribeJsonRaw {
    #[serde(rename = "$schema", default)]
    schema_ref: Option<String>,
    #[serde(rename = "apiVersion")]
    api_version: String,
    kind: ExtensionKind,
    compat: crate::compat::Compat,
    metadata: Metadata,
    engine: Engine,
    capabilities: Capabilities,
    runtime: Runtime,
    #[serde(default)]
    execution: Option<serde_json::Value>,
    contributions: contributions::Contributions,
    #[serde(default)]
    localization: Option<localization_block::Localization>,
    #[serde(default)]
    signature: Option<Signature>,
    #[serde(rename = "manifestSha256", default)]
    manifest_sha256: Option<String>,
}

impl TryFrom<DescribeJsonRaw> for DescribeJson {
    type Error = String;

    fn try_from(raw: DescribeJsonRaw) -> Result<Self, String> {
        if raw.runtime.components.is_empty() {
            return Err("runtime.components must declare at least one entry".into());
        }

        // Mirror the JSON Schema bound (memoryLimitMB ∈ [1, 1024]) in the type
        // itself, so a doc deserialized without first running schema validation
        // can't carry 0 or a multi-gigabyte value (audit cycle-2 N8).
        if !(1..=1024).contains(&raw.runtime.memory_limit_mb) {
            return Err(format!(
                "runtime.memoryLimitMB must be in [1, 1024] (got {})",
                raw.runtime.memory_limit_mb
            ));
        }

        if raw.execution.is_some() && raw.kind != ExtensionKind::Bundle {
            return Err(format!(
                "`execution` is only allowed when kind=BundleExtension (got kind={:?})",
                raw.kind
            ));
        }

        let known: std::collections::BTreeSet<&crate::component_id::ComponentId> =
            raw.runtime.components.keys().collect();
        for nt in &raw.contributions.node_types {
            if let Some(rr) = &nt.runtime_ref
                && !known.contains(rr)
            {
                return Err(format!(
                    "node_type {:?} runtime_ref {:?} not in runtime.components",
                    nt.type_id, rr
                ));
            }
        }
        for tool in &raw.contributions.tools {
            if let Some(rr) = &tool.runtime_ref
                && !known.contains(rr)
            {
                return Err(format!(
                    "tool {:?} runtime_ref {:?} not in runtime.components",
                    tool.name, rr
                ));
            }
        }

        Ok(DescribeJson {
            schema_ref: raw.schema_ref,
            api_version: raw.api_version,
            kind: raw.kind,
            compat: raw.compat,
            metadata: raw.metadata,
            engine: raw.engine,
            capabilities: raw.capabilities,
            runtime: raw.runtime,
            execution: raw.execution,
            contributions: raw.contributions,
            localization: raw.localization,
            signature: raw.signature,
            manifest_sha256: raw.manifest_sha256,
        })
    }
}

impl<'de> serde::Deserialize<'de> for DescribeJson {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = DescribeJsonRaw::deserialize(d)?;
        Self::try_from(raw).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metadata {
    pub id: String,
    pub name: String,
    pub version: String,
    pub summary: crate::localization::LocalizedString,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<crate::localization::LocalizedString>,
    pub author: Author,
    pub license: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub screenshots: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Author {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(rename = "publicKey", default, skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Engine {
    #[serde(rename = "greenticDesigner")]
    pub greentic_designer: String,
    #[serde(rename = "extRuntime")]
    pub ext_runtime: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    #[serde(default)]
    pub offered: Vec<CapabilityRef>,
    #[serde(default)]
    pub required: Vec<CapabilityRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Runtime {
    #[serde(rename = "memoryLimitMB", default = "default_memory")]
    pub memory_limit_mb: u32,
    pub permissions: Permissions,
    pub components: std::collections::BTreeMap<
        crate::component_id::ComponentId,
        crate::runtime_component::RuntimeComponent,
    >,
}

const fn default_memory() -> u32 {
    64
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Permissions {
    #[serde(default)]
    pub network: Vec<String>,
    #[serde(default)]
    pub secrets: Vec<String>,
    #[serde(rename = "callExtensionKinds", default)]
    pub call_extension_kinds: Vec<String>,
    /// LLM roles (wire names, e.g. `"sorla_composer"`) this extension is allowed
    /// to request from the host `greentic:extension-host/llm` import. The host
    /// resolves each role to a tenant-configured provider; an empty list means
    /// the extension may not call the LLM host import at all.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub llm_roles: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignatureAlgorithm {
    #[serde(rename = "ed25519")]
    Ed25519,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Signature {
    pub algorithm: SignatureAlgorithm,
    #[serde(rename = "publicKey")]
    pub public_key: String,
    pub value: String,
    #[serde(rename = "keyId", default, skip_serializing_if = "Option::is_none")]
    pub key_id: Option<String>,
}

impl DescribeJson {
    #[must_use]
    pub fn identity_key(&self) -> String {
        format!("{}@{}", self.metadata.id, self.metadata.version)
    }
}
