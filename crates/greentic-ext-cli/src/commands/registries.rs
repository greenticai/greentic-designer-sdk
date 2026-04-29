use std::path::Path;

use clap::{Args as ClapArgs, Subcommand};
use greentic_ext_registry::config::RegistryEntry;

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
            cfg.registries.push(RegistryEntry {
                name: name.clone(),
                url,
                token_env,
            });
            super::save_config(home, &cfg)?;
            println!("✓ added {name}");
        }
        Op::Remove { name } => {
            cfg.registries.retain(|r| r.name != name);
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
