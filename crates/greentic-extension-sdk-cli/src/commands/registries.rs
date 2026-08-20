use std::path::Path;

use clap::{Args as ClapArgs, Subcommand};
use greentic_extension_sdk_registry::config::RegistryEntry;

#[derive(ClapArgs, Debug)]
pub struct Args {
    #[command(subcommand)]
    pub op: Op,
}

#[derive(Subcommand, Debug)]
pub enum Op {
    /// List configured registries
    List,
    /// Add a registry
    Add {
        /// Local alias for the registry
        name: String,
        /// Base URL, e.g. `https://store.greentic.cloud`
        url: String,
        #[arg(long)]
        token_env: Option<String>,
    },
    /// Remove a registry
    Remove { name: String },
    /// Set default registry
    SetDefault { name: String },
}

pub fn run(args: Args, home: &Path) -> anyhow::Result<()> {
    let mut cfg = super::load_config(home)?;
    match args.op {
        Op::List => {
            println!("default: {}", cfg.default.registry);
            for r in &cfg.registries {
                println!("  {}  {}", r.name, r.url);
            }
        }
        Op::Add {
            name,
            url,
            token_env,
        } => {
            // Duplicates were silently accepted, and every `find()` then picked
            // the first — so a second `add` looked like it worked and changed
            // nothing.
            if let Some(existing) = cfg.registries.iter().find(|r| r.name == name) {
                anyhow::bail!(
                    "registry '{name}' already exists ({}). Remove it first with: \
                     gtdx registries remove {name}",
                    existing.url
                );
            }
            cfg.registries.push(RegistryEntry {
                name: name.clone(),
                url,
                token_env,
            });
            super::save_config(home, &cfg)?;
            println!("✓ added {name}");
            println!("  Set it as default with: gtdx registries set-default {name}");
        }
        Op::Remove { name } => {
            // `retain` cannot fail, so this printed "✓ removed ghost" for a
            // registry that was never configured.
            let before = cfg.registries.len();
            cfg.registries.retain(|r| r.name != name);
            if cfg.registries.len() == before {
                anyhow::bail!(
                    "no registry named '{name}'. Configured: [{}]",
                    cfg.registries
                        .iter()
                        .map(|r| r.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            super::save_config(home, &cfg)?;
            println!("✓ removed {name}");
        }
        Op::SetDefault { name } => {
            if !cfg.registries.iter().any(|r| r.name == name) {
                return Err(anyhow::anyhow!(
                    "registry '{name}' not configured. Add it with: gtdx registries add {name} <url>"
                ));
            }
            cfg.default.registry.clone_from(&name);
            super::save_config(home, &cfg)?;
            println!("✓ default = {name}");
        }
    }
    Ok(())
}
