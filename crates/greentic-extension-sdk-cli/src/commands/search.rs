use std::path::Path;

use clap::Args as ClapArgs;
use greentic_extension_sdk_registry::{ExtensionRegistry, SearchQuery, store::GreenticStoreRegistry};

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Search term (partial-match on extension name). If omitted, lists everything the registry exposes.
    pub query: Option<String>,
    #[arg(long)]
    pub registry: Option<String>,
    #[arg(long)]
    pub kind: Option<String>,
    #[arg(long, default_value_t = 20)]
    pub limit: u32,
}

pub async fn run(args: Args, home: &Path) -> anyhow::Result<()> {
    let cfg = super::load_config(home)?;
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

    let kind = match args.kind.as_deref() {
        Some("design") => Some(greentic_extension_sdk_contract::ExtensionKind::Design),
        Some("bundle") => Some(greentic_extension_sdk_contract::ExtensionKind::Bundle),
        Some("deploy") => Some(greentic_extension_sdk_contract::ExtensionKind::Deploy),
        Some(x) => return Err(anyhow::anyhow!("unknown kind: {x}")),
        None => None,
    };

    let results = reg
        .search(SearchQuery {
            kind,
            query: args.query,
            limit: args.limit,
            ..Default::default()
        })
        .await?;
    if results.is_empty() {
        println!("(no extensions match)");
        return Ok(());
    }
    for r in results {
        println!(
            "{:<40}  {:>10}  {:?}  {}",
            r.name, r.latest_version, r.kind, r.summary
        );
    }
    Ok(())
}
