use std::{
    collections::BTreeMap,
    fs,
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
pub struct Args {
    /// Project folder name (kebab-case). Also default id suffix.
    pub name: String,

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

    /// Skip interactive prompts
    #[arg(short = 'y', long)]
    pub yes: bool,

    /// Node type ID (defaults to derived suffix of --name).
    #[arg(long)]
    pub node_type_id: Option<String>,

    /// Display label for the node (defaults to humanized --name).
    #[arg(long)]
    pub label: Option<String>,
}

pub fn run(args: &Args, _home: &Path) -> anyhow::Result<()> {
    let target = args
        .dir
        .clone()
        .unwrap_or_else(|| PathBuf::from(&args.name));
    let id = args
        .id
        .clone()
        .unwrap_or_else(|| format!("com.example.{}", args.name));
    let author = args.author.clone().unwrap_or_else(detect_git_author);
    validate_id(&id)?;
    validate_version(&args.version)?;

    run_preflight(&target, args.force)?;
    prepare_target(&target, args.force)?;

    let ctx = build_context(args, &id, &author);
    let mut files_written = render_templates(&ctx, args.kind.as_str(), &target)?;
    files_written += write_wit_and_lock(args.kind.as_str(), &target)?;

    make_scripts_executable(&target)?;
    run_git_init(&target, args.no_git);

    print_summary(args.kind.as_str(), &target, files_written);
    Ok(())
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

fn build_context(args: &Args, id: &str, author: &str) -> Context {
    let mut ctx = Context::new();
    ctx.set("name", args.name.clone());
    let name_cargo = args.name.replace('.', "-");
    ctx.set("name_cargo", &name_cargo);
    ctx.set("kind", args.kind.as_str());
    // Assumes ASCII kebab-case `name`; non-ASCII or all-uppercase input may produce odd labels.
    let derived_id = args
        .name
        .split('.')
        .next_back()
        .unwrap_or(&args.name)
        .to_string();
    let node_type_id = args
        .node_type_id
        .clone()
        .unwrap_or_else(|| derived_id.clone());
    let label = args.label.clone().unwrap_or_else(|| {
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
    ctx.set("id", id);
    ctx.set("id_wit", id_to_wit_package(id));
    ctx.set("version", &args.version);
    ctx.set("author", author);
    ctx.set("license", &args.license);
    ctx.set("contract_version", CONTRACT_VERSION);
    ctx
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

fn write_wit_and_lock(kind: &str, target: &Path) -> anyhow::Result<usize> {
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
        generated_by: format!("gtdx {CONTRACT_VERSION}"),
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

fn is_reverse_dns(id: &str) -> bool {
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
