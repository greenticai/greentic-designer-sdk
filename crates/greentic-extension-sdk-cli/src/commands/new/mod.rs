mod wizard;

use std::{
    collections::BTreeMap,
    fs,
    io::IsTerminal,
    path::{Path, PathBuf},
    time::SystemTime,
};

use clap::Args as ClapArgs;

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

    /// Extension id (reverse-DNS). Default: com.example.<name>
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

    /// Seed a `--kind mcp` extension from an OpenAPI/Swagger spec (generates the
    /// router via greentic-mcp-gen instead of the empty echo skeleton).
    #[arg(long, value_name = "SPEC")]
    pub from_openapi: Option<PathBuf>,
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
    /// `OpenAPI` spec path for `--kind mcp` seeded scaffolds.
    from_openapi: Option<PathBuf>,
}

pub fn run(args: &Args, _home: &Path) -> anyhow::Result<()> {
    let resolved = resolve(args)?;

    let target = resolved
        .dir
        .clone()
        .unwrap_or_else(|| PathBuf::from(&resolved.name));

    validate_from_openapi(resolved.kind, resolved.from_openapi.as_deref())?;

    run_preflight(&target, resolved.force)?;
    prepare_target(&target, resolved.force)?;

    let ctx = build_context(&resolved);

    let files_written = if let Some(spec) = resolved.from_openapi.as_deref() {
        scaffold_from_openapi(&ctx, spec, &target)?
    } else {
        let mut n = render_templates(&ctx, resolved.kind.as_str(), &target)?;
        n += write_wit_and_lock(resolved.kind.as_str(), &target)?;
        n
    };

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
    let id = args
        .id
        .clone()
        .unwrap_or_else(|| format!("com.example.{name}"));
    let author = args.author.clone().unwrap_or_else(detect_git_author);
    validate_id(&id)?;
    validate_version(&args.version)?;
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
        from_openapi: args.from_openapi.clone(),
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
    ctx.set("label", &label);
    let id = resolved.id.as_str();
    ctx.set("id", id);
    ctx.set("id_wit", id_to_wit_package(id));
    ctx.set("version", &resolved.version);
    ctx.set("author", &resolved.author);
    ctx.set("license", &resolved.license);
    ctx.set("contract_version", CONTRACT_VERSION);
    // `sdk_version` is the gtdx CLI / SDK crate version (the toolchain that
    // generated this scaffold). v2 describe.json templates use it for
    // `engine.*` + `compat.*` so scaffolds pin to the same SDK line that
    // produced them, independent of the WIT contract `CONTRACT_VERSION`
    // (which tracks the wit-package @version, evolves slower).
    ctx.set("sdk_version", env!("CARGO_PKG_VERSION"));
    // `runtime_ref_key` is the key used in v2 `runtime.components` map and
    // in every `nodeTypes[].runtime_ref` / `tools[].runtime_ref`. Default
    // is the last dotted segment of the extension id (matches the
    // dw-canvas + dw-composers convention we shipped in #7 / #21).
    let runtime_ref_key = id.split('.').next_back().unwrap_or(id).to_string();
    ctx.set("runtime_ref_key", &runtime_ref_key);
    ctx
}

fn scaffold_from_openapi(ctx: &Context, spec: &Path, target: &Path) -> anyhow::Result<usize> {
    use crate::scaffold::openapi;

    let bin = openapi::resolve_mcp_gen()?;
    let artifacts = openapi::run_generator(&bin, spec, target)?;

    // Render the mcp describe.json template, then patch network + secrets.
    let mut files = 1usize; // the generated wasm
    let describe_tmpl = template::load_templates_kind("mcp")
        .into_iter()
        .find(|e| e.dst_rel.ends_with("describe.json"))
        .ok_or_else(|| anyhow::anyhow!("mcp describe.json template missing"))?;
    let rendered = ctx.render(std::str::from_utf8(describe_tmpl.src_bytes)?)?;
    let authored = openapi::author_describe_json(&rendered, artifacts.meta.as_deref())?;
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

fn render_templates(ctx: &Context, kind: &str, target: &Path) -> anyhow::Result<usize> {
    let mut files_written = 0usize;
    for entry in template::load_templates_common() {
        let dst = target.join(&entry.dst_rel);
        let rendered = ctx.render(std::str::from_utf8(entry.src_bytes)?)?;
        template::write_file(&dst, rendered.as_bytes())?;
        files_written += 1;
    }
    for entry in template::load_templates_kind(kind) {
        let dst = target.join(&entry.dst_rel);
        let rendered = ctx.render(std::str::from_utf8(entry.src_bytes)?)?;
        template::write_file(&dst, rendered.as_bytes())?;
        files_written += 1;
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
    for file in embedded::files_for_kind(kind) {
        let pkg_dir = wit_package_subdir_for(file.name);
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

fn validate_id(id: &str) -> anyhow::Result<()> {
    if !is_reverse_dns(id) {
        anyhow::bail!("id must match reverse-DNS (got {id:?})");
    }
    Ok(())
}

pub(super) fn is_reverse_dns(id: &str) -> bool {
    // Reverse-DNS: [a-z][a-z0-9-]*(\.[a-z][a-z0-9-]*)+
    let parts: Vec<&str> = id.split('.').collect();
    if parts.len() < 2 {
        return false;
    }
    for p in parts {
        if p.is_empty() {
            return false;
        }
        let mut chars = p.chars();
        let first = chars.next().unwrap();
        if !first.is_ascii_lowercase() {
            return false;
        }
        if !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            return false;
        }
    }
    true
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

fn id_to_wit_package(id: &str) -> String {
    // com.example.demo -> com-example:demo
    let mut parts: Vec<&str> = id.split('.').collect();
    let last = parts.pop().unwrap_or("ext");
    format!("{}:{}", parts.join("-"), last)
}

fn wit_package_subdir_for(filename: &str) -> &'static str {
    match filename {
        "extension-base.wit" => "extension-base",
        "extension-host.wit" => "extension-host",
        "extension-design.wit" => "extension-design",
        "extension-bundle.wit" => "extension-bundle",
        "extension-deploy.wit" => "extension-deploy",
        "extension-provider.wit" => "extension-provider",
        "runtime-side.wit" => "runtime-side",
        _ => "extension-misc",
    }
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
