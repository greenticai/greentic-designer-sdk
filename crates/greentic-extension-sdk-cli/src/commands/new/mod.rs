mod capabilities;
mod view_addon;
mod wizard;

use std::{
    collections::BTreeMap,
    fs,
    io::IsTerminal,
    path::{Path, PathBuf},
    time::SystemTime,
};

use clap::Args as ClapArgs;
use greentic_extension_sdk_contract::extension_id::validate_extension_id;

use crate::scaffold::{
    Kind,
    contract_lock::ContractLock,
    embedded::{self, CONTRACT_VERSION},
    preflight::{self, Check},
    template::{self, Context},
};

#[derive(ClapArgs, Debug)]
// CLI flag struct: each bool is an independent on/off switch, not shared state.
#[allow(clippy::struct_excessive_bools)]
pub struct Args {
    /// Project folder name (kebab-case). Also default id suffix.
    /// Omit (and run on a terminal) to launch the interactive wizard.
    pub name: Option<String>,

    /// Extension kind
    #[arg(short = 'k', long, value_enum, default_value = "design")]
    pub kind: Kind,

    /// Extension id (reverse-DNS). Default: greentic.<name>
    #[arg(short = 'i', long)]
    pub id: Option<String>,

    /// Initial version
    #[arg(short = 'v', long, default_value = "0.1.0")]
    pub version: String,

    /// Author name; defaults to git config user.name
    #[arg(long)]
    pub author: Option<String>,

    /// SPDX license id
    #[arg(long, default_value = "Apache-2.0")]
    pub license: String,

    /// Skip `git init`
    #[arg(long)]
    pub no_git: bool,

    /// Output directory; defaults to ./<name>
    #[arg(long)]
    pub dir: Option<PathBuf>,

    /// Overwrite if target exists
    #[arg(long)]
    pub force: bool,

    /// Skip the interactive wizard; resolve everything from flags/defaults.
    #[arg(short = 'y', long)]
    pub yes: bool,

    /// Force the interactive wizard even when a name/flags are given.
    #[arg(short = 'w', long)]
    pub wizard: bool,

    /// Node type ID (defaults to derived suffix of --name).
    #[arg(long)]
    pub node_type_id: Option<String>,

    /// Display label for the node (defaults to humanized --name).
    #[arg(long)]
    pub label: Option<String>,

    /// `--kind wasm-component` only: OCI reference of the already-published
    /// component that executes the node, ideally pinned by digest
    /// (`oci://ghcr.io/org/component-x@sha256:...`).
    ///
    /// A node's component must be reachable by `oci_ref`: the designer's flow
    /// compiler reads `runtime.components.<runtime_ref>.oci_ref` and skips a
    /// `gtpack`-only component, and the install path relocates a nested
    /// `.gtpack` for `ProviderExtension` only. Omitted, the scaffold writes a
    /// placeholder you must replace before publishing.
    #[arg(long, value_name = "OCI_REF")]
    pub component_ref: Option<String>,

    /// Seed a `--kind mcp` extension from an OpenAPI/Swagger spec (generates the
    /// router via greentic-mcp-gen instead of the empty echo skeleton).
    #[arg(long, value_name = "SPEC")]
    pub from_openapi: Option<PathBuf>,

    // --- runtime limits -----------------------------------------------------
    /// Memory ceiling for the extension's components (`runtime.memoryLimitMB`),
    /// 1..=1024. Omitted, the scaffold keeps the contract default of 64.
    #[arg(long, value_name = "MB", help_heading = "Capabilities")]
    pub memory_mb: Option<u32>,

    // --- host permissions ---------------------------------------------------
    /// URL pattern the guest may reach, e.g. `https://api.acme.com/*`. Repeat
    /// for more. https only; plain http is accepted for loopback hosts only.
    #[arg(long, value_name = "PATTERN", help_heading = "Capabilities")]
    pub permit_network: Vec<String>,

    /// Secret read *grant* the guest may use — `*`, a `secret://…/` URI, or a
    /// path prefix ending in `/`. Repeat for more. Credential field names an
    /// operator supplies belong in `requiredSecrets`, not here.
    #[arg(long, value_name = "GRANT", help_heading = "Capabilities")]
    pub permit_secret: Vec<String>,

    /// Extension kind this extension may call into (`runtime.permissions.
    /// callExtensionKinds`), e.g. `ProviderExtension`. Repeat for more.
    #[arg(long, value_name = "KIND", help_heading = "Capabilities")]
    pub permit_call_kind: Vec<String>,

    /// LLM role the extension may request from the host `llm` import, e.g.
    /// `sorla_composer`. Repeat for more.
    #[arg(long, value_name = "ROLE", help_heading = "Capabilities")]
    pub permit_llm_role: Vec<String>,

    /// OAuth provider id the extension may request tokens for, e.g. `hubspot`.
    /// Repeat for more.
    #[arg(long, value_name = "PROVIDER", help_heading = "Capabilities")]
    pub permit_oauth: Vec<String>,

    // --- capability contracts ----------------------------------------------
    /// Capability contract this extension provides, as `<id>@<exact-version>`
    /// (e.g. `greentic:guardrail/topic@1.0.0`). Repeat for more.
    #[arg(long, value_name = "ID@VERSION", help_heading = "Capabilities")]
    pub offer_capability: Vec<String>,

    /// Capability contract this extension needs, as `<id>@<version-req>`
    /// (e.g. `greentic:llm/chat@^1`). Repeat for more.
    #[arg(long, value_name = "ID@REQ", help_heading = "Capabilities")]
    pub require_capability: Vec<String>,

    // --- contributed view ---------------------------------------------------
    /// Scaffold an example contributed view (a UI page) alongside the extension.
    #[arg(long, default_value_t = false, help_heading = "Capabilities")]
    pub with_view: bool,

    /// Id of the scaffolded view. Requires `--with-view`. Default: `hello`.
    #[arg(long, value_name = "ID", help_heading = "Capabilities")]
    pub view_id: Option<String>,

    /// Host application the view targets. Requires `--with-view`.
    #[arg(
        long,
        value_enum,
        value_name = "SURFACE",
        default_value_t,
        help_heading = "Capabilities"
    )]
    pub view_surface: capabilities::ViewSurfaceArg,

    /// Placement slot for the view. Requires `--with-view`. Default:
    /// `designer.sidebar` / `admin.sidebar`, following `--view-surface`.
    #[arg(long, value_name = "SLOT", help_heading = "Capabilities")]
    pub view_slot: Option<String>,

    /// Literal title shown when the view's `title_key` has no translation.
    /// Requires `--with-view`. Defaults to the view id, humanised.
    #[arg(long, value_name = "TEXT", help_heading = "Capabilities")]
    pub view_title: Option<String>,

    /// Floor on who may see the view. Requires `--with-view`.
    #[arg(
        long,
        value_enum,
        value_name = "VISIBILITY",
        default_value_t,
        help_heading = "Capabilities"
    )]
    pub view_min_visibility: capabilities::ViewVisibilityArg,

    /// Host the view may reach through the host's server-side proxy
    /// (`permissions.ui.fetchHosts`). Requires `--with-view`. Repeat for more.
    #[arg(long, value_name = "PATTERN", help_heading = "Capabilities")]
    pub view_fetch_host: Vec<String>,

    /// Platform REST endpoint the view may call through the bridge, as
    /// `"<METHOD> <path-pattern>"` (e.g. `"GET /api/flows"`). Requires
    /// `--with-view`. Repeat for more.
    #[arg(long, value_name = "METHOD PATH", help_heading = "Capabilities")]
    pub view_api: Vec<String>,

    // --- tool surfaces ------------------------------------------------------
    /// Runtime context a contributed tool may be invoked from. Repeat to
    /// declare both. Only valid for a kind that contributes tools.
    #[arg(
        long,
        value_enum,
        value_name = "SURFACE",
        help_heading = "Capabilities"
    )]
    pub tool_capability: Vec<capabilities::ToolSurfaceArg>,

    // --- icon + catalogue metadata ------------------------------------------
    /// Path to an icon file (svg/png/jpg/webp, <= 1 MiB) to attach as the
    /// extension's `metadata.icon`. Copied into the scaffold's `assets/` dir.
    #[arg(long, value_name = "PATH", help_heading = "Capabilities")]
    pub icon: Option<PathBuf>,

    /// One-line summary shown in catalogue listings (`metadata.summary`).
    #[arg(long, value_name = "TEXT", help_heading = "Capabilities")]
    pub summary: Option<String>,

    /// Long-form description (`metadata.description`).
    #[arg(long, value_name = "TEXT", help_heading = "Capabilities")]
    pub description: Option<String>,

    /// Project homepage URL (`metadata.homepage`).
    #[arg(long, value_name = "URL", help_heading = "Capabilities")]
    pub homepage: Option<String>,

    /// Source repository URL (`metadata.repository`).
    #[arg(long, value_name = "URL", help_heading = "Capabilities")]
    pub repository: Option<String>,

    /// Catalogue keyword (`metadata.keywords`). Repeat for more.
    #[arg(long, value_name = "KEYWORD", help_heading = "Capabilities")]
    pub keyword: Vec<String>,
}

impl Args {
    /// The capability inputs exactly as the command line supplied them.
    ///
    /// Both resolution paths start here: the flag path validates this as-is,
    /// and the wizard uses it for prompt defaults before overriding what the
    /// author changes. One constructor means the two cannot drift into
    /// carrying different fields.
    fn raw_capabilities(&self) -> capabilities::RawCapabilities {
        capabilities::RawCapabilities {
            memory_mb: self.memory_mb,
            network: self.permit_network.clone(),
            secrets: self.permit_secret.clone(),
            call_extension_kinds: self.permit_call_kind.clone(),
            llm_roles: self.permit_llm_role.clone(),
            oauth_providers: self.permit_oauth.clone(),
            offered: self.offer_capability.clone(),
            required: self.require_capability.clone(),
            tool_surfaces: self.tool_capability.clone(),
            summary: self.summary.clone(),
            description: self.description.clone(),
            homepage: self.homepage.clone(),
            repository: self.repository.clone(),
            keywords: self.keyword.clone(),
            with_view: self.with_view,
            view_id: self.view_id.clone(),
            view_surface: self.view_surface,
            view_slot: self.view_slot.clone(),
            view_title: self.view_title.clone(),
            view_min_visibility: self.view_min_visibility,
            view_fetch_hosts: self.view_fetch_host.clone(),
            view_apis: self.view_api.clone(),
        }
    }
}

/// Fully-resolved scaffold inputs, produced either from CLI flags
/// (non-interactive) or from the interactive wizard.
pub(super) struct Resolved {
    name: String,
    kind: Kind,
    id: String,
    version: String,
    author: String,
    license: String,
    no_git: bool,
    dir: Option<PathBuf>,
    force: bool,
    node_type_id: Option<String>,
    label: Option<String>,
    component_ref: Option<String>,
    /// `OpenAPI` spec path for `--kind mcp` seeded scaffolds.
    from_openapi: Option<PathBuf>,
    /// Icon to copy into `assets/` and record as `metadata.icon`.
    ///
    /// Held here rather than read straight off `Args` in `run`, so the wizard
    /// can set it — while it lived only on `Args` the interactive path had no
    /// way to reach the field at all.
    icon: Option<PathBuf>,
    /// Validated capability, permission and catalogue-metadata declarations.
    capabilities: capabilities::CapabilitySpec,
    /// The contributed view, when `--with-view` was asked for. `Some` is what
    /// drives scaffolding `assets/views/<id>/`, so there is no separate bool
    /// that could disagree with it.
    view: Option<capabilities::ViewSpec>,
    /// Advisory notes from capability resolution, printed before scaffolding.
    capability_notes: Vec<String>,
}

/// Pull the digest out of a digest-pinned OCI reference.
///
/// `oci://host/ns/name@sha256:<64 lowercase hex>` yields the hex. Anything
/// else — a tag-only ref, a different algorithm, a truncated or uppercase
/// digest — yields `None`, because writing a digest the reference did not
/// actually pin would be worse than the placeholder it replaces. Lowercase
/// only, matching the v2 schema pattern and the `Sha256` newtype.
fn oci_ref_digest(reference: &str) -> Option<&str> {
    let (_, digest) = reference.rsplit_once("@sha256:")?;
    let well_formed = digest.len() == 64
        && digest
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    well_formed.then_some(digest)
}

pub fn run(args: &Args, _home: &Path) -> anyhow::Result<()> {
    let resolved = resolve(args)?;

    let target = resolved
        .dir
        .clone()
        .unwrap_or_else(|| PathBuf::from(&resolved.name));

    validate_from_openapi(resolved.kind, resolved.from_openapi.as_deref())?;
    validate_with_view(resolved.kind, resolved.view.is_some())?;

    for note in &resolved.capability_notes {
        println!("  ! {note}");
    }

    run_preflight(&target, resolved.force)?;
    prepare_target(&target, resolved.force)?;

    let mut ctx = build_context(&resolved);

    let files_written = if let Some(spec) = resolved.from_openapi.as_deref() {
        scaffold_from_openapi(&ctx, &resolved, spec, &target)?
    } else {
        let mut n = render_templates(&mut ctx, &resolved, &target)?;
        n += write_wit_and_lock(resolved.kind.as_str(), &target)?;
        n
    };

    if let Some(icon) = resolved.icon.as_deref() {
        let rel = crate::icon::apply_icon(&target, icon)?;
        println!("  icon: {rel}");
    }

    make_scripts_executable(&target)?;
    run_git_init(&target, resolved.no_git);

    // The OpenAPI path already printed its own "Next: gtdx publish …" line
    // inside scaffold_from_openapi; skip the generic next-steps block there.
    if resolved.from_openapi.is_none() {
        print_summary(resolved.kind.as_str(), &target, files_written);
    }
    Ok(())
}

/// Decide between the interactive wizard and flag-driven resolution.
///
/// The wizard runs when explicitly requested (`--wizard`) or when no project
/// name was supplied — provided we are attached to a terminal and `--yes` was
/// not passed. Otherwise inputs are taken verbatim from the CLI flags, which
/// keeps the original scripted/`--yes` behaviour intact.
fn resolve(args: &Args) -> anyhow::Result<Resolved> {
    let wants_wizard = args.wizard || (args.name.is_none() && !args.yes);
    if wants_wizard && std::io::stdin().is_terminal() {
        return wizard::run(args);
    }
    resolve_from_flags(args)
}

fn resolve_from_flags(args: &Args) -> anyhow::Result<Resolved> {
    let name = args.name.clone().ok_or_else(|| {
        anyhow::anyhow!(
            "missing project name: run `gtdx new <name> [flags]`, or `gtdx new` on a terminal for the interactive wizard"
        )
    })?;
    let id = args.id.clone().unwrap_or_else(|| default_id(&name));
    let author = args.author.clone().unwrap_or_else(detect_git_author);
    validate_name(&name)?;
    // A derived id that fails is really a complaint about the project name, and
    // saying so is the difference between a fixable error and a baffling one:
    // `provider-3aigent` is a perfectly good crate name, so nothing about
    // "invalid extension id" points back at what the author typed.
    validate_id(&id).map_err(|e| {
        if args.id.is_some() {
            e
        } else {
            e.context(format!(
                "the id was derived from the project name {name:?}; rename the project \
                 or pass --id <reverse-dns>"
            ))
        }
    })?;
    validate_version(&args.version)?;
    let caps = capabilities::resolve(&args.raw_capabilities())?;
    Ok(Resolved {
        name,
        kind: args.kind,
        id,
        version: args.version.clone(),
        author,
        license: args.license.clone(),
        no_git: args.no_git,
        dir: args.dir.clone(),
        force: args.force,
        node_type_id: args.node_type_id.clone(),
        label: args.label.clone(),
        component_ref: args.component_ref.clone(),
        from_openapi: args.from_openapi.clone(),
        icon: args.icon.clone(),
        capabilities: caps.spec,
        view: caps.view,
        capability_notes: caps.notes,
    })
}

fn run_preflight(target: &Path, force: bool) -> anyhow::Result<()> {
    let checks = vec![
        preflight::check_cargo_available(),
        preflight::check_cargo_component_available(),
        preflight::check_wasm32_wasip2_target(),
        preflight::check_target_dir(target, force),
    ];
    print_checks(&checks);
    if checks.iter().any(|c| matches!(c, Check::Fail { .. })) {
        anyhow::bail!("preflight failed; fix the issues above and re-run");
    }
    Ok(())
}

fn prepare_target(target: &Path, force: bool) -> anyhow::Result<()> {
    if target.exists() && force {
        fs::remove_dir_all(target)?;
    }
    fs::create_dir_all(target)?;
    Ok(())
}

/// `(placeholder, embedded WIT file suffix)` for every package a scaffolded
/// `world.wit` can reference. Keys are used as `{{<placeholder>}}`.
const WIT_VERSION_PLACEHOLDERS: &[(&str, &str)] = &[
    ("wit_version_base", "base"),
    ("wit_version_host", "host"),
    ("wit_version_design", "design"),
    ("wit_version_bundle", "bundle"),
    ("wit_version_deploy", "deploy"),
    ("wit_version_provider", "provider"),
    ("wit_version_addon", "addon"),
];

/// The `greentic:extension-<pkg>@<version>` reference a given kind is authored
/// against, for the generated README / AGENTS prose.
///
/// `wasm-component` and `llm` reuse the design world, and `mcp` is not a
/// greentic extension package at all — so this is a lookup, not
/// `format!("greentic:extension-{kind}")`, which produced non-existent
/// references such as `greentic:extension-wasm-component@0.2.0`.
fn kind_wit_ref(kind: &str) -> String {
    let pkg = match kind {
        "wasm-component" | "llm" => "design",
        other => other,
    };
    if pkg == "mcp" {
        return "wasix:mcp/router".to_string();
    }
    embedded::package_version_for(pkg).map_or_else(
        || format!("greentic:extension-{pkg}"),
        |v| format!("greentic:extension-{pkg}@{v}"),
    )
}

fn build_context(resolved: &Resolved) -> Context {
    let mut ctx = Context::new();
    ctx.set("name", resolved.name.clone());
    let name_cargo = resolved.name.replace('.', "-");
    ctx.set("name_cargo", &name_cargo);
    ctx.set("kind", resolved.kind.as_str());
    // Assumes ASCII kebab-case `name`; non-ASCII or all-uppercase input may produce odd labels.
    let derived_id = resolved
        .name
        .split('.')
        .next_back()
        .unwrap_or(&resolved.name)
        .to_string();
    let node_type_id = resolved
        .node_type_id
        .clone()
        .unwrap_or_else(|| derived_id.clone());
    let label = resolved.label.clone().unwrap_or_else(|| {
        derived_id
            .replace('-', " ")
            .split(' ')
            .map(|w| {
                let mut c = w.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    });
    ctx.set("node_type_id", &node_type_id);
    // A placeholder rather than a blank: `Context::render` would accept "" and
    // emit `"oci_ref": ""`, which deserializes fine and resolves to nothing at
    // compile time. `example.invalid` is reserved by RFC 2606, so a scaffold
    // that reaches a registry fails loudly instead of hitting a real host.
    let placeholder_digest = "0".repeat(64);
    let component_ref = resolved.component_ref.clone().unwrap_or_else(|| {
        format!("oci://example.invalid/REPLACE-ME/{node_type_id}@sha256:{placeholder_digest}")
    });
    // The reference is digest-pinned, so the node component's `sha256` is
    // already known here — reading it back out of the ref is the difference
    // between a scaffold that passes `gtdx lint --publish` and one that trips
    // `E_SHA256_ZERO` despite the author having supplied everything asked of
    // them. A ref without a usable digest keeps the placeholder, and that
    // refusal is the documented behaviour rather than something to paper over.
    let component_digest = oci_ref_digest(&component_ref).unwrap_or(&placeholder_digest);
    ctx.set("component_digest", component_digest);
    ctx.set("component_ref", component_ref.clone());
    ctx.set("label", &label);
    let id = resolved.id.as_str();
    ctx.set("id", id);
    ctx.set("id_wit", id_to_wit_package(id));
    ctx.set("version", &resolved.version);
    ctx.set("author", &resolved.author);
    ctx.set("license", &resolved.license);
    ctx.set("contract_version", CONTRACT_VERSION);
    // Per-package WIT versions, read from the embedded packages themselves.
    //
    // `CONTRACT_VERSION` is the contract *generation*, not a uniform per-file
    // version: within generation 0.2.0, `extension-host` is still `@0.1.0` and
    // `extension-design` is `@0.3.0`. Rendering a world with one version for
    // every package asks for `greentic:extension-host@0.2.0`, which has never
    // existed, and every scaffolded project fails its first
    // `cargo component build` with `package '...' not found`. Never substitute
    // `contract_version` into a `world.wit`.
    for (key, suffix) in WIT_VERSION_PLACEHOLDERS {
        // `unwrap_or(CONTRACT_VERSION)` would reintroduce exactly the bug this
        // exists to prevent, so an absent package renders nothing and
        // `Context::render` then fails loudly on the unsubstituted token.
        if let Some(v) = embedded::package_version_for(suffix) {
            ctx.set(key, v);
        }
    }
    ctx.set("kind_wit_ref", kind_wit_ref(resolved.kind.as_str()));
    // `sdk_version` is the gtdx CLI / SDK crate version (the toolchain that
    // generated this scaffold). v2 describe.json templates use it for
    // `engine.*` + `compat.*` so scaffolds pin to the same SDK line that
    // produced them, independent of the WIT contract `CONTRACT_VERSION`
    // (which tracks the wit-package @version, evolves slower).
    ctx.set("sdk_version", env!("CARGO_PKG_VERSION"));
    // The minimum designer an extension needs is the describe-contract floor,
    // NOT the SDK that generated it — see `compat::MIN_DESIGNER_VERSION`.
    // Templates previously reused `sdk_version` here, which made every fresh
    // scaffold declare itself incompatible with designers that can load it.
    ctx.set(
        "min_designer_version",
        greentic_extension_sdk_contract::compat::MIN_DESIGNER_VERSION,
    );
    // No `config_schema` placeholder is set, and the templates emit no
    // top-level `configSchema` — deliberately, including under
    // `--with-view`. Unlike the per-contribution `config_schema` fields on
    // `Recipe`/`Addon`/`NodeType`, which are *required* and so must be
    // scaffolded with something, this one is optional and most extensions
    // have no operator configuration at all. An empty placeholder schema
    // would render as an empty form in the admin console — worse for the
    // operator than the field's absence, which the console reads as
    // "nothing to configure". See `docs/authoring-config.md`.
    //
    // `runtime_ref_key` is the key used in v2 `runtime.components` map and
    // in every `nodeTypes[].runtime_ref` / `tools[].runtime_ref`. Default
    // is the last dotted segment of the extension id (matches the
    // dw-canvas + dw-composers convention we shipped in #7 / #21).
    let runtime_ref_key = id.split('.').next_back().unwrap_or(id).to_string();
    ctx.set("runtime_ref_key", &runtime_ref_key);
    ctx
}

fn scaffold_from_openapi(
    ctx: &Context,
    resolved: &Resolved,
    spec: &Path,
    target: &Path,
) -> anyhow::Result<usize> {
    use crate::scaffold::openapi;

    let bin = openapi::resolve_mcp_gen()?;
    let artifacts = openapi::run_generator(&bin, spec, target)?;

    // Render the mcp describe.json template, then patch network + secrets.
    let mut files = 1usize; // the generated wasm
    let describe_tmpl = template::load_templates_kind("mcp")?
        .into_iter()
        .find(|e| e.dst_rel.ends_with("describe.json"))
        .ok_or_else(|| anyhow::anyhow!("mcp describe.json template missing"))?;
    let rendered = ctx.render(std::str::from_utf8(describe_tmpl.src_bytes)?)?;
    let authored = openapi::author_describe_json(&rendered, artifacts.meta.as_deref())?;
    // The capability flags apply here too. `author_describe_json` has already
    // filled `runtime.permissions.network` from the spec's `servers` block, and
    // `capabilities::apply` appends rather than replaces, so a `--permit-network`
    // on top of a seeded scaffold widens the allowlist instead of erasing it.
    let authored = if resolved.capabilities.is_empty() {
        authored
    } else {
        let mut describe: serde_json::Value = serde_json::from_str(&authored)
            .map_err(|e| anyhow::anyhow!("parse authored describe.json: {e}"))?;
        capabilities::apply(&mut describe, &resolved.capabilities)?;
        serde_json::to_string_pretty(&describe)? + "\n"
    };
    template::write_file(&target.join("describe.json"), authored.as_bytes())?;
    files += 1;

    // Minimal Cargo.toml anchor so `gtdx publish --manifest ./Cargo.toml` works.
    let cargo_anchor = format!(
        "# Anchor manifest for `gtdx publish --wasm`. The component is the\n\
         # pre-built wasm generated from the OpenAPI spec; there is no crate to build here.\n\
         [package]\nname = \"{}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n[lib]\npath = \"/dev/null\"\n",
        target
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("mcp-ext")
    );
    template::write_file(&target.join("Cargo.toml"), cargo_anchor.as_bytes())?;
    files += 1;

    println!(
        "  Next: gtdx publish --wasm {} --manifest {} .",
        artifacts.wasm.display(),
        target.join("Cargo.toml").display()
    );
    Ok(files)
}

fn render_templates(
    ctx: &mut Context,
    resolved: &Resolved,
    target: &Path,
) -> anyhow::Result<usize> {
    let mut files_written = 0usize;
    for entry in template::load_templates_common() {
        let dst = target.join(&entry.dst_rel);
        let rendered = ctx.render(std::str::from_utf8(entry.src_bytes)?)?;
        template::write_file(&dst, rendered.as_bytes())?;
        files_written += 1;
    }
    for entry in template::load_templates_kind(resolved.kind.as_str())? {
        let dst = target.join(&entry.dst_rel);
        let rendered = ctx.render(std::str::from_utf8(entry.src_bytes)?)?;
        template::write_file(&dst, rendered.as_bytes())?;
        files_written += 1;
    }

    // The describe is read, patched and written once — and only when there is
    // something to patch, so an unconfigured scaffold keeps the template's own
    // bytes rather than a round-trip through serde_json.
    //
    // The view patch in particular must land *before* the view-addon templates
    // render: the example page's `{{view_tool}}` placeholder needs the tool
    // name this kind actually contributes (or none), and that is only known
    // once `contributions.tools` has been inspected.
    let mut chosen_tool = None;
    if !resolved.capabilities.is_empty() || resolved.view.is_some() {
        let describe_path = target.join("describe.json");
        let current = std::fs::read_to_string(&describe_path)
            .map_err(|e| anyhow::anyhow!("read {}: {e}", describe_path.display()))?;
        let mut describe: serde_json::Value = serde_json::from_str(&current)
            .map_err(|e| anyhow::anyhow!("parse rendered {}: {e}", describe_path.display()))?;

        capabilities::apply(&mut describe, &resolved.capabilities)?;
        if let Some(view) = &resolved.view {
            chosen_tool = view_addon::add_view_to_describe(&mut describe, view)?;
        }
        template::write_file(
            &describe_path,
            (serde_json::to_string_pretty(&describe)? + "\n").as_bytes(),
        )?;
    }

    if let Some(view) = &resolved.view {
        let (tool_name, tool_args) = match chosen_tool {
            Some(tool) => (tool.name, tool.args),
            None => (String::new(), serde_json::json!({})),
        };
        ctx.set("view_tool", tool_name);
        ctx.set(
            "view_tool_args",
            serde_json::to_string(&tool_args)
                .map_err(|e| anyhow::anyhow!("serialize placeholder tool args: {e}"))?,
        );
        for entry in template::load_templates_view_addon(&view.id) {
            let dst = target.join(&entry.dst_rel);
            let rendered = ctx.render(std::str::from_utf8(entry.src_bytes)?)?;
            template::write_file(&dst, rendered.as_bytes())?;
            files_written += 1;
        }
    }
    Ok(files_written)
}

/// Write the embedded WIT contract deps + `.gtdx-contract.lock` for `kind`
/// into `target`. `pub(crate)` so `commands::openapi::run` can reuse it to
/// scaffold the WIT side of a generated `DesignExtension` connector (the same
/// contract files a `gtdx new --kind design` scaffold gets).
pub(crate) fn write_wit_and_lock(kind: &str, target: &Path) -> anyhow::Result<usize> {
    let mut files_written = 0usize;
    let mut lock_files = BTreeMap::new();
    for file in embedded::files_for_kind(kind)? {
        let pkg_dir = wit_package_subdir_for(file.name)?;
        let dst = target
            .join("wit/deps/greentic")
            .join(pkg_dir)
            .join("world.wit");
        template::write_file(&dst, file.bytes)?;
        let rel = dst.strip_prefix(target).unwrap().display().to_string();
        lock_files.insert(rel, format!("sha256:{}", embedded::sha256_hex(file.bytes)));
        files_written += 1;
    }
    let lock = ContractLock {
        contract_version: CONTRACT_VERSION.to_string(),
        generated_by: format!("gtdx {}", env!("CARGO_PKG_VERSION")),
        generated_at: now_iso8601(),
        files: lock_files,
    };
    template::write_file(
        &target.join(".gtdx-contract.lock"),
        lock.to_toml()?.as_bytes(),
    )?;
    files_written += 1;
    Ok(files_written)
}

#[cfg(unix)]
fn make_scripts_executable(target: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    for rel in ["build.sh", "ci/local_check.sh"] {
        let p = target.join(rel);
        if p.exists() {
            let mut perms = fs::metadata(&p)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&p, perms)?;
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn make_scripts_executable(_target: &Path) -> anyhow::Result<()> {
    Ok(())
}

fn run_git_init(target: &Path, no_git: bool) {
    if no_git {
        return;
    }
    let _ = std::process::Command::new("git")
        .arg("init")
        .arg("--quiet")
        .current_dir(target)
        .status();
}

fn print_summary(kind: &str, target: &Path, files_written: usize) {
    println!();
    println!(
        "Scaffolded {} extension at {} ({} files, contract {}).",
        kind,
        target.display(),
        files_written,
        CONTRACT_VERSION
    );
    println!();
    println!("Next steps:");
    println!("  cd {}", target.display());
    println!("  gtdx dev        # watch, rebuild, reinstall");
    println!("  gtdx publish    # pack to dist/");
}

fn detect_git_author() -> String {
    std::process::Command::new("git")
        .args(["config", "--get", "user.name"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if s.is_empty() { None } else { Some(s) }
            } else {
                None
            }
        })
        .unwrap_or_else(|| "Unknown".to_string())
}

/// The `metadata.id` a scaffold gets when the author passes no `--id`.
///
/// `greentic.` is a default, not a requirement — `E_ID_PATTERN` accepts any
/// reverse-DNS namespace. It stays the default because a scaffold has no way to
/// know which namespace its author owns, and `com.example.<name>` (the previous
/// default) shipped a placeholder namespace nobody controls.
///
/// A name that scaffolds at all is `<lowercase-kebab>`, and the id rule allows a
/// later segment to start with a digit, so `greentic.<name>` is always valid
/// here — including `greentic.3aigent-designer`, which the old rule rejected.
pub(crate) fn default_id(name: &str) -> String {
    format!("greentic.{name}")
}

/// What a project name must match once its dots are folded to dashes.
const PROJECT_NAME_PATTERN: &str = "^[a-z][a-z0-9]*(-[a-z0-9]+)*$";

/// The project name must be a valid cargo package name.
///
/// It is spent as `[package] name`, with `.` folded to `-` the way
/// `build_context` derives `name_cargo`, so cargo's rules are the ones that
/// bind here — not WIT's, which apply to the id instead (see
/// [`greentic_extension_sdk_contract::extension_id`]). The two differ in
/// exactly one place that matters: cargo allows a digit-led word after the
/// first (`provider-3aigent` is a fine crate), WIT does not.
///
/// Checked here rather than left to the build, because cargo's own refusal
/// (`invalid character `3` in package name`) arrives only once the author runs
/// `gtdx build`, from a file they did not write.
///
/// # Errors
///
/// Returns an error naming the offending part when `name` would not be a valid
/// cargo package name.
pub(super) fn validate_name(name: &str) -> anyhow::Result<()> {
    let cargo_name = name.replace('.', "-");
    let fail = |why: String| -> anyhow::Result<()> {
        anyhow::bail!(
            "project name {name:?} is invalid: {why}. It becomes the cargo package \
             name {cargo_name:?}, so it must match {PROJECT_NAME_PATTERN}"
        )
    };

    if cargo_name.is_empty() {
        return fail("it is empty".to_string());
    }
    for (index, word) in cargo_name.split('-').enumerate() {
        let mut chars = word.chars();
        let Some(first) = chars.next() else {
            return fail(
                "it has an empty word — '-' and '.' must each sit between two words, so no                  leading, trailing or doubled separator"
                    .to_string(),
            );
        };
        // Only the very first character is barred from being a digit; cargo is
        // happy with `provider-3aigent`.
        if index == 0 && !first.is_ascii_lowercase() {
            return fail(if first.is_ascii_digit() {
                format!(
                    "it starts with {first:?} — a cargo package name may not start with a digit"
                )
            } else {
                format!("it starts with {first:?} — it must start with a lowercase letter a-z")
            });
        }
        if let Some(ch) = word
            .chars()
            .find(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit()))
        {
            return fail(if ch.is_whitespace() {
                format!("the word {word:?} contains whitespace")
            } else if ch == '_' {
                format!("the word {word:?} contains {ch:?} — use '-' instead of '_'")
            } else {
                format!(
                    "the word {word:?} contains {ch:?} — only lowercase letters a-z, digits                      0-9, '-' and '.' are allowed"
                )
            });
        }
    }
    Ok(())
}

fn validate_id(id: &str) -> anyhow::Result<()> {
    validate_extension_id(id).map_err(|e| anyhow::anyhow!("{e}"))
}

fn validate_version(version: &str) -> anyhow::Result<()> {
    semver::Version::parse(version)
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!("version {version:?} is not valid semver: {e}"))
}

fn validate_from_openapi(kind: Kind, from_openapi: Option<&Path>) -> anyhow::Result<()> {
    if from_openapi.is_some() && kind != Kind::Mcp {
        anyhow::bail!("--from-openapi is only valid with --kind mcp");
    }
    Ok(())
}

/// `mcp` (`wasix:mcp/router`) artifacts carry no `contributions` block at
/// all, so there is nowhere for `--with-view` to patch a view in.
fn validate_with_view(kind: Kind, with_view: bool) -> anyhow::Result<()> {
    if with_view && kind == Kind::Mcp {
        anyhow::bail!(
            "--with-view is not valid with --kind mcp: `wasix:mcp/router` artifacts \
             carry no contributions block at all"
        );
    }
    Ok(())
}

fn id_to_wit_package(id: &str) -> String {
    // greentic.demo -> greentic:demo; com.example.demo -> com-example:demo
    let mut parts: Vec<&str> = id.split('.').collect();
    let last = parts.pop().unwrap_or("ext");
    format!("{}:{}", parts.join("-"), last)
}

/// Map an embedded WIT filename to the vendored dependency directory it is
/// written into (`wit/deps/greentic/<subdir>/world.wit`).
///
/// Errors on an unmapped file rather than falling back to `extension-misc`:
/// that catch-all directory is referenced by no `Cargo.toml.tmpl`, so the
/// package was vendored somewhere nothing could find it and the scaffold
/// failed later, blaming a missing WIT package.
fn wit_package_subdir_for(filename: &str) -> anyhow::Result<&'static str> {
    let subdir = match filename {
        "extension-base.wit" => "extension-base",
        "extension-host.wit" => "extension-host",
        "extension-design.wit" => "extension-design",
        "extension-bundle.wit" => "extension-bundle",
        "extension-deploy.wit" => "extension-deploy",
        "extension-provider.wit" => "extension-provider",
        "runtime-side.wit" => "runtime-side",
        "extension-dw-composer.wit" => "dw-composer",
        "extension-addon.wit" => "extension-addon",
        other => anyhow::bail!(
            "no vendored dependency directory mapped for WIT file `{other}` — \
             add an arm to wit_package_subdir_for and reference the directory \
             from the kind's Cargo.toml.tmpl"
        ),
    };
    Ok(subdir)
}

fn now_iso8601() -> String {
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let d = civil_date(secs);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        d.year, d.month, d.day, d.hour, d.minute, d.second
    )
}

struct DateParts {
    year: u32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
}

// Civil algorithm (Howard Hinnant): u64 seconds -> Y-M-D H:M:S in UTC.
// The cast chain is mathematically bounded (days-since-epoch is far from i64
// limits; doy/mp/d/m stay well within u32).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::unreadable_literal
)]
fn civil_date(mut secs: u64) -> DateParts {
    let second = (secs % 60) as u32;
    secs /= 60;
    let minute = (secs % 60) as u32;
    secs /= 60;
    let hour = (secs % 24) as u32;
    secs /= 24;
    // Days since 1970-01-01 -> Y-M-D via civil algorithm (Howard Hinnant).
    let mut days = secs as i64;
    days += 719_468;
    let era = if days >= 0 {
        days / 146_097
    } else {
        (days - 146_096) / 146_097
    };
    let doe = (days - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = (y + i64::from(m <= 2)) as u32;
    DateParts {
        year,
        month: m as u32,
        day: d as u32,
        hour,
        minute,
        second,
    }
}

fn print_checks(checks: &[Check]) {
    for c in checks {
        match c {
            Check::Pass { name, detail } => println!("  ✓ {name}: {detail}"),
            Check::Warn { name, hint } => println!("  ! {name}: {hint}"),
            Check::Fail { name, hint } => eprintln!("  ✗ {name}: {hint}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{kind_wit_ref, oci_ref_digest};

    const HEX: &str = "461c6a68b1c0d4e5f60718293a4b5c6d7e8f90112233445566778899aabbccdd";

    /// `addon` resolves through the same generic lookup as `design`/`bundle`/
    /// `deploy`/`provider` — no special-casing needed, since `kind_wit_ref`
    /// reads the version straight off the embedded `extension-addon.wit`.
    #[test]
    fn kind_wit_ref_resolves_addon() {
        assert_eq!(kind_wit_ref("addon"), "greentic:extension-addon@0.1.0");
    }

    #[test]
    fn extracts_a_pinned_digest() {
        assert_eq!(
            oci_ref_digest(&format!("oci://ghcr.io/org/component-x@sha256:{HEX}")),
            Some(HEX)
        );
    }

    #[test]
    fn rejects_references_that_pin_nothing_usable() {
        // A tag is not a digest, a different algorithm is not sha256, and a
        // truncated or uppercase hex string would not survive schema
        // validation — each must fall back to the placeholder rather than be
        // written out as if the reference had pinned it.
        for reference in [
            "oci://ghcr.io/org/component-x:1.2.3",
            "oci://ghcr.io/org/component-x",
            &format!("oci://ghcr.io/org/component-x@sha512:{HEX}"),
            &format!("oci://ghcr.io/org/component-x@sha256:{}", &HEX[..40]),
            &format!(
                "oci://ghcr.io/org/component-x@sha256:{}",
                HEX.to_uppercase()
            ),
        ] {
            assert_eq!(oci_ref_digest(reference), None, "should reject {reference}");
        }
    }
}

#[cfg(test)]
mod wit_subdir_tests {
    use super::wit_package_subdir_for;

    /// Every embedded WIT file must have an explicit dependency directory.
    /// The old `_ => "extension-misc"` fallback put unmapped packages in a
    /// directory no Cargo.toml.tmpl references, so the scaffold built nothing
    /// and blamed a missing WIT package rather than a missing mapping.
    #[test]
    fn every_embedded_wit_file_is_mapped() {
        for file in crate::scaffold::embedded::wit_files() {
            let subdir = wit_package_subdir_for(file.name)
                .unwrap_or_else(|e| panic!("{} is unmapped: {e}", file.name));
            assert_ne!(
                subdir, "extension-misc",
                "{} still resolves to the old catch-all",
                file.name
            );
        }
    }

    #[test]
    fn an_unmapped_wit_file_is_an_error() {
        let err = wit_package_subdir_for("extension-nonexistent.wit")
            .expect_err("an unmapped wit file must error");
        assert!(
            err.to_string().contains("extension-nonexistent.wit"),
            "the error should name the file, got: {err}"
        );
    }
}
