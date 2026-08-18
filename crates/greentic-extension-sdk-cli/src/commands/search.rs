use std::path::Path;

use clap::Args as ClapArgs;
use greentic_extension_sdk_registry::{ExtensionRegistry, SearchQuery};

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
    // Shared with publish/yank: reads the token_env variable AND
    // ~/.greentic/credentials.toml, and falls back to the built-in
    // greentic-store URL. Search previously did neither, so `gtdx login`
    // did not authenticate a search and the canonical store 404'd as
    // "no such registry" until someone ran `gtdx registries add`.
    let reg = super::resolve_store_registry(args.registry.as_deref(), home)?;

    let kind = match args.kind.as_deref() {
        Some("design") => Some(greentic_extension_sdk_contract::ExtensionKind::Design),
        Some("bundle") => Some(greentic_extension_sdk_contract::ExtensionKind::Bundle),
        Some("deploy") => Some(greentic_extension_sdk_contract::ExtensionKind::Deploy),
        Some("mcp") => Some(greentic_extension_sdk_contract::ExtensionKind::WasixMcpRouter),
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
