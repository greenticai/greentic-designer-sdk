//! `gtdx yank` / `gtdx unyank` — withdraw a published version, reversibly.
//!
//! The store has had this endpoint for a while; the CLI has not, which left
//! "we shipped a bad version" with no first-class answer. The workarounds were
//! a raw `curl` with a hand-copied bearer token, or `gtdx publish --force`,
//! which overwrites the bytes served under a version number consumers may have
//! pinned by sha256 and cannot be undone.

use std::path::Path;

use clap::Args as ClapArgs;
use greentic_extension_sdk_registry::store::GreenticStoreRegistry;

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Extension id, e.g. greentic.telco-x
    pub name: String,
    /// Version to withdraw
    pub version: String,
    /// Why it was withdrawn. Stored by the store and shown to anyone who
    /// inspects the version — worth the extra seconds.
    #[arg(long)]
    pub reason: Option<String>,
    /// Registry name from config (defaults to [default].registry)
    #[arg(long)]
    pub registry: Option<String>,
}

#[derive(ClapArgs, Debug)]
pub struct UnyankArgs {
    /// Extension id, e.g. greentic.telco-x
    pub name: String,
    /// Version to put back in circulation
    pub version: String,
    /// Registry name from config (defaults to [default].registry)
    #[arg(long)]
    pub registry: Option<String>,
}

pub async fn run_yank(args: Args, home: &Path) -> anyhow::Result<()> {
    let reg = super::resolve_store_registry(args.registry.as_deref(), home)?;
    reg.yank(&args.name, &args.version, args.reason.as_deref())
        .await?;
    println!(
        "yanked {}@{} — it stays downloadable for existing pins, but is hidden from the version \
         list and will never be selected as the latest version.",
        args.name, args.version
    );
    println!("undo with: gtdx unyank {} {}", args.name, args.version);
    Ok(())
}

pub async fn run_unyank(args: UnyankArgs, home: &Path) -> anyhow::Result<()> {
    let reg: GreenticStoreRegistry = super::resolve_store_registry(args.registry.as_deref(), home)?;
    reg.unyank(&args.name, &args.version).await?;
    println!(
        "unyanked {}@{} — it is installable again.",
        args.name, args.version
    );
    Ok(())
}
