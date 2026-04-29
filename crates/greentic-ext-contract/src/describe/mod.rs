use serde::{Deserialize, Serialize};

use crate::capability::CapabilityRef;
use crate::kind::ExtensionKind;

pub mod provider;
pub use provider::RuntimeGtpack;

/// Top-level descriptor for a Greentic extension.
///
/// Invariants enforced at deserialize time:
/// - `kind == Provider`  ↔  `runtime.gtpack.is_some()` (required)
/// - `kind == Design` with `runtime.gtpack.is_some()` requires `contributions.nodeTypes`
///   to be a non-empty array (node-providing design extension)
/// - `runtime.gtpack.is_some()` is forbidden on all other kinds
/// - `execution.is_some()` only when `kind == Bundle`
///
/// `execution` is a pass-through `serde_json::Value` at contract level;
/// each `BundleExtension`'s own reader parses the typed shape
/// (`{kind: "builtin", builtinId: "..."}` or `{kind: "wasm"}`).
#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DescribeJson {
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema_ref: Option<String>,
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: ExtensionKind,
    pub metadata: Metadata,
    pub engine: Engine,
    pub capabilities: Capabilities,
    pub runtime: Runtime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution: Option<serde_json::Value>,
    pub contributions: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<Signature>,
}

/// Private intermediate for deserialization — identical shape to `DescribeJson`.
/// `TryFrom` validates the kind-specific invariants before constructing the real type.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DescribeJsonRaw {
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    schema_ref: Option<String>,
    #[serde(rename = "apiVersion")]
    api_version: String,
    kind: ExtensionKind,
    metadata: Metadata,
    engine: Engine,
    capabilities: Capabilities,
    runtime: Runtime,
    #[serde(default)]
    execution: Option<serde_json::Value>,
    contributions: serde_json::Value,
    #[serde(default)]
    signature: Option<Signature>,
}

impl TryFrom<DescribeJsonRaw> for DescribeJson {
    type Error = String;

    fn try_from(raw: DescribeJsonRaw) -> Result<Self, String> {
        let has_gtpack = raw.runtime.gtpack.is_some();
        let has_node_types = raw
            .contributions
            .get("nodeTypes")
            .and_then(|v| v.as_array())
            .is_some_and(|a| !a.is_empty());

        match (raw.kind, has_gtpack) {
            (ExtensionKind::Provider, false) => {
                return Err("kind=ProviderExtension requires `runtime.gtpack` to be set".into());
            }
            (ExtensionKind::Design, true) if !has_node_types => {
                return Err(
                    "DesignExtension with `runtime.gtpack` must contribute `nodeTypes` \
                     (gtpack is only justified when the extension teaches the runtime new node types)"
                        .into(),
                );
            }
            (k, true) if k != ExtensionKind::Provider && k != ExtensionKind::Design => {
                return Err(format!(
                    "runtime.gtpack is only allowed for ProviderExtension, or for \
                     DesignExtension that contributes `nodeTypes` (got kind={k:?})"
                ));
            }
            _ => {}
        }
        if raw.execution.is_some() && raw.kind != ExtensionKind::Bundle {
            return Err(format!(
                "`execution` is only allowed when kind=BundleExtension (got kind={:?})",
                raw.kind
            ));
        }
        Ok(DescribeJson {
            schema_ref: raw.schema_ref,
            api_version: raw.api_version,
            kind: raw.kind,
            metadata: raw.metadata,
            engine: raw.engine,
            capabilities: raw.capabilities,
            runtime: raw.runtime,
            execution: raw.execution,
            contributions: raw.contributions,
            signature: raw.signature,
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
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
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
    pub component: String,
    #[serde(rename = "memoryLimitMB", default = "default_memory")]
    pub memory_limit_mb: u32,
    pub permissions: Permissions,
    /// Provider-only: bundled `.gtpack` artifact metadata.
    /// Present if and only if `kind == ProviderExtension`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gtpack: Option<RuntimeGtpack>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Signature {
    pub algorithm: String,
    #[serde(rename = "publicKey")]
    pub public_key: String,
    pub value: String,
}

impl DescribeJson {
    #[must_use]
    pub fn identity_key(&self) -> String {
        format!("{}@{}", self.metadata.id, self.metadata.version)
    }
}
