//! Interactive `gtdx new` wizard.
//!
//! Guides the user through the same inputs the flag-driven path accepts, so a
//! new extension can be scaffolded without memorising the full command. Any
//! flags already supplied on the command line are used as prompt defaults.

use std::path::{Path, PathBuf};

use dialoguer::{Confirm, Input, MultiSelect, Select};

use greentic_extension_sdk_contract::extension_id::validate_extension_id;

use super::capabilities::{
    self, RawCapabilities, ToolSurfaceArg, ViewSurfaceArg, ViewVisibilityArg,
};
use super::{Args, Resolved, detect_git_author};
use crate::scaffold::{Kind, template};

/// Extension kinds offered in the picker, each with a one-line description.
/// Ordered most-common-first; `mcp` is surfaced near the top because it is the
/// flow-capable `wasix:mcp/router` style most new integrations want.
const KIND_CHOICES: &[(Kind, &str, &str)] = &[
    (Kind::Design, "design", "Designer/agentic tools (default)"),
    (
        Kind::Mcp,
        "mcp",
        "MCP router (wasix:mcp/router) — usable as flow node + agentic tool",
    ),
    (Kind::Provider, "provider", "Messaging/event provider"),
    (
        Kind::WasmComponent,
        "wasm-component",
        "Generic WASM flow component",
    ),
    (Kind::Llm, "llm", "LLM provider extension"),
    (Kind::Bundle, "bundle", "Bundle extension"),
    (Kind::Deploy, "deploy", "Deploy target extension"),
];

/// Run the interactive wizard, falling back to `args` values as defaults.
pub(super) fn run(args: &Args) -> anyhow::Result<Resolved> {
    println!("gtdx new — interactive wizard (press Enter to accept defaults)\n");

    let name = prompt_name(args)?;
    let kind = prompt_kind(args)?;
    // If the user chose MCP and hasn't already provided --from-openapi, offer
    // the OpenAPI seed prompt.
    let from_openapi = prompt_openapi_seed(args, kind)?;
    let id = prompt_id(args, &name)?;
    let version = prompt_version(args)?;
    let author = prompt_author(args)?;
    let license = prompt_license(args)?;
    let (raw, icon) = prompt_capabilities(args, kind)?;

    // Validated before the confirmation prompt, so a mistyped grant is caught
    // while the author still has the context to fix it — not after they have
    // said yes and watched the scaffold fail.
    let caps = capabilities::resolve(&raw)?;

    print_summary(
        &name,
        kind,
        &id,
        &version,
        &author,
        &license,
        &raw,
        icon.as_deref(),
    );
    for note in &caps.notes {
        println!("  ! {note}");
    }
    let confirmed = Confirm::new()
        .with_prompt("Create this extension?")
        .default(true)
        .interact()?;
    if !confirmed {
        anyhow::bail!("cancelled by user");
    }

    Ok(Resolved {
        name,
        kind,
        id,
        version,
        author,
        license,
        no_git: args.no_git,
        dir: args.dir.clone(),
        force: args.force,
        node_type_id: args.node_type_id.clone(),
        label: args.label.clone(),
        component_ref: args.component_ref.clone(),
        from_openapi,
        icon,
        capabilities: caps.spec,
        view: caps.view,
        capability_notes: caps.notes,
    })
}

// ---------------------------------------------------------------------------
// Capability step
// ---------------------------------------------------------------------------

/// One row of the capability picker.
///
/// An enum with an `ALL` slice rather than a literal list of prompt strings, so
/// the picker and the tests share one definition: a row added here and
/// forgotten in `run` fails `every_row_is_offered_and_handled` instead of
/// becoming a capability the wizard silently cannot reach — which is exactly
/// how `--icon` and `--with-view` ended up unreachable before this existed.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(super) enum Row {
    Network,
    Secrets,
    CallKinds,
    LlmRoles,
    OauthProviders,
    Memory,
    Offered,
    Required,
    View,
    ToolSurfaces,
    Icon,
    Catalogue,
}

impl Row {
    pub(super) const ALL: &'static [Self] = &[
        Self::Network,
        Self::Secrets,
        Self::CallKinds,
        Self::LlmRoles,
        Self::OauthProviders,
        Self::Memory,
        Self::Offered,
        Self::Required,
        Self::View,
        Self::ToolSurfaces,
        Self::Icon,
        Self::Catalogue,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Network => "Network access",
            Self::Secrets => "Secrets",
            Self::CallKinds => "Call other extensions",
            Self::LlmRoles => "LLM roles",
            Self::OauthProviders => "OAuth providers",
            Self::Memory => "Memory limit",
            Self::Offered => "Offered capabilities",
            Self::Required => "Required capabilities",
            Self::View => "Contributed view",
            Self::ToolSurfaces => "Tool surfaces",
            Self::Icon => "Icon",
            Self::Catalogue => "Catalogue metadata",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Network => "hosts the guest may fetch (runtime.permissions.network)",
            Self::Secrets => "secret read grants (runtime.permissions.secrets)",
            Self::CallKinds => "kinds it may call into (callExtensionKinds)",
            Self::LlmRoles => "LLM roles it may request from the host",
            Self::OauthProviders => "OAuth providers it may request tokens for",
            Self::Memory => "runtime.memoryLimitMB — 1..=1024, default 64",
            Self::Offered => "capability contracts it provides to others",
            Self::Required => "capability contracts it needs from others",
            Self::View => "a UI page, its placement and what it may reach",
            Self::ToolSurfaces => "show tools in flows, the agentic worker, or both",
            Self::Icon => "metadata.icon — svg/png/jpg/webp, <= 1 MiB",
            Self::Catalogue => "summary, description, homepage, repository, keywords",
        }
    }

    /// Whether the row applies to `kind` at all. Offering a choice that the
    /// scaffold would then refuse is worse than not offering it.
    fn applies_to(self, kind: Kind) -> bool {
        match self {
            // `wasix:mcp/router` artifacts carry no contributions block for a
            // view to attach to — the same rule `validate_with_view` enforces.
            Self::View => kind != Kind::Mcp,
            // A surface is declared on `contributions.tools[]`; a kind that
            // contributes none has nowhere to record it.
            Self::ToolSurfaces => template::kind_contributes_tools(kind.as_str()),
            _ => true,
        }
    }

    /// Whether the command line already supplied something for this row, which
    /// pre-checks it in the picker.
    fn preselected(self, args: &Args) -> bool {
        match self {
            Self::Network => !args.permit_network.is_empty(),
            Self::Secrets => !args.permit_secret.is_empty(),
            Self::CallKinds => !args.permit_call_kind.is_empty(),
            Self::LlmRoles => !args.permit_llm_role.is_empty(),
            Self::OauthProviders => !args.permit_oauth.is_empty(),
            Self::Memory => args.memory_mb.is_some(),
            Self::Offered => !args.offer_capability.is_empty(),
            Self::Required => !args.require_capability.is_empty(),
            Self::View => args.with_view,
            Self::ToolSurfaces => !args.tool_capability.is_empty(),
            Self::Icon => args.icon.is_some(),
            Self::Catalogue => {
                args.summary.is_some()
                    || args.description.is_some()
                    || args.homepage.is_some()
                    || args.repository.is_some()
                    || !args.keyword.is_empty()
            }
        }
    }
}

/// Ask which capabilities the extension uses, then drill into each chosen one.
///
/// A multi-select gate rather than a linear run of prompts: an author who needs
/// none of this answers one question, and an author who needs three answers
/// only those three.
fn prompt_capabilities(
    args: &Args,
    kind: Kind,
) -> anyhow::Result<(RawCapabilities, Option<PathBuf>)> {
    let mut raw = args.raw_capabilities();
    let mut icon = args.icon.clone();

    let rows: Vec<Row> = Row::ALL
        .iter()
        .copied()
        .filter(|row| row.applies_to(kind))
        .collect();
    let labels: Vec<String> = rows
        .iter()
        .map(|row| format!("{:<22} {}", row.label(), row.description()))
        .collect();
    let defaults: Vec<bool> = rows.iter().map(|row| row.preselected(args)).collect();

    println!();
    println!("Capabilities — all optional. Every one of these can also be added later");
    println!("by editing describe.json, so choosing none here is the normal answer.");
    println!("Space toggles a row, Enter continues.\n");
    let chosen = MultiSelect::new()
        .with_prompt("Capabilities to configure now")
        // dialoguer would otherwise echo every selected item back on one line;
        // with twelve rows carrying descriptions that is an unreadable wall,
        // and the "About to scaffold" summary already reports the choices.
        .report(false)
        .items(&labels)
        .defaults(&defaults)
        .interact()?;
    let chosen: Vec<Row> = chosen.into_iter().map(|i| rows[i]).collect();
    if chosen.is_empty() {
        println!("  (none selected)");
    }

    clear_unchecked(&mut raw, &mut icon, &chosen);
    for row in chosen {
        drill_into(row, &mut raw, &mut icon)?;
    }

    Ok((raw, icon))
}

/// Apply the picker's verdict to rows the author left unchecked.
///
/// Unchecking clears whatever the command line put there: the picker is the
/// author's final say, so unchecking `Network` after passing `--permit-network`
/// must actually drop it rather than silently keep it.
fn clear_unchecked(raw: &mut RawCapabilities, icon: &mut Option<PathBuf>, chosen: &[Row]) {
    raw.with_view = false;
    if !chosen.contains(&Row::Network) {
        raw.network.clear();
    }
    if !chosen.contains(&Row::Secrets) {
        raw.secrets.clear();
    }
    if !chosen.contains(&Row::CallKinds) {
        raw.call_extension_kinds.clear();
    }
    if !chosen.contains(&Row::LlmRoles) {
        raw.llm_roles.clear();
    }
    if !chosen.contains(&Row::OauthProviders) {
        raw.oauth_providers.clear();
    }
    if !chosen.contains(&Row::Memory) {
        raw.memory_mb = None;
    }
    if !chosen.contains(&Row::Offered) {
        raw.offered.clear();
    }
    if !chosen.contains(&Row::Required) {
        raw.required.clear();
    }
    if !chosen.contains(&Row::ToolSurfaces) {
        raw.tool_surfaces.clear();
    }
    if !chosen.contains(&Row::Icon) {
        *icon = None;
    }
}

/// Ask for the detail behind one checked row.
fn drill_into(
    row: Row,
    raw: &mut RawCapabilities,
    icon: &mut Option<PathBuf>,
) -> anyhow::Result<()> {
    match row {
        Row::Network => {
            raw.network = prompt_list(
                "Network host pattern",
                "https://api.acme.com/*",
                &raw.network,
            )?;
        }
        Row::Secrets => {
            raw.secrets = prompt_list("Secret grant", "secret://acme/", &raw.secrets)?;
        }
        Row::CallKinds => {
            raw.call_extension_kinds = prompt_list(
                "Extension kind to call",
                "ProviderExtension",
                &raw.call_extension_kinds,
            )?;
        }
        Row::LlmRoles => {
            raw.llm_roles = prompt_list("LLM role", "sorla_composer", &raw.llm_roles)?;
        }
        Row::OauthProviders => {
            raw.oauth_providers =
                prompt_list("OAuth provider id", "hubspot", &raw.oauth_providers)?;
        }
        Row::Memory => {
            raw.memory_mb = Some(
                Input::<u32>::new()
                    .with_prompt("Memory limit (MB)")
                    .default(raw.memory_mb.unwrap_or(64))
                    .validate_with(validate_memory_input)
                    .interact_text()?,
            );
        }
        Row::Offered => {
            raw.offered = prompt_list(
                "Offered capability",
                "greentic:guardrail/topic@1.0.0",
                &raw.offered,
            )?;
        }
        Row::Required => {
            raw.required =
                prompt_list("Required capability", "greentic:llm/chat@^1", &raw.required)?;
        }
        Row::View => prompt_view(raw)?,
        Row::ToolSurfaces => raw.tool_surfaces = prompt_tool_surfaces(&raw.tool_surfaces)?,
        Row::Icon => *icon = Some(prompt_icon(icon.as_deref())?),
        Row::Catalogue => prompt_catalogue(raw)?,
    }
    Ok(())
}

fn prompt_view(raw: &mut RawCapabilities) -> anyhow::Result<()> {
    raw.with_view = true;

    let surfaces = [ViewSurfaceArg::Designer, ViewSurfaceArg::Admin];
    let surface_index = Select::new()
        .with_prompt("View surface")
        .items(&["designer", "admin"])
        .default(
            surfaces
                .iter()
                .position(|s| *s == raw.view_surface)
                .unwrap_or(0),
        )
        .interact()?;
    raw.view_surface = surfaces[surface_index];

    raw.view_id = Some(
        Input::<String>::new()
            .with_prompt("View id")
            .default(raw.view_id.clone().unwrap_or_else(|| "hello".to_string()))
            .interact_text()?
            .trim()
            .to_string(),
    );
    raw.view_title = Some(
        Input::<String>::new()
            .with_prompt("View title")
            .default(
                raw.view_title
                    .clone()
                    .unwrap_or_else(|| "Hello".to_string()),
            )
            .interact_text()?
            .trim()
            .to_string(),
    );
    raw.view_slot =
        Some(
            Input::<String>::new()
                .with_prompt("Placement slot")
                .default(raw.view_slot.clone().unwrap_or_else(|| {
                    capabilities::default_slot_for(raw.view_surface).to_string()
                }))
                .interact_text()?
                .trim()
                .to_string(),
        );

    let visibilities = [
        ViewVisibilityArg::Member,
        ViewVisibilityArg::TenantAdmin,
        ViewVisibilityArg::PlatformAdmin,
    ];
    let visibility_index = Select::new()
        .with_prompt("Minimum visibility")
        .items(&["member", "tenant_admin", "platform_admin"])
        .default(
            visibilities
                .iter()
                .position(|v| *v == raw.view_min_visibility)
                .unwrap_or(0),
        )
        .interact()?;
    raw.view_min_visibility = visibilities[visibility_index];

    raw.view_fetch_hosts = prompt_list(
        "View proxied fetch host",
        "https://api.acme.com/*",
        &raw.view_fetch_hosts,
    )?;
    raw.view_apis = prompt_list("View platform API grant", "GET /api/flows", &raw.view_apis)?;
    Ok(())
}

fn prompt_tool_surfaces(existing: &[ToolSurfaceArg]) -> anyhow::Result<Vec<ToolSurfaceArg>> {
    let all = [ToolSurfaceArg::Flow, ToolSurfaceArg::AgenticWorker];
    let labels = [
        "flow             usable as a flow node",
        "agentic_worker   callable by the agentic worker",
    ];
    // Default to `flow` when nothing was supplied: that is what a consumer
    // assumes for a tool declaring no capabilities at all, so the pre-selection
    // matches the behaviour the author already has.
    let defaults: Vec<bool> = all
        .iter()
        .map(|s| {
            if existing.is_empty() {
                *s == ToolSurfaceArg::Flow
            } else {
                existing.contains(s)
            }
        })
        .collect();
    let chosen = MultiSelect::new()
        .with_prompt("Tool surfaces (space to toggle)")
        .items(&labels)
        .defaults(&defaults)
        .interact()?;
    Ok(chosen.into_iter().map(|i| all[i]).collect())
}

fn prompt_icon(existing: Option<&Path>) -> anyhow::Result<PathBuf> {
    let mut input = Input::<String>::new().with_prompt("Icon path (svg/png/jpg/webp)");
    if let Some(existing) = existing {
        input = input.default(existing.display().to_string());
    }
    let path = input.validate_with(validate_icon_input).interact_text()?;
    Ok(PathBuf::from(path.trim()))
}

fn prompt_catalogue(raw: &mut RawCapabilities) -> anyhow::Result<()> {
    raw.summary = prompt_optional("Summary (one line)", raw.summary.as_deref())?;
    raw.description = prompt_optional("Description (long form)", raw.description.as_deref())?;
    raw.homepage = prompt_optional("Homepage URL", raw.homepage.as_deref())?;
    raw.repository = prompt_optional("Repository URL", raw.repository.as_deref())?;
    raw.keywords = prompt_list("Keyword", "crm", &raw.keywords)?;
    Ok(())
}

/// Collect a repeatable list, one entry per prompt, until the author submits an
/// empty line. `existing` values (from flags) are kept and shown first.
///
/// `example` is not decoration: "empty to finish" told an author how to stop but
/// never what a valid entry looks like, which left every one of these prompts a
/// blank line with no clue what to type.
fn prompt_list(prompt: &str, example: &str, existing: &[String]) -> anyhow::Result<Vec<String>> {
    let mut out: Vec<String> = existing.to_vec();
    for value in &out {
        println!("  · {value}");
    }
    loop {
        let entry: String = Input::new()
            .with_prompt(format!("{prompt} (e.g. {example} — empty to finish)"))
            .allow_empty(true)
            .interact_text()?;
        let entry = entry.trim().to_string();
        if entry.is_empty() {
            return Ok(out);
        }
        if !out.contains(&entry) {
            out.push(entry);
        }
    }
}

/// A single optional value. An empty answer means "leave it out", so an author
/// can clear a flag-supplied value by submitting nothing.
fn prompt_optional(prompt: &str, existing: Option<&str>) -> anyhow::Result<Option<String>> {
    let mut input = Input::<String>::new()
        .with_prompt(format!("{prompt} (empty to skip)"))
        .allow_empty(true);
    if let Some(existing) = existing {
        input = input.default(existing.to_string());
    }
    let value = input.interact_text()?;
    let value = value.trim().to_string();
    Ok((!value.is_empty()).then_some(value))
}

fn prompt_name(args: &Args) -> anyhow::Result<String> {
    let mut input = Input::<String>::new().with_prompt("Project name (kebab-case)");
    if let Some(existing) = &args.name {
        input = input.default(existing.clone());
    }
    let name = input.validate_with(validate_name).interact_text()?;
    Ok(name.trim().to_string())
}

fn prompt_kind(args: &Args) -> anyhow::Result<Kind> {
    let labels: Vec<String> = KIND_CHOICES
        .iter()
        .map(|(_, name, desc)| format!("{name:<15} {desc}"))
        .collect();
    let default_index = KIND_CHOICES
        .iter()
        .position(|(kind, _, _)| *kind == args.kind)
        .unwrap_or(0);
    let selected = Select::new()
        .with_prompt("Extension kind")
        .items(&labels)
        .default(default_index)
        .interact()?;
    Ok(KIND_CHOICES[selected].0)
}

/// If kind is MCP and `--from-openapi` was not already supplied, ask whether
/// the user wants to seed from an `OpenAPI` spec. Returns the spec path if yes.
fn prompt_openapi_seed(args: &Args, kind: Kind) -> anyhow::Result<Option<PathBuf>> {
    // Pass through whatever was on the command line.
    if args.from_openapi.is_some() || kind != Kind::Mcp {
        return Ok(args.from_openapi.clone());
    }
    let seed = Confirm::new()
        .with_prompt("Seed this MCP extension from an OpenAPI spec?")
        .default(false)
        .interact()?;
    if !seed {
        return Ok(None);
    }
    let path: String = Input::new()
        .with_prompt("OpenAPI spec path")
        .interact_text()?;
    Ok(Some(PathBuf::from(path)))
}

fn prompt_id(args: &Args, name: &str) -> anyhow::Result<String> {
    let default_id = args.id.clone().unwrap_or_else(|| super::default_id(name));
    let id = Input::<String>::new()
        .with_prompt("Extension id (reverse-DNS)")
        .default(default_id)
        .validate_with(validate_id_input)
        .interact_text()?;
    Ok(id.trim().to_string())
}

fn prompt_version(args: &Args) -> anyhow::Result<String> {
    let version = Input::<String>::new()
        .with_prompt("Version")
        .default(args.version.clone())
        .validate_with(validate_version_input)
        .interact_text()?;
    Ok(version.trim().to_string())
}

fn prompt_author(args: &Args) -> anyhow::Result<String> {
    let default_author = args.author.clone().unwrap_or_else(detect_git_author);
    let author = Input::<String>::new()
        .with_prompt("Author")
        .default(default_author)
        .allow_empty(true)
        .interact_text()?;
    Ok(author.trim().to_string())
}

fn prompt_license(args: &Args) -> anyhow::Result<String> {
    let license = Input::<String>::new()
        .with_prompt("License (SPDX id)")
        .default(args.license.clone())
        .interact_text()?;
    Ok(license.trim().to_string())
}

#[allow(clippy::too_many_arguments)]
fn print_summary(
    name: &str,
    kind: Kind,
    id: &str,
    version: &str,
    author: &str,
    license: &str,
    raw: &RawCapabilities,
    icon: Option<&Path>,
) {
    println!("\nAbout to scaffold:");
    println!("  name     {name}");
    println!("  kind     {}", kind.as_str());
    println!("  id       {id}");
    println!("  version  {version}");
    println!("  author   {author}");
    println!("  license  {license}");

    // Only what the author actually chose is echoed back. Printing every field
    // — most of them empty — is how a summary stops being read.
    if let Some(mb) = raw.memory_mb {
        println!("  memory   {mb} MB");
    }
    print_list("network", &raw.network);
    print_list("secrets", &raw.secrets);
    print_list("call", &raw.call_extension_kinds);
    print_list("llm", &raw.llm_roles);
    print_list("oauth", &raw.oauth_providers);
    print_list("offers", &raw.offered);
    print_list("requires", &raw.required);
    if !raw.tool_surfaces.is_empty() {
        let surfaces: Vec<&str> = raw
            .tool_surfaces
            .iter()
            .map(|s| greentic_extension_sdk_contract::ToolCapability::from(*s).as_wire_str())
            .collect();
        println!("  tools    {}", surfaces.join(", "));
    }
    if raw.with_view {
        println!(
            "  view     {} on {:?} at {}",
            raw.view_id.as_deref().unwrap_or("hello"),
            raw.view_surface,
            raw.view_slot
                .as_deref()
                .unwrap_or_else(|| capabilities::default_slot_for(raw.view_surface))
        );
        print_list("view api", &raw.view_apis);
        print_list("view fetch", &raw.view_fetch_hosts);
    }
    if let Some(icon) = icon {
        println!("  icon     {}", icon.display());
    }
    print_list("keywords", &raw.keywords);
}

fn print_list(label: &str, values: &[String]) {
    if !values.is_empty() {
        println!("  {label:<8} {}", values.join(", "));
    }
}

// dialoguer's `Validate<String>` bound forces a `&String` receiver here.
#[allow(clippy::ptr_arg)]
fn validate_name(input: &String) -> Result<(), String> {
    super::validate_name(input.trim()).map_err(|e| e.to_string())
}

#[allow(clippy::ptr_arg)]
fn validate_id_input(input: &String) -> Result<(), String> {
    validate_extension_id(input.trim()).map_err(|e| e.to_string())
}

// dialoguer's `Validate<u32>` bound forces a `&u32` receiver here, the same
// way `Validate<String>` does for the text prompts below.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn validate_memory_input(input: &u32) -> Result<(), String> {
    if (1..=1024).contains(input) {
        Ok(())
    } else {
        Err("runtime.memoryLimitMB must be between 1 and 1024".to_string())
    }
}

// dialoguer's `Validate<String>` bound forces a `&String` receiver here.
#[allow(clippy::ptr_arg)]
fn validate_icon_input(input: &String) -> Result<(), String> {
    let path = Path::new(input.trim());
    if path.is_file() {
        Ok(())
    } else {
        Err(format!("{} is not a readable file", path.display()))
    }
}

#[allow(clippy::ptr_arg)]
fn validate_version_input(input: &String) -> Result<(), String> {
    semver::Version::parse(input.trim())
        .map(|_| ())
        .map_err(|_| "must be valid semver, e.g. 0.1.0".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every scaffoldable `Kind` must be offered by the wizard picker;
    /// otherwise a newly added kind would be silently unreachable interactively.
    #[test]
    fn every_kind_is_offered_in_the_picker() {
        for kind in [
            Kind::Design,
            Kind::Bundle,
            Kind::Deploy,
            Kind::Provider,
            Kind::WasmComponent,
            Kind::Mcp,
            Kind::Llm,
        ] {
            assert!(
                KIND_CHOICES.iter().any(|(choice, _, _)| *choice == kind),
                "Kind {kind:?} is missing from the wizard picker"
            );
        }
    }

    /// Parse a real `gtdx new` command line, so these tests exercise the flag
    /// names an author actually types rather than a struct literal that would
    /// still compile after a rename.
    fn parse_args(extra: &[&str]) -> Args {
        #[derive(clap::Parser)]
        struct Harness {
            #[command(flatten)]
            args: Args,
        }
        let mut argv = vec!["gtdx-new", "demo"];
        argv.extend_from_slice(extra);
        clap::Parser::try_parse_from(argv)
            .map_or_else(|e| panic!("parse {extra:?}: {e}"), |h: Harness| h.args)
    }

    /// One capability flag, and the picker row it must pre-check.
    ///
    /// This table is the link between the two halves of the feature. A flag
    /// added without a row leaves the wizard unable to reach it — exactly the
    /// state `--icon` and `--with-view` were in — and a row wired to the wrong
    /// field pre-checks the wrong thing. Both show up here.
    fn preselect_cases() -> Vec<(Vec<&'static str>, Row)> {
        vec![
            (
                vec!["--permit-network", "https://api.acme.com/*"],
                Row::Network,
            ),
            (vec!["--permit-secret", "acme/"], Row::Secrets),
            (
                vec!["--permit-call-kind", "ProviderExtension"],
                Row::CallKinds,
            ),
            (vec!["--permit-llm-role", "sorla_composer"], Row::LlmRoles),
            (vec!["--permit-oauth", "hubspot"], Row::OauthProviders),
            (vec!["--memory-mb", "128"], Row::Memory),
            (
                vec!["--offer-capability", "greentic:guardrail/topic@1.0.0"],
                Row::Offered,
            ),
            (
                vec!["--require-capability", "greentic:llm/chat@^1"],
                Row::Required,
            ),
            (vec!["--with-view"], Row::View),
            (vec!["--tool-capability", "flow"], Row::ToolSurfaces),
            (vec!["--icon", "logo.svg"], Row::Icon),
            (vec!["--summary", "One line."], Row::Catalogue),
            (vec!["--description", "Long."], Row::Catalogue),
            (vec!["--homepage", "https://acme.example"], Row::Catalogue),
            (
                vec!["--repository", "https://github.com/acme/x"],
                Row::Catalogue,
            ),
            (vec!["--keyword", "crm"], Row::Catalogue),
        ]
    }

    #[test]
    fn every_capability_flag_preselects_exactly_its_own_row() {
        for (flags, expected) in preselect_cases() {
            let args = parse_args(&flags);
            let selected: Vec<Row> = Row::ALL
                .iter()
                .copied()
                .filter(|row| row.preselected(&args))
                .collect();
            assert_eq!(
                selected,
                vec![expected],
                "{flags:?} should pre-check exactly {expected:?}"
            );
        }
    }

    /// A row nobody can reach from the command line is a row the wizard offers
    /// and the flag path cannot reproduce — the two must stay symmetric.
    #[test]
    fn every_row_has_a_flag_that_reaches_it() {
        let covered: Vec<Row> = preselect_cases().into_iter().map(|(_, row)| row).collect();
        for row in Row::ALL {
            assert!(
                covered.contains(row),
                "{row:?} is offered by the wizard but no flag in preselect_cases() reaches it"
            );
        }
    }

    /// With no capability flags at all, nothing is pre-checked: the wizard's
    /// default is still one keystroke past the picker.
    #[test]
    fn a_bare_command_line_preselects_nothing() {
        let args = parse_args(&[]);
        assert!(Row::ALL.iter().all(|row| !row.preselected(&args)));
    }

    #[test]
    fn row_labels_are_unique_and_described() {
        let mut labels: Vec<&str> = Row::ALL.iter().map(|r| r.label()).collect();
        let count = labels.len();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), count, "duplicate row label");
        assert!(Row::ALL.iter().all(|r| !r.description().is_empty()));
    }

    /// `wasix:mcp/router` artifacts carry no contributions block, so the view
    /// row must not be offered for them — `validate_with_view` would reject the
    /// choice moments later.
    #[test]
    fn the_view_row_is_hidden_for_mcp() {
        use clap::ValueEnum as _;

        assert!(!Row::View.applies_to(Kind::Mcp));
        // Derived rather than listed, so a kind added to `Kind` is covered here
        // without an edit — and so this compiles on both release lines, whose
        // kind sets differ.
        for kind in Kind::value_variants().iter().filter(|k| **k != Kind::Mcp) {
            assert!(Row::View.applies_to(*kind), "{kind:?}");
        }
    }

    /// The tool-surface row follows each kind's own template, so a kind that
    /// starts or stops contributing tools needs no edit here.
    #[test]
    fn the_tool_surface_row_follows_each_kinds_template() {
        use clap::ValueEnum as _;

        for kind in Kind::value_variants() {
            assert_eq!(
                Row::ToolSurfaces.applies_to(*kind),
                template::kind_contributes_tools(kind.as_str()),
                "{kind:?}"
            );
        }
    }

    /// Every other row applies to every kind; a row gaining a restriction
    /// without a matching test would otherwise disappear silently.
    #[test]
    fn only_the_view_and_tool_surface_rows_are_kind_gated() {
        use clap::ValueEnum as _;

        for row in Row::ALL {
            if matches!(row, Row::View | Row::ToolSurfaces) {
                continue;
            }
            for kind in Kind::value_variants() {
                assert!(row.applies_to(*kind), "{row:?} should apply to {kind:?}");
            }
        }
    }

    #[test]
    fn memory_validation_matches_the_contract_bound() {
        assert!(validate_memory_input(&1).is_ok());
        assert!(validate_memory_input(&1024).is_ok());
        assert!(validate_memory_input(&0).is_err());
        assert!(validate_memory_input(&1025).is_err());
    }

    #[test]
    fn name_validation_rejects_empty_and_whitespace() {
        assert!(validate_name(&"my-ext".to_string()).is_ok());
        assert!(validate_name(&"   ".to_string()).is_err());
        assert!(validate_name(&"my ext".to_string()).is_err());
    }

    /// The name becomes the cargo package name (with `.` folded to `-`), which
    /// may not start with a digit — cargo refuses the scaffold outright with
    /// `invalid character `3` in package name`.
    #[test]
    fn name_validation_rejects_a_leading_digit() {
        let msg = validate_name(&"3aigent-designer".to_string()).expect_err("leading digit");
        assert!(msg.contains("3aigent-designer"), "{msg}");
        assert!(msg.contains("cargo"), "should say whose rule it is: {msg}");
    }

    /// A dotted name is folded to `-` for the cargo package name, so it is fine
    /// — several kinds' fixtures rely on it.
    #[test]
    fn name_validation_accepts_a_dotted_name() {
        assert!(validate_name(&"greentic.snap-test".to_string()).is_ok());
    }

    /// Only the *first* character is barred from being a digit here. A digit-led
    /// word later on is fine for cargo; it is the derived id that rejects it,
    /// with its own message about WIT.
    #[test]
    fn name_validation_accepts_a_digit_led_word() {
        assert!(validate_name(&"provider-3aigent".to_string()).is_ok());
        assert!(validate_name(&"provider-aigent3".to_string()).is_ok());
    }

    /// A `\` line-continuation that gets lost leaves a run of indentation
    /// spaces in the middle of the sentence the author actually reads.
    #[test]
    fn name_validation_message_has_no_double_spaces() {
        let msg = validate_name(&"3aigent-designer".to_string()).expect_err("leading digit");
        assert!(!msg.contains("  "), "collapsed continuation in: {msg}");
    }

    #[test]
    fn name_validation_rejects_underscore() {
        let msg = validate_name(&"telco_x".to_string()).expect_err("underscore");
        assert!(msg.contains('_'), "{msg}");
    }

    #[test]
    fn id_validation_enforces_reverse_dns() {
        assert!(validate_id_input(&"com.acme.my-ext".to_string()).is_ok());
        assert!(validate_id_input(&"not-reverse-dns".to_string()).is_err());
    }

    /// Digits are fine once a word has started; a digit-led word is not, because
    /// the id becomes the WIT package name.
    #[test]
    fn id_validation_accepts_digits_inside_a_word() {
        assert!(validate_id_input(&"greentic.aigent3-designer".to_string()).is_ok());
    }

    #[test]
    fn id_validation_rejects_a_digit_led_word() {
        let msg = validate_id_input(&"greentic.3aigent-designer".to_string())
            .expect_err("digit-led word");
        assert!(msg.contains("3aigent"), "{msg}");
        assert!(msg.contains("WIT"), "{msg}");
    }

    /// The old message was a bare "must be reverse-DNS, e.g. com.acme.my-ext" —
    /// it never said which part of the id was wrong.
    #[test]
    fn id_validation_message_names_the_offending_part() {
        let msg = validate_id_input(&"greentic.telco_x".to_string()).expect_err("underscore");
        assert!(msg.contains("telco_x"), "{msg}");
        assert!(msg.contains('_'), "{msg}");
    }

    #[test]
    fn version_validation_enforces_semver() {
        assert!(validate_version_input(&"0.1.0".to_string()).is_ok());
        assert!(validate_version_input(&"not-a-version".to_string()).is_err());
    }
}
