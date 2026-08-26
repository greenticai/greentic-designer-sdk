use greentic_types::secrets::SecretRequirement;
use serde::{Deserialize, Serialize};

use crate::capability::CapabilityRef;
use crate::kind::ExtensionKind;

pub mod contributions;
pub mod localization_block;
pub mod provider;

pub use contributions::{
    Contributions, DwProvider, Knowledge, NodeType, OutputPort, Placement, Prompt, Recipe, Schema,
    Surface, Tool, View, Visibility,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<Engine>,
    pub capabilities: Capabilities,
    pub runtime: Runtime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution: Option<serde_json::Value>,
    /// Defaulted rather than required because `kind: wasix:mcp/router`
    /// artifacts carry no contributions — a router's tools are discovered at
    /// runtime via `list-tools`. Design extensions still have it mandated, by
    /// the v2 JSON Schema rather than by this struct.
    #[serde(default)]
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
    /// Secrets that this extension requires operators to provision before it can
    /// run. Mirrors the per-tool `secret_requirements` but applies extension-wide.
    #[serde(
        rename = "requiredSecrets",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub required_secrets: Vec<SecretRequirement>,
    /// Snake-case secret requirements emitted by `kind: wasix:mcp/router`
    /// artifacts (the MCP schema types this only as an array of objects, so it
    /// is carried through untyped rather than reinterpreted).
    ///
    /// Kept as an opaque passthrough because the describe is re-serialized and
    /// then signed: a field this crate dropped would silently vanish from the
    /// published artifact. Design extensions use `requiredSecrets` instead and
    /// leave this empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secret_requirements: Vec<serde_json::Value>,
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
    #[serde(default)]
    engine: Option<Engine>,
    capabilities: Capabilities,
    runtime: Runtime,
    #[serde(default)]
    execution: Option<serde_json::Value>,
    #[serde(default)]
    contributions: contributions::Contributions,
    #[serde(default)]
    localization: Option<localization_block::Localization>,
    #[serde(default)]
    signature: Option<Signature>,
    #[serde(rename = "manifestSha256", default)]
    manifest_sha256: Option<String>,
    #[serde(rename = "requiredSecrets", default)]
    required_secrets: Vec<SecretRequirement>,
    #[serde(default)]
    secret_requirements: Vec<serde_json::Value>,
}

/// Invariants the `Addon` type cannot state on its own.
///
/// Addon ids namespace to `<extension_id>/<addon_id>` on the platform, so a
/// duplicate makes one of the two unaddressable. Outputs are addressed by
/// name from other resources' bindings, so a duplicate means
/// `${resources.x.outputs.url}` resolves to whichever entry the platform saw
/// last. A schema that is not JSON — or that parses as JSON but isn't a JSON
/// *object* (`"42"`, `"null"`) — renders as an empty form with no error, the
/// worst place to discover the typo; both are rejected here. And
/// `schema_version` is `u32` in
/// the struct but `"minimum": 1` in `describe-v2.json`; reject `0` here so
/// the two layers agree instead of the schema silently being the stricter
/// one.
fn validate_addons(addons: &[contributions::Addon]) -> Result<(), String> {
    let mut seen_addons = std::collections::BTreeSet::new();
    for addon in addons {
        if !seen_addons.insert(addon.id.as_str()) {
            return Err(format!(
                "contributions.addons[] declares duplicate id {:?}",
                addon.id
            ));
        }

        if addon.schema_version == 0 {
            return Err(format!(
                "addon {:?} has schema_version 0 - it must be >= 1",
                addon.id
            ));
        }

        let mut seen_outputs = std::collections::BTreeSet::new();
        for out in &addon.outputs {
            if !seen_outputs.insert(out.name.as_str()) {
                return Err(format!(
                    "addon {:?} declares duplicate output name {:?}",
                    addon.id, out.name
                ));
            }
        }

        for (field, text) in [
            ("config_schema", &addon.config_schema),
            ("desired_state_schema", &addon.desired_state_schema),
        ] {
            match serde_json::from_str::<serde_json::Value>(text) {
                Ok(serde_json::Value::Object(_)) => {}
                Ok(_) => {
                    return Err(format!(
                        "addon {:?} has a {field} that parses as JSON but is not a JSON \
                         object - a JSON Schema for a form must be an object",
                        addon.id
                    ));
                }
                Err(_) => {
                    return Err(format!(
                        "addon {:?} has a {field} that is not valid JSON",
                        addon.id
                    ));
                }
            }
        }
    }
    Ok(())
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

        // View ids namespace to `<extension_id>/<view_id>` on the host, so a
        // duplicate would make two different pages collide on one route.
        let mut seen_views = std::collections::BTreeSet::new();
        for view in &raw.contributions.views {
            if !seen_views.insert(view.id.as_str()) {
                return Err(format!(
                    "contributions.views[] declares duplicate id {:?}",
                    view.id
                ));
            }
        }

        // A view may only invoke tools this same extension contributes. A
        // dangling name would fail at the bridge, at runtime, in the browser —
        // the worst place to discover it.
        let tool_names: std::collections::BTreeSet<&str> = raw
            .contributions
            .tools
            .iter()
            .map(|t| t.name.as_str())
            .collect();
        for view in &raw.contributions.views {
            for wanted in &view.tools {
                if !tool_names.contains(wanted.as_str()) {
                    return Err(format!(
                        "view {:?} lists tool {:?}, which is not in contributions.tools",
                        view.id, wanted
                    ));
                }
            }
        }

        validate_addons(&raw.contributions.addons)?;

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
            required_secrets: raw.required_secrets,
            secret_requirements: raw.secret_requirements,
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
    /// WIT world the artifact exports, as a top-level runtime declaration.
    ///
    /// Only `kind: wasix:mcp/router` artifacts emit this (it names the router
    /// world); design extensions declare their worlds per component instead and
    /// leave this `None`. Optional so both shapes deserialize, since `Runtime`
    /// is `deny_unknown_fields`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world: Option<String>,
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
    #[serde(rename = "llmRoles", default, skip_serializing_if = "Vec::is_empty")]
    pub llm_roles: Vec<String>,
    /// OAuth provider ids (e.g. `"hubspot"`) this extension is allowed to request
    /// tokens for via the host `greentic:oauth-broker/broker-v1` import. The host
    /// rejects `get-token` for any provider not listed here; an empty list means
    /// the extension may not call the OAuth broker import at all.
    #[serde(
        rename = "oauthProviders",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub oauth_providers: Vec<String>,
    /// What a contributed `contributions.views[]` page may reach. Absent means
    /// the extension contributes no view, or contributes one that only renders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<UiPermissions>,
}

/// Grants that apply to browser-executed view code, not to the WASM guest.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiPermissions {
    /// Hosts a view may reach through the host's server-side proxy. The view
    /// never issues these itself: an iframe without `allow-same-origin` sends
    /// `Origin: null`, which most third-party APIs reject at CORS, and
    /// proxying keeps any credential on the server. Validated exactly like
    /// `permissions.network` — https only, loopback and link-local rejected.
    #[serde(rename = "fetchHosts", default, skip_serializing_if = "Vec::is_empty")]
    pub fetch_hosts: Vec<String>,
    /// Platform REST endpoints a view may call through the bridge. The host
    /// intersects this with the calling user's own RBAC, so the list can only
    /// ever narrow what that user could already do by hand.
    #[serde(rename = "platformApi", default, skip_serializing_if = "Vec::is_empty")]
    pub platform_api: Vec<ApiGrant>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiGrant {
    /// `GET`, `POST`, `PUT`, `PATCH` or `DELETE`. Constrained by the JSON
    /// Schema rather than by a Rust enum, so a describe naming a method this
    /// crate version does not know still round-trips instead of failing the
    /// whole parse.
    pub method: String,
    /// Path pattern, e.g. `/api/flows` or `/api/admin/tenants/*`.
    pub path_pattern: String,
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

#[cfg(test)]
mod engine_optional_tests {
    use super::*;

    fn minimal_describe_json(with_engine: bool) -> serde_json::Value {
        let mut doc = serde_json::json!({
            "apiVersion": "greentic.ai/v2",
            "kind": "DesignExtension",
            "compat": {
                "min_designer_version": ">=1.2.0",
                "min_runner_version": "^1.2.0",
                "contract_version": "1.2.0"
            },
            "metadata": {
                "id": "greentic.engine-test",
                "name": "Engine Test",
                "version": "0.1.0",
                "summary": "x",
                "author": { "name": "Greentic" },
                "license": "MIT"
            },
            "capabilities": { "offered": [], "required": [] },
            "runtime": {
                "memoryLimitMB": 32,
                "permissions": {},
                "components": {
                    "main": {
                        "oci_ref": "ghcr.io/greentic/engine-test:0.1.0",
                        "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
                        "world": "greentic:x/design-extension"
                    }
                }
            },
            "contributions": {}
        });
        if with_engine {
            doc["engine"] = serde_json::json!({
                "greenticDesigner": ">=1.2.0",
                "extRuntime": "^1.2.0"
            });
        }
        doc
    }

    #[test]
    fn describe_without_engine_parses() {
        let doc = minimal_describe_json(false);
        let parsed: DescribeJson = serde_json::from_value(doc).expect("must parse without engine");
        assert!(parsed.engine.is_none());
    }

    #[test]
    fn describe_with_engine_still_parses() {
        let doc = minimal_describe_json(true);
        let parsed: DescribeJson = serde_json::from_value(doc).expect("must parse with engine");
        assert!(parsed.engine.is_some());
    }

    #[test]
    fn permissions_parses_oauth_providers() {
        let json = r#"{ "network": [], "secrets": [], "oauthProviders": ["hubspot"] }"#;
        let p: Permissions = serde_json::from_str(json).unwrap();
        assert_eq!(p.oauth_providers, vec!["hubspot".to_string()]);
    }

    #[test]
    fn permissions_oauth_providers_defaults_empty_and_is_skipped_when_empty() {
        let json = r#"{ "network": [], "secrets": [] }"#;
        let p: Permissions = serde_json::from_str(json).unwrap();
        assert!(p.oauth_providers.is_empty());
        let out = serde_json::to_string(&p).unwrap();
        assert!(!out.contains("oauthProviders"), "got: {out}");
    }
}
