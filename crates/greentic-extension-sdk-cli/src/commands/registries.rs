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
        name: String,
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
            // Apply the same rule the Store client applies at request time,
            // rather than storing anything and failing later with an error
            // that points at the request instead of the config.
            greentic_extension_sdk_registry::store::validate_registry_url(
                &url,
                crate::registry_security::insecure_registry_opt_in(),
            )?;
            let entry = RegistryEntry {
                name: name.clone(),
                url,
                token_env,
            };
            // Replace, don't append. Every lookup does `.find()` and takes the
            // first match, so a duplicate name meant the documented
            // "gtdx registries add <name> <url>  # override URL" silently kept
            // the old URL while `list` showed both.
            let verb = if let Some(existing) = cfg.registries.iter_mut().find(|r| r.name == name) {
                *existing = entry;
                "updated"
            } else {
                cfg.registries.push(entry);
                "added"
            };
            super::save_config(home, &cfg)?;
            println!("✓ {verb} {name}");
        }
        Op::Remove { name } => {
            let before = cfg.registries.len();
            cfg.registries.retain(|r| r.name != name);
            if cfg.registries.len() == before {
                // Removing nothing is a failed removal. Printing "✓ removed"
                // for a name that was never configured is a lie the caller
                // cannot distinguish from success.
                anyhow::bail!("registry {name} not configured");
            }
            super::save_config(home, &cfg)?;
            println!("✓ removed {name}");
        }
        Op::SetDefault { name } => {
            if !cfg.registries.iter().any(|r| r.name == name) {
                return Err(anyhow::anyhow!("registry {name} not configured"));
            }
            cfg.default.registry.clone_from(&name);
            super::save_config(home, &cfg)?;
            println!("✓ default = {name}");
        }
    }
    Ok(())
}
