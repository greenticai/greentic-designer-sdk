//! Capability, permission and catalogue-metadata authoring for `gtdx new`.
//!
//! "Capability" is overloaded in this codebase, and the flags below deliberately
//! keep the five meanings apart rather than flattening them into one word:
//!
//! | What the author wants | Where it lands |
//! |---|---|
//! | contracts this extension provides / needs | `capabilities.offered[]` / `.required[]` |
//! | what the WASM guest may reach | `runtime.permissions.*` |
//! | how much memory it may use | `runtime.memoryLimitMB` |
//! | a UI page it contributes | `contributions.views[]` + `runtime.permissions.ui` |
//! | where a tool may be invoked from | `contributions.tools[].capabilities[]` |
//!
//! Everything here is validated at scaffold time against the *same* predicates
//! `gtdx lint` and `gtdx publish` apply later, so a project `gtdx new` accepts
//! is one that passes its own first `gtdx lint`. Where a rule already exists,
//! it is imported rather than restated — see [`crate::publish::validator`] and
//! [`crate::commands::lint`].
//!
//! Values are patched into the already-rendered `describe.json` rather than
//! shipped as template overlays, for the same reason `view_addon` does it:
//! an overlay replaces whole files, so every one of these fields would have to
//! be duplicated across all nine kind templates and would drift from all of
//! them.

use anyhow::{Result, bail};
use greentic_extension_sdk_contract::{
    AgenticWorkerMetadata, CapabilityId, CapabilityRef, ToolCapability,
    describe::ApiGrant,
    describe::contributions::{Surface, Visibility},
};
use semver::{Version, VersionReq};

use crate::commands::lint::{rules::looks_like_grant, rules_views};
use crate::publish::validator::{NETWORK_PATTERN_RULE, network_pattern_allowed};

/// HTTP methods `permissions.ui.platformApi[].method` accepts.
const API_METHODS: [&str; 5] = ["GET", "POST", "PUT", "PATCH", "DELETE"];

// ---------------------------------------------------------------------------
// clap value enums
// ---------------------------------------------------------------------------

/// `--view-surface`. Mirrors the contract's `Surface`; kept as its own type so
/// clap owns the CLI spelling and the contract owns the wire spelling.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum ViewSurfaceArg {
    #[default]
    Designer,
    Admin,
}

impl From<ViewSurfaceArg> for Surface {
    fn from(v: ViewSurfaceArg) -> Self {
        match v {
            ViewSurfaceArg::Designer => Self::Designer,
            ViewSurfaceArg::Admin => Self::Admin,
        }
    }
}

/// `--view-min-visibility`. The wire form is `snake_case`, so both spellings are
/// accepted on the command line and the underscore one is canonical.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum ViewVisibilityArg {
    #[default]
    Member,
    #[value(name = "tenant_admin", alias = "tenant-admin")]
    TenantAdmin,
    #[value(name = "platform_admin", alias = "platform-admin")]
    PlatformAdmin,
}

impl From<ViewVisibilityArg> for Visibility {
    fn from(v: ViewVisibilityArg) -> Self {
        match v {
            ViewVisibilityArg::Member => Self::Member,
            ViewVisibilityArg::TenantAdmin => Self::TenantAdmin,
            ViewVisibilityArg::PlatformAdmin => Self::PlatformAdmin,
        }
    }
}

/// `--tool-capability`: the runtime contexts a contributed tool may be invoked
/// from. Multiple values are meaningful — a tool can be both a flow node and an
/// agentic-worker tool.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, clap::ValueEnum)]
pub enum ToolSurfaceArg {
    Flow,
    #[value(name = "agentic_worker", alias = "agentic-worker")]
    AgenticWorker,
}

impl From<ToolSurfaceArg> for ToolCapability {
    fn from(v: ToolSurfaceArg) -> Self {
        match v {
            ToolSurfaceArg::Flow => Self::Flow,
            ToolSurfaceArg::AgenticWorker => Self::AgenticWorker,
        }
    }
}

// ---------------------------------------------------------------------------
// Raw inputs -> validated spec
// ---------------------------------------------------------------------------

/// The unvalidated capability inputs, in exactly the shape both the flag path
/// and the wizard produce. Having one struct means the two paths cannot drift
/// into validating different things.
#[derive(Debug, Clone, Default)]
pub(super) struct RawCapabilities {
    pub memory_mb: Option<u32>,
    pub network: Vec<String>,
    pub secrets: Vec<String>,
    pub call_extension_kinds: Vec<String>,
    pub llm_roles: Vec<String>,
    pub oauth_providers: Vec<String>,
    pub offered: Vec<String>,
    pub required: Vec<String>,
    pub tool_surfaces: Vec<ToolSurfaceArg>,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub repository: Option<String>,
    pub keywords: Vec<String>,
    pub with_view: bool,
    pub view_id: Option<String>,
    pub view_surface: ViewSurfaceArg,
    pub view_slot: Option<String>,
    pub view_title: Option<String>,
    pub view_min_visibility: ViewVisibilityArg,
    pub view_fetch_hosts: Vec<String>,
    pub view_apis: Vec<String>,
}

/// Catalogue-facing `metadata.*` overrides.
#[derive(Debug, Clone, Default)]
pub(super) struct CatalogueMetadata {
    pub summary: Option<String>,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub repository: Option<String>,
    pub keywords: Vec<String>,
}

impl CatalogueMetadata {
    fn is_empty(&self) -> bool {
        self.summary.is_none()
            && self.description.is_none()
            && self.homepage.is_none()
            && self.repository.is_none()
            && self.keywords.is_empty()
    }
}

/// The contributed view, fully resolved.
#[derive(Debug, Clone)]
pub(super) struct ViewSpec {
    pub id: String,
    pub surface: Surface,
    pub slot: String,
    pub title_fallback: String,
    pub min_visibility: Visibility,
    pub fetch_hosts: Vec<String>,
    pub platform_api: Vec<ApiGrant>,
}

/// Everything that is patched into `describe.json` after rendering, except the
/// view (which [`super::view_addon`] owns because it also drives the example
/// page's own placeholders).
#[derive(Debug, Clone, Default)]
pub(super) struct CapabilitySpec {
    pub memory_mb: Option<u32>,
    pub network: Vec<String>,
    pub secrets: Vec<String>,
    pub call_extension_kinds: Vec<String>,
    pub llm_roles: Vec<String>,
    pub oauth_providers: Vec<String>,
    pub offered: Vec<CapabilityRef>,
    pub required: Vec<CapabilityRef>,
    pub tool_surfaces: Vec<ToolSurfaceArg>,
    pub metadata: CatalogueMetadata,
}

impl CapabilitySpec {
    /// Whether this spec would change a rendered describe at all.
    ///
    /// `gtdx new` skips reading and rewriting `describe.json` entirely when it
    /// would not, so a scaffold nobody configured keeps the template's own
    /// bytes instead of a `serde_json` round-trip.
    pub(super) fn is_empty(&self) -> bool {
        self.memory_mb.is_none()
            && self.network.is_empty()
            && self.secrets.is_empty()
            && self.call_extension_kinds.is_empty()
            && self.llm_roles.is_empty()
            && self.oauth_providers.is_empty()
            && self.offered.is_empty()
            && self.required.is_empty()
            && self.tool_surfaces.is_empty()
            && self.metadata.is_empty()
    }
}

/// The validated result: what to write, plus advisory notes for the author.
#[derive(Debug)]
pub(super) struct Resolved {
    pub spec: CapabilitySpec,
    pub view: Option<ViewSpec>,
    /// Non-fatal remarks (unknown view slot, unrecognised extension kind).
    /// Warnings rather than errors for the same reason `W_VIEW_SLOT_UNKNOWN`
    /// is: the lists they check against are snapshots in this binary, and a
    /// host may have added an entry after it shipped.
    pub notes: Vec<String>,
}

/// Validate every capability input and resolve defaults.
///
/// # Errors
///
/// Returns the first violation, naming the flag and the offending value. Every
/// rule applied here is one `gtdx lint` or `gtdx publish` would apply later.
pub(super) fn resolve(raw: &RawCapabilities) -> Result<Resolved> {
    let mut notes = Vec::new();

    if let Some(mb) = raw.memory_mb
        && !(1..=1024).contains(&mb)
    {
        bail!("--memory-mb {mb} is out of range: runtime.memoryLimitMB must be 1..=1024");
    }

    for pattern in &raw.network {
        check_network_pattern("--permit-network", pattern)?;
    }
    for grant in &raw.secrets {
        check_secret_grant(grant)?;
    }
    for kind in &raw.call_extension_kinds {
        if let Some(note) = unknown_extension_kind_note(kind) {
            notes.push(note);
        }
    }

    let offered = parse_capability_refs("--offer-capability", &raw.offered, VersionForm::Exact)?;
    let required = parse_capability_refs(
        "--require-capability",
        &raw.required,
        VersionForm::Requirement,
    )?;
    check_no_capability_cycle(&offered, &required)?;

    let view = resolve_view(raw, &mut notes)?;

    Ok(Resolved {
        spec: CapabilitySpec {
            memory_mb: raw.memory_mb,
            network: raw.network.clone(),
            secrets: raw.secrets.clone(),
            call_extension_kinds: raw.call_extension_kinds.clone(),
            llm_roles: raw.llm_roles.clone(),
            oauth_providers: raw.oauth_providers.clone(),
            offered,
            required,
            tool_surfaces: dedup_tool_surfaces(&raw.tool_surfaces),
            metadata: CatalogueMetadata {
                summary: raw.summary.clone(),
                description: raw.description.clone(),
                homepage: raw.homepage.clone(),
                repository: raw.repository.clone(),
                keywords: raw.keywords.clone(),
            },
        },
        view,
        notes,
    })
}

/// The view flags are only meaningful alongside `--with-view`; silently
/// dropping them would scaffold a project that quietly ignores half of what the
/// author asked for.
fn resolve_view(raw: &RawCapabilities, notes: &mut Vec<String>) -> Result<Option<ViewSpec>> {
    if !raw.with_view {
        let orphan = [
            ("--view-id", raw.view_id.is_some()),
            ("--view-slot", raw.view_slot.is_some()),
            ("--view-title", raw.view_title.is_some()),
            ("--view-fetch-host", !raw.view_fetch_hosts.is_empty()),
            ("--view-api", !raw.view_apis.is_empty()),
        ]
        .into_iter()
        .find_map(|(flag, given)| given.then_some(flag));
        if let Some(flag) = orphan {
            bail!("{flag} needs --with-view: there is no contributed view to configure without it");
        }
        return Ok(None);
    }

    let id = raw.view_id.clone().unwrap_or_else(|| "hello".to_string());
    if !rules_views::is_valid_view_id(&id) {
        bail!("--view-id {id:?} must match ^[a-z0-9][a-z0-9._-]*$ (gtdx lint: E_VIEW_ID_PATTERN)");
    }

    let surface = Surface::from(raw.view_surface);
    let slot = raw
        .view_slot
        .clone()
        .unwrap_or_else(|| default_slot_for(raw.view_surface).to_string());
    if !rules_views::KNOWN_SLOTS.contains(&slot.as_str()) {
        notes.push(format!(
            "view slot {slot:?} is not one this gtdx knows ({}); \
             that is a warning, not an error — the list is a snapshot and hosts add slots \
             between releases (gtdx lint: W_VIEW_SLOT_UNKNOWN)",
            rules_views::KNOWN_SLOTS.join(", ")
        ));
    }

    for pattern in &raw.view_fetch_hosts {
        check_network_pattern("--view-fetch-host", pattern)?;
    }
    let platform_api = raw
        .view_apis
        .iter()
        .map(|raw| parse_api_grant(raw))
        .collect::<Result<Vec<_>>>()?;

    let title_fallback = raw.view_title.clone().unwrap_or_else(|| humanize(&id));

    Ok(Some(ViewSpec {
        id,
        surface,
        slot,
        title_fallback,
        min_visibility: Visibility::from(raw.view_min_visibility),
        fetch_hosts: raw.view_fetch_hosts.clone(),
        platform_api,
    }))
}

/// Turn a view id into the literal title shown when its `title_key` has no
/// translation: `usage-dashboard` becomes `Usage Dashboard`.
///
/// A derived default rather than a fixed `"Hello"`, which was only ever right
/// for the id it was named after — an author who set `--view-id usage` and left
/// the title alone got a page called "Hello". The default id still yields
/// exactly the title it always did.
fn humanize(id: &str) -> String {
    id.split(['-', '_', '.'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The slot a surface's view lands in when the author names none. Both are in
/// `KNOWN_SLOTS`, so an unconfigured view never trips the slot warning.
pub(super) fn default_slot_for(surface: ViewSurfaceArg) -> &'static str {
    match surface {
        ViewSurfaceArg::Designer => "designer.sidebar",
        ViewSurfaceArg::Admin => "admin.sidebar",
    }
}

fn check_network_pattern(flag: &str, pattern: &str) -> Result<()> {
    if network_pattern_allowed(pattern) {
        return Ok(());
    }
    bail!("{flag} {pattern:?} — {NETWORK_PATTERN_RULE}");
}

/// `runtime.permissions.secrets` holds read-permission *grants*, not credential
/// field names. Rejecting a plain key here is the same rule as
/// `E_PERMS_SECRETS_PLAIN_KEY`, applied before the project exists rather than
/// after the author's first lint.
fn check_secret_grant(grant: &str) -> Result<()> {
    if looks_like_grant(grant) {
        return Ok(());
    }
    bail!(
        "--permit-secret {grant:?} is a plain key, not a grant. Grants are `*`, a URI \
         containing `://` (e.g. `secret://acme/`), or a path prefix ending in `/` \
         (e.g. `acme/`). A credential field name an operator must supply belongs in \
         `requiredSecrets` instead — see docs/authoring-secrets.md \
         (gtdx lint: E_PERMS_SECRETS_PLAIN_KEY)"
    );
}

/// `callExtensionKinds` is an open string list in the schema, so an unknown
/// value is reported and kept rather than rejected.
fn unknown_extension_kind_note(kind: &str) -> Option<String> {
    use greentic_extension_sdk_contract::ExtensionKind;

    let known: Vec<&str> = ExtensionKind::ALL.iter().map(|k| k.wire_name()).collect();
    (!known.contains(&kind)).then(|| {
        format!(
            "--permit-call-kind {kind:?} is not a kind this gtdx knows ({}); \
             keeping it, since the field is an open list",
            known.join(", ")
        )
    })
}

/// Which semver form a `capabilities.*` entry must take.
#[derive(Copy, Clone, PartialEq, Eq)]
enum VersionForm {
    /// `capabilities.offered[].version` — `gtdx publish` requires an exact
    /// version, because it is the version other extensions resolve against.
    Exact,
    /// `capabilities.required[].version` — a semver requirement (`^1`, `>=1.2`).
    Requirement,
}

fn parse_capability_refs(
    flag: &str,
    raw: &[String],
    form: VersionForm,
) -> Result<Vec<CapabilityRef>> {
    raw.iter()
        .map(|entry| parse_capability_ref(flag, entry, form))
        .collect()
}

/// Parse `<namespace>:<path>@<version>`, e.g. `greentic:guardrail/topic@1.0.0`.
///
/// Split on the **last** `@` so a path that legitimately contains one is not
/// mangled, and validate both halves — a malformed version must never become a
/// silent `*` match-everything (the reason `CapabilityRef::version_req` fails
/// closed).
fn parse_capability_ref(flag: &str, entry: &str, form: VersionForm) -> Result<CapabilityRef> {
    let Some((id_raw, version)) = entry.rsplit_once('@') else {
        bail!(
            "{flag} {entry:?} is missing its version: expected <namespace>:<path>@<version>, \
             e.g. greentic:guardrail/topic@1.0.0"
        );
    };
    if version.is_empty() {
        bail!("{flag} {entry:?} has an empty version after `@`");
    }
    let id: CapabilityId = id_raw
        .parse()
        .map_err(|e| anyhow::anyhow!("{flag} {entry:?}: {e}"))?;

    match form {
        VersionForm::Exact => {
            Version::parse(version).map_err(|e| {
                anyhow::anyhow!(
                    "{flag} {entry:?}: {version:?} is not an exact semver ({e}). \
                     capabilities.offered pins the version other extensions resolve \
                     against, so `gtdx publish` rejects a requirement such as `^1` here"
                )
            })?;
        }
        VersionForm::Requirement => {
            VersionReq::parse(version).map_err(|e| {
                anyhow::anyhow!("{flag} {entry:?}: {version:?} is not a semver requirement ({e})")
            })?;
        }
    }

    Ok(CapabilityRef {
        id,
        version: version.to_string(),
        deprecated: None,
    })
}

/// An extension cannot depend on a capability it provides itself — the same
/// rule `gtdx lint` reports as `E_CAP_CYCLE`.
fn check_no_capability_cycle(offered: &[CapabilityRef], required: &[CapabilityRef]) -> Result<()> {
    for req in required {
        if offered.iter().any(|off| off.id == req.id) {
            bail!(
                "capability {} is both offered and required — an extension cannot depend on \
                 a capability it provides itself (gtdx lint: E_CAP_CYCLE)",
                req.id
            );
        }
    }
    Ok(())
}

/// Parse `"<METHOD> <path-pattern>"`, e.g. `GET /api/flows`.
fn parse_api_grant(raw: &str) -> Result<ApiGrant> {
    let mut parts = raw.split_whitespace();
    let (Some(method), Some(path)) = (parts.next(), parts.next()) else {
        bail!("--view-api {raw:?} must be \"<METHOD> <path-pattern>\", e.g. \"GET /api/flows\"");
    };
    if parts.next().is_some() {
        bail!("--view-api {raw:?} has trailing text after the path pattern");
    }
    let method = method.to_ascii_uppercase();
    if !API_METHODS.contains(&method.as_str()) {
        bail!(
            "--view-api {raw:?}: method {method:?} is not one of {}",
            API_METHODS.join(", ")
        );
    }
    if !path.starts_with('/') {
        bail!("--view-api {raw:?}: path pattern {path:?} must start with `/`");
    }
    Ok(ApiGrant {
        method,
        path_pattern: path.to_string(),
    })
}

/// Order-stable dedup: `--tool-capability flow --tool-capability flow` must not
/// emit `["flow", "flow"]`.
fn dedup_tool_surfaces(raw: &[ToolSurfaceArg]) -> Vec<ToolSurfaceArg> {
    let mut out: Vec<ToolSurfaceArg> = Vec::new();
    for surface in raw {
        if !out.contains(surface) {
            out.push(*surface);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Patching the rendered describe.json
// ---------------------------------------------------------------------------

/// Write `spec` into an already-rendered `describe.json`.
///
/// Emit policy: fields the kind template already writes (`permissions.network`,
/// `.secrets`, `.callExtensionKinds`, `capabilities.offered`, `.required`) keep
/// their template shape and are filled in place. Fields no template writes
/// (`llmRoles`, `oauthProviders`, `metadata.*`, `tools[].capabilities`) appear
/// only when the author asked for them, so an unconfigured scaffold is byte-for-
/// byte what it was before this flag surface existed.
///
/// # Errors
///
/// Fails when the rendered describe lacks a block the spec needs, or when the
/// author asked for a tool surface on a kind that contributes no tools.
pub(super) fn apply(describe: &mut serde_json::Value, spec: &CapabilitySpec) -> Result<()> {
    apply_metadata(describe, &spec.metadata)?;
    apply_capability_refs(describe, spec)?;
    apply_runtime(describe, spec)?;
    apply_tool_surfaces(describe, &spec.tool_surfaces)?;
    Ok(())
}

fn apply_metadata(describe: &mut serde_json::Value, meta: &CatalogueMetadata) -> Result<()> {
    if meta.is_empty() {
        return Ok(());
    }
    let obj = describe
        .get_mut("metadata")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_else(|| anyhow::anyhow!("rendered describe.json has no metadata object"))?;

    for (key, value) in [
        ("summary", meta.summary.as_ref()),
        ("description", meta.description.as_ref()),
        ("homepage", meta.homepage.as_ref()),
        ("repository", meta.repository.as_ref()),
    ] {
        if let Some(value) = value {
            obj.insert(key.to_string(), serde_json::Value::String(value.clone()));
        }
    }
    if !meta.keywords.is_empty() {
        obj.insert(
            "keywords".to_string(),
            serde_json::to_value(&meta.keywords)?,
        );
    }
    Ok(())
}

fn apply_capability_refs(describe: &mut serde_json::Value, spec: &CapabilitySpec) -> Result<()> {
    if spec.offered.is_empty() && spec.required.is_empty() {
        return Ok(());
    }
    let obj = describe
        .get_mut("capabilities")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_else(|| anyhow::anyhow!("rendered describe.json has no capabilities object"))?;
    if !spec.offered.is_empty() {
        obj.insert("offered".to_string(), serde_json::to_value(&spec.offered)?);
    }
    if !spec.required.is_empty() {
        obj.insert(
            "required".to_string(),
            serde_json::to_value(&spec.required)?,
        );
    }
    Ok(())
}

fn apply_runtime(describe: &mut serde_json::Value, spec: &CapabilitySpec) -> Result<()> {
    if let Some(mb) = spec.memory_mb {
        let runtime = describe
            .get_mut("runtime")
            .and_then(serde_json::Value::as_object_mut)
            .ok_or_else(|| anyhow::anyhow!("rendered describe.json has no runtime object"))?;
        runtime.insert("memoryLimitMB".to_string(), serde_json::Value::from(mb));
    }

    let touches_permissions = !spec.network.is_empty()
        || !spec.secrets.is_empty()
        || !spec.call_extension_kinds.is_empty()
        || !spec.llm_roles.is_empty()
        || !spec.oauth_providers.is_empty();
    if !touches_permissions {
        return Ok(());
    }

    let permissions = describe
        .pointer_mut("/runtime/permissions")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_else(|| anyhow::anyhow!("rendered describe.json has no runtime.permissions"))?;
    for (key, values) in [
        ("network", &spec.network),
        ("secrets", &spec.secrets),
        ("callExtensionKinds", &spec.call_extension_kinds),
        ("llmRoles", &spec.llm_roles),
        ("oauthProviders", &spec.oauth_providers),
    ] {
        if !values.is_empty() {
            merge_string_list(permissions, key, values);
        }
    }
    Ok(())
}

/// Append `values` to the string list at `key`, keeping what is already there.
///
/// Appending rather than replacing matters on the `--from-openapi` path, where
/// `runtime.permissions.network` is already populated from the spec's own
/// `servers` block: replacing it would silently drop every host the generator
/// derived, and the extension would fail at runtime against an allowlist that
/// looked deliberate. On a plain template the list is empty, so appending and
/// replacing agree.
fn merge_string_list(
    permissions: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    values: &[String],
) {
    let mut merged: Vec<String> = permissions
        .get(key)
        .and_then(serde_json::Value::as_array)
        .map(|existing| {
            existing
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    for value in values {
        if !merged.contains(value) {
            merged.push(value.clone());
        }
    }
    permissions.insert(
        key.to_string(),
        serde_json::Value::Array(merged.into_iter().map(serde_json::Value::String).collect()),
    );
}

/// Set `contributions.tools[].capabilities` on every contributed tool, and give
/// an `agentic_worker` tool the conservative metadata the planning layer assumes
/// when a tool declares that surface and ships none: `External` side effects and
/// `confirmation_required: true`, so a scaffold errs toward asking.
fn apply_tool_surfaces(
    describe: &mut serde_json::Value,
    surfaces: &[ToolSurfaceArg],
) -> Result<()> {
    if surfaces.is_empty() {
        return Ok(());
    }
    let tools = describe
        .pointer_mut("/contributions/tools")
        .and_then(serde_json::Value::as_array_mut)
        .filter(|tools| !tools.is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "--tool-capability needs a kind that contributes tools, and this one \
                 contributes none. `contributions.tools[]` is where the surface is \
                 declared, so there is nothing to attach it to"
            )
        })?;

    let wire: Vec<&str> = surfaces
        .iter()
        .map(|s| ToolCapability::from(*s).as_wire_str())
        .collect();
    let agentic = surfaces.contains(&ToolSurfaceArg::AgenticWorker);
    let metadata = agentic
        .then(|| {
            AgenticWorkerMetadata::default()
                .with_conservative_defaults()
                .encode()
        })
        .transpose()?;

    for tool in tools.iter_mut() {
        let obj = tool
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("contributions.tools[] entry is not an object"))?;
        obj.insert("capabilities".to_string(), serde_json::to_value(&wire)?);
        if let Some(metadata) = &metadata {
            // Never clobber metadata a template already authored: the flag
            // declares the surface, and conservative defaults are a floor for a
            // tool that has none, not an override of one that does.
            obj.entry("agentic_worker_metadata")
                .or_insert_with(|| serde_json::Value::String(metadata.clone()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw() -> RawCapabilities {
        RawCapabilities::default()
    }

    // --- memory ---

    #[test]
    fn memory_outside_the_schema_bound_is_rejected() {
        for mb in [0u32, 1025, 4096] {
            let err = resolve(&RawCapabilities {
                memory_mb: Some(mb),
                ..raw()
            })
            .expect_err("out of range");
            assert!(err.to_string().contains("1..=1024"), "{err}");
        }
        for mb in [1u32, 64, 1024] {
            assert!(
                resolve(&RawCapabilities {
                    memory_mb: Some(mb),
                    ..raw()
                })
                .is_ok(),
                "{mb} should be accepted"
            );
        }
    }

    // --- network ---

    #[test]
    fn network_accepts_https_and_loopback_http_only() {
        for pattern in [
            "https://api.example.com/*",
            "http://127.0.0.1:8787/*",
            "http://localhost/*",
            "http://[::1]:8787/*",
        ] {
            assert!(
                resolve(&RawCapabilities {
                    network: vec![pattern.to_string()],
                    ..raw()
                })
                .is_ok(),
                "{pattern} should be accepted"
            );
        }
        for pattern in ["http://api.example.com/*", "http://127.0.0.1.evil.com/*"] {
            let err = resolve(&RawCapabilities {
                network: vec![pattern.to_string()],
                ..raw()
            })
            .expect_err("cleartext to a public host");
            assert!(err.to_string().contains("--permit-network"), "{err}");
            assert!(err.to_string().contains("https://"), "{err}");
        }
    }

    /// A view's proxied fetch hosts follow the same address rule as the guest's
    /// own network permission, and must say which flag was wrong.
    #[test]
    fn view_fetch_hosts_use_the_same_address_rule() {
        let err = resolve(&RawCapabilities {
            with_view: true,
            view_fetch_hosts: vec!["http://api.example.com/*".to_string()],
            ..raw()
        })
        .expect_err("cleartext to a public host");
        assert!(err.to_string().contains("--view-fetch-host"), "{err}");
    }

    // --- secrets ---

    #[test]
    fn secret_grants_must_be_grants_not_plain_keys() {
        for grant in ["*", "secret://acme/", "acme/"] {
            assert!(
                resolve(&RawCapabilities {
                    secrets: vec![grant.to_string()],
                    ..raw()
                })
                .is_ok(),
                "{grant} should be accepted"
            );
        }
        let err = resolve(&RawCapabilities {
            secrets: vec!["SLACK_BOT_TOKEN".to_string()],
            ..raw()
        })
        .expect_err("plain key");
        assert!(err.to_string().contains("requiredSecrets"), "{err}");
        assert!(
            err.to_string().contains("E_PERMS_SECRETS_PLAIN_KEY"),
            "should name the lint it pre-empts: {err}"
        );
    }

    // --- capability refs ---

    #[test]
    fn offered_requires_an_exact_version_and_required_accepts_a_range() {
        let resolved = resolve(&RawCapabilities {
            offered: vec!["greentic:guardrail/topic@1.0.0".to_string()],
            required: vec!["greentic:llm/chat@^1.2".to_string()],
            ..raw()
        })
        .expect("valid refs");
        assert_eq!(
            resolved.spec.offered[0].id.as_str(),
            "greentic:guardrail/topic"
        );
        assert_eq!(resolved.spec.offered[0].version, "1.0.0");
        assert_eq!(resolved.spec.required[0].version, "^1.2");

        let err = resolve(&RawCapabilities {
            offered: vec!["greentic:guardrail/topic@^1".to_string()],
            ..raw()
        })
        .expect_err("a requirement is not an exact version");
        assert!(err.to_string().contains("exact semver"), "{err}");
    }

    #[test]
    fn a_capability_ref_without_a_version_is_rejected() {
        let err = resolve(&RawCapabilities {
            required: vec!["greentic:llm/chat".to_string()],
            ..raw()
        })
        .expect_err("no version");
        assert!(err.to_string().contains("missing its version"), "{err}");
    }

    #[test]
    fn a_capability_id_without_a_namespace_is_rejected() {
        let err = resolve(&RawCapabilities {
            required: vec!["chat@^1".to_string()],
            ..raw()
        })
        .expect_err("no namespace");
        assert!(err.to_string().contains("chat@^1"), "{err}");
    }

    /// A path containing `@` must survive: the split is on the last `@`.
    #[test]
    fn the_version_split_takes_the_last_at_sign() {
        let resolved = resolve(&RawCapabilities {
            required: vec!["greentic:scope/a@b@^1".to_string()],
            ..raw()
        })
        .expect("valid ref");
        assert_eq!(resolved.spec.required[0].id.as_str(), "greentic:scope/a@b");
        assert_eq!(resolved.spec.required[0].version, "^1");
    }

    #[test]
    fn offering_and_requiring_the_same_capability_is_a_cycle() {
        let err = resolve(&RawCapabilities {
            offered: vec!["greentic:guardrail/topic@1.0.0".to_string()],
            required: vec!["greentic:guardrail/topic@^1".to_string()],
            ..raw()
        })
        .expect_err("self-cycle");
        assert!(err.to_string().contains("E_CAP_CYCLE"), "{err}");
    }

    // --- views ---

    #[test]
    fn view_defaults_follow_the_chosen_surface() {
        let designer = resolve(&RawCapabilities {
            with_view: true,
            ..raw()
        })
        .expect("valid")
        .view
        .expect("a view");
        assert_eq!(designer.id, "hello");
        assert_eq!(designer.slot, "designer.sidebar");

        let admin = resolve(&RawCapabilities {
            with_view: true,
            view_surface: ViewSurfaceArg::Admin,
            ..raw()
        })
        .expect("valid")
        .view
        .expect("a view");
        assert_eq!(admin.slot, "admin.sidebar");
    }

    /// Both defaults must be slots this binary already knows, or every
    /// unconfigured `--with-view` scaffold would emit a slot warning.
    #[test]
    fn default_slots_are_known_slots() {
        for surface in [ViewSurfaceArg::Designer, ViewSurfaceArg::Admin] {
            assert!(
                rules_views::KNOWN_SLOTS.contains(&default_slot_for(surface)),
                "{surface:?} default slot is not a known slot"
            );
        }
    }

    #[test]
    fn the_view_title_defaults_to_a_humanised_id() {
        for (id, expected) in [
            ("hello", "Hello"),
            ("usage", "Usage"),
            ("usage-dashboard", "Usage Dashboard"),
            ("tenant.usage_report", "Tenant Usage Report"),
        ] {
            let view = resolve(&RawCapabilities {
                with_view: true,
                view_id: Some(id.to_string()),
                ..raw()
            })
            .expect("valid")
            .view
            .expect("a view");
            assert_eq!(view.title_fallback, expected, "id {id}");
        }
    }

    #[test]
    fn an_explicit_view_title_wins_over_the_derived_one() {
        let view = resolve(&RawCapabilities {
            with_view: true,
            view_id: Some("usage".to_string()),
            view_title: Some("Acme Usage".to_string()),
            ..raw()
        })
        .expect("valid")
        .view
        .expect("a view");
        assert_eq!(view.title_fallback, "Acme Usage");
    }

    #[test]
    fn an_unknown_slot_is_a_note_not_an_error() {
        let resolved = resolve(&RawCapabilities {
            with_view: true,
            view_slot: Some("admin.somethingNew".to_string()),
            ..raw()
        })
        .expect("an unknown slot must not fail the scaffold");
        assert_eq!(resolved.view.expect("a view").slot, "admin.somethingNew");
        assert!(
            resolved
                .notes
                .iter()
                .any(|n| n.contains("W_VIEW_SLOT_UNKNOWN")),
            "{:?}",
            resolved.notes
        );
    }

    #[test]
    fn a_view_id_that_could_escape_its_directory_is_rejected() {
        let err = resolve(&RawCapabilities {
            with_view: true,
            view_id: Some("../..".to_string()),
            ..raw()
        })
        .expect_err("path traversal");
        assert!(err.to_string().contains("E_VIEW_ID_PATTERN"), "{err}");
    }

    /// Accepting `--view-id` while ignoring it would scaffold a project that
    /// silently drops half of what the author asked for.
    #[test]
    fn view_flags_without_with_view_are_rejected() {
        for raw_args in [
            RawCapabilities {
                view_id: Some("usage".to_string()),
                ..raw()
            },
            RawCapabilities {
                view_apis: vec!["GET /api/flows".to_string()],
                ..raw()
            },
        ] {
            let err = resolve(&raw_args).expect_err("orphan view flag");
            assert!(err.to_string().contains("--with-view"), "{err}");
        }
    }

    // --- api grants ---

    #[test]
    fn api_grants_parse_method_and_path() {
        let resolved = resolve(&RawCapabilities {
            with_view: true,
            view_apis: vec!["get /api/flows".to_string()],
            ..raw()
        })
        .expect("valid grant");
        let grants = resolved.view.expect("a view").platform_api;
        assert_eq!(grants[0].method, "GET", "method should be normalised");
        assert_eq!(grants[0].path_pattern, "/api/flows");
    }

    #[test]
    fn malformed_api_grants_are_rejected() {
        for entry in [
            "/api/flows",
            "FETCH /api/flows",
            "GET api/flows",
            "GET /a /b",
        ] {
            let err = resolve(&RawCapabilities {
                with_view: true,
                view_apis: vec![entry.to_string()],
                ..raw()
            })
            .expect_err("malformed grant");
            assert!(err.to_string().contains("--view-api"), "{entry}: {err}");
        }
    }

    // --- call kinds ---

    #[test]
    fn an_unknown_call_kind_is_a_note_not_an_error() {
        let resolved = resolve(&RawCapabilities {
            call_extension_kinds: vec!["NotAKind".to_string()],
            ..raw()
        })
        .expect("open list, so not an error");
        assert!(
            resolved.notes.iter().any(|n| n.contains("NotAKind")),
            "{:?}",
            resolved.notes
        );
    }

    /// The known-kind list is derived from `ExtensionKind::ALL`, so a kind
    /// added to the contract must not start warning here.
    #[test]
    fn every_contract_kind_is_accepted_without_a_note() {
        use greentic_extension_sdk_contract::ExtensionKind;

        for kind in ExtensionKind::ALL {
            assert!(
                unknown_extension_kind_note(kind.wire_name()).is_none(),
                "{} should be known",
                kind.wire_name()
            );
        }
    }

    // --- tool surfaces ---

    #[test]
    fn tool_surfaces_are_deduped_in_order() {
        let resolved = resolve(&RawCapabilities {
            tool_surfaces: vec![
                ToolSurfaceArg::AgenticWorker,
                ToolSurfaceArg::Flow,
                ToolSurfaceArg::AgenticWorker,
            ],
            ..raw()
        })
        .expect("valid");
        assert_eq!(
            resolved.spec.tool_surfaces,
            vec![ToolSurfaceArg::AgenticWorker, ToolSurfaceArg::Flow]
        );
    }

    // --- apply ---

    fn rendered() -> serde_json::Value {
        serde_json::json!({
            "metadata": { "summary": "A Greentic Designer design extension." },
            "capabilities": { "offered": [], "required": [] },
            "runtime": {
                "permissions": { "network": [], "secrets": [], "callExtensionKinds": [] },
                "components": {}
            },
            "contributions": { "tools": [{ "name": "echo" }] }
        })
    }

    #[test]
    fn an_empty_spec_changes_nothing() {
        let mut describe = rendered();
        let before = describe.clone();
        apply(&mut describe, &CapabilitySpec::default()).expect("apply");
        assert_eq!(
            describe, before,
            "an unconfigured scaffold must be untouched"
        );
    }

    #[test]
    fn apply_writes_every_block_it_was_given() {
        let resolved = resolve(&RawCapabilities {
            memory_mb: Some(128),
            network: vec!["https://api.acme.com/*".to_string()],
            secrets: vec!["acme/".to_string()],
            call_extension_kinds: vec!["ProviderExtension".to_string()],
            llm_roles: vec!["sorla_composer".to_string()],
            oauth_providers: vec!["hubspot".to_string()],
            offered: vec!["greentic:guardrail/topic@1.0.0".to_string()],
            required: vec!["greentic:llm/chat@^1".to_string()],
            summary: Some("Acme connector.".to_string()),
            description: Some("Long form.".to_string()),
            homepage: Some("https://acme.example".to_string()),
            repository: Some("https://github.com/acme/ext".to_string()),
            keywords: vec!["acme".to_string(), "crm".to_string()],
            ..raw()
        })
        .expect("valid");

        let mut describe = rendered();
        apply(&mut describe, &resolved.spec).expect("apply");

        assert_eq!(describe["runtime"]["memoryLimitMB"], 128);
        assert_eq!(
            describe["runtime"]["permissions"]["network"][0],
            "https://api.acme.com/*"
        );
        assert_eq!(describe["runtime"]["permissions"]["secrets"][0], "acme/");
        assert_eq!(
            describe["runtime"]["permissions"]["callExtensionKinds"][0],
            "ProviderExtension"
        );
        assert_eq!(
            describe["runtime"]["permissions"]["llmRoles"][0],
            "sorla_composer"
        );
        assert_eq!(
            describe["runtime"]["permissions"]["oauthProviders"][0],
            "hubspot"
        );
        assert_eq!(
            describe["capabilities"]["offered"][0]["id"],
            "greentic:guardrail/topic"
        );
        assert_eq!(describe["capabilities"]["offered"][0]["version"], "1.0.0");
        assert_eq!(describe["capabilities"]["required"][0]["version"], "^1");
        assert_eq!(describe["metadata"]["summary"], "Acme connector.");
        assert_eq!(describe["metadata"]["description"], "Long form.");
        assert_eq!(describe["metadata"]["homepage"], "https://acme.example");
        assert_eq!(
            describe["metadata"]["repository"],
            "https://github.com/acme/ext"
        );
        assert_eq!(describe["metadata"]["keywords"][1], "crm");
    }

    /// A `CapabilityRef` must serialize with the contract's own field names —
    /// building the JSON by hand here is how `deprecated` or a renamed field
    /// would silently drift out of the scaffold.
    #[test]
    fn capability_refs_serialize_without_a_null_deprecated_field() {
        let resolved = resolve(&RawCapabilities {
            offered: vec!["greentic:guardrail/topic@1.0.0".to_string()],
            ..raw()
        })
        .expect("valid");
        let mut describe = rendered();
        apply(&mut describe, &resolved.spec).expect("apply");
        let entry = describe["capabilities"]["offered"][0]
            .as_object()
            .expect("object");
        assert!(
            !entry.contains_key("deprecated"),
            "an absent `deprecated` must be skipped, not written as null: {entry:?}"
        );
    }

    #[test]
    fn tool_surfaces_land_on_every_contributed_tool() {
        let resolved = resolve(&RawCapabilities {
            tool_surfaces: vec![ToolSurfaceArg::Flow, ToolSurfaceArg::AgenticWorker],
            ..raw()
        })
        .expect("valid");
        let mut describe = rendered();
        apply(&mut describe, &resolved.spec).expect("apply");

        let tool = &describe["contributions"]["tools"][0];
        assert_eq!(tool["capabilities"][0], "flow");
        assert_eq!(tool["capabilities"][1], "agentic_worker");

        let meta: AgenticWorkerMetadata = AgenticWorkerMetadata::decode(
            tool["agentic_worker_metadata"].as_str().expect("string"),
        )
        .expect("decodes");
        assert_eq!(meta.confirmation_required, Some(true));
        assert_eq!(
            meta.side_effects,
            Some(greentic_extension_sdk_contract::SideEffects::External)
        );
    }

    /// `flow` alone is not an agentic-worker tool, so it gets no metadata.
    #[test]
    fn flow_only_tools_get_no_agentic_worker_metadata() {
        let resolved = resolve(&RawCapabilities {
            tool_surfaces: vec![ToolSurfaceArg::Flow],
            ..raw()
        })
        .expect("valid");
        let mut describe = rendered();
        apply(&mut describe, &resolved.spec).expect("apply");
        assert!(
            describe["contributions"]["tools"][0]
                .get("agentic_worker_metadata")
                .is_none()
        );
    }

    #[test]
    fn existing_tool_metadata_is_never_clobbered() {
        let resolved = resolve(&RawCapabilities {
            tool_surfaces: vec![ToolSurfaceArg::AgenticWorker],
            ..raw()
        })
        .expect("valid");
        let mut describe = rendered();
        describe["contributions"]["tools"][0]["agentic_worker_metadata"] =
            serde_json::Value::String(r#"{"cost":"low"}"#.to_string());
        apply(&mut describe, &resolved.spec).expect("apply");
        assert_eq!(
            describe["contributions"]["tools"][0]["agentic_worker_metadata"],
            r#"{"cost":"low"}"#
        );
    }

    /// A kind contributing no tools has nowhere to record the surface; saying so
    /// beats writing the flag into a describe where nothing reads it.
    #[test]
    fn tool_surfaces_on_a_toolless_kind_are_an_error() {
        let resolved = resolve(&RawCapabilities {
            tool_surfaces: vec![ToolSurfaceArg::Flow],
            ..raw()
        })
        .expect("valid");
        let mut describe = rendered();
        describe["contributions"] = serde_json::json!({});
        let err = apply(&mut describe, &resolved.spec).expect_err("no tools");
        assert!(err.to_string().contains("contributes none"), "{err}");
    }
}
