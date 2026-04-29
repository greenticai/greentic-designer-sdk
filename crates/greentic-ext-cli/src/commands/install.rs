use std::path::{Path, PathBuf};

use clap::Args as ClapArgs;
use greentic_ext_registry::lifecycle::{InstallOptions, Installer, TrustPolicy};
use greentic_ext_registry::local::LocalFilesystemRegistry;
use greentic_ext_registry::storage::Storage;
use greentic_ext_registry::store::GreenticStoreRegistry;

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Name (from registry) or path to a local .gtxpack file
    pub target: String,
    /// Version (required for registry install, ignored for local path)
    #[arg(long)]
    pub version: Option<String>,
    /// Registry name from config (defaults to [default].registry)
    #[arg(long)]
    pub registry: Option<String>,
    /// Skip permission prompt
    #[arg(long, short = 'y')]
    pub yes: bool,
    /// Trust policy override: strict | normal | loose
    #[arg(long)]
    pub trust: Option<String>,
}

pub async fn run(args: Args, home: &Path) -> anyhow::Result<()> {
    let cfg = super::load_config(home)?;
    let storage = Storage::new(home);
    let trust_policy = parse_trust(args.trust.as_deref(), &cfg.default.trust_policy)?;

    let target_path = PathBuf::from(&args.target);
    if target_path.exists() {
        install_from_local_file(&storage, &target_path, trust_policy, args.yes).await
    } else {
        let version = args
            .version
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("--version required for registry install"))?;
        install_from_registry(&cfg, &args, &storage, version, trust_policy).await
    }
}

async fn install_from_local_file(
    storage: &Storage,
    pack_path: &Path,
    trust: TrustPolicy,
    yes: bool,
) -> anyhow::Result<()> {
    let parent = pack_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("no parent dir"))?;
    let reg = LocalFilesystemRegistry::new("cli-local", parent);

    let filename = pack_path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("bad filename"))?;
    let (name, version) = parse_pack_name(filename)?;

    let installer = Installer::new(storage.clone_shallow(), &reg);
    installer
        .install(
            &name,
            &version,
            InstallOptions {
                trust_policy: trust,
                accept_permissions: yes,
                force: false,
            },
        )
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    println!("✓ installed {name}@{version}");
    Ok(())
}

async fn install_from_registry(
    cfg: &greentic_ext_registry::config::GtdxConfig,
    args: &Args,
    storage: &Storage,
    version: &str,
    trust: TrustPolicy,
) -> anyhow::Result<()> {
    let reg_name = args.registry.as_deref().unwrap_or(&cfg.default.registry);
    let entry = cfg
        .registries
        .iter()
        .find(|r| r.name == reg_name)
        .ok_or_else(|| anyhow::anyhow!("no such registry: {reg_name}"))?;

    let token = entry
        .token_env
        .as_deref()
        .and_then(|e| std::env::var(e).ok());
    let reg = GreenticStoreRegistry::new(&entry.name, &entry.url, token);
    let installer = Installer::new(storage.clone_shallow(), &reg);
    installer
        .install(
            &args.target,
            version,
            InstallOptions {
                trust_policy: trust,
                accept_permissions: args.yes,
                force: false,
            },
        )
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    println!("✓ installed {}@{version}", args.target);
    Ok(())
}

fn parse_trust(override_val: Option<&str>, default_val: &str) -> anyhow::Result<TrustPolicy> {
    let raw = override_val.unwrap_or(default_val);
    match raw {
        "strict" => Ok(TrustPolicy::Strict),
        "normal" => Ok(TrustPolicy::Normal),
        "loose" => Ok(TrustPolicy::Loose),
        x => Err(anyhow::anyhow!("unknown trust policy: {x}")),
    }
}

fn parse_pack_name(filename: &str) -> anyhow::Result<(String, String)> {
    let stem = filename
        .strip_suffix(".gtxpack")
        .ok_or_else(|| anyhow::anyhow!("not a .gtxpack file: {filename}"))?;
    let idx = stem
        .rfind('-')
        .ok_or_else(|| anyhow::anyhow!("no version in filename: {filename}"))?;
    let (name, rest) = stem.split_at(idx);
    let version = rest.strip_prefix('-').unwrap_or(rest);
    Ok((name.into(), version.into()))
}
