use std::collections::HashMap;
use std::path::Path;

use clap::Args as ClapArgs;
use greentic_extension_sdk_registry::storage::Storage;
use greentic_extension_sdk_registry::update::{ExtensionUpdate, UpdateStatus, check_updates};
use greentic_extension_sdk_state::ExtensionState;

use super::{ALL_KINDS, scan_installed};

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Registry name from config (defaults to [default].registry)
    #[arg(long)]
    pub registry: Option<String>,
}

pub async fn run(args: Args, home: &Path) -> anyhow::Result<()> {
    let cfg = super::load_config(home)?;
    let storage = Storage::new(home);
    let installed = scan_installed(&storage, &ALL_KINDS)?;

    if installed.is_empty() {
        println!("No extensions installed.");
        return Ok(());
    }

    let state = ExtensionState::load(home).unwrap_or_default();
    let mut constraints: HashMap<String, String> = HashMap::new();
    for ext in &installed {
        constraints.insert(ext.id.clone(), state.constraint_for(&ext.id).to_string());
    }

    let triples: Vec<_> = installed
        .iter()
        .map(|e| (e.kind, e.id.clone(), e.version.clone()))
        .collect();

    // Resolve the registry entry. If it is missing (e.g. fresh home with no
    // config.toml), synthesise Unknown rows rather than returning an error so
    // the command always exits 0.
    let updates: Vec<ExtensionUpdate> = {
        let reg_name = args.registry.as_deref().unwrap_or(&cfg.default.registry);
        if let Some(entry) = cfg.registries.iter().find(|r| r.name == reg_name) {
            let token = entry
                .token_env
                .as_deref()
                .and_then(|e| std::env::var(e).ok());
            let reg = greentic_extension_sdk_registry::store::GreenticStoreRegistry::new(
                &entry.name,
                &entry.url,
                token,
            )
            .with_insecure_allowed(crate::registry_security::insecure_registry_opt_in());
            check_updates(&reg, &triples, &constraints).await
        } else {
            // No matching registry configured — report every extension as Unknown.
            eprintln!(
                "warning: registry '{reg_name}' not configured; update status is unknown. \
                 Run `gtdx registries` to configure a store."
            );
            triples
                .iter()
                .map(|(kind, id, current)| ExtensionUpdate {
                    id: id.clone(),
                    kind: *kind,
                    current: current.clone(),
                    status: UpdateStatus::Unknown {
                        reason: format!("registry '{reg_name}' not configured"),
                    },
                })
                .collect()
        }
    };

    println!("{:<40} {:<12} {:<12} STATUS", "ID", "CURRENT", "TARGET");
    for u in &updates {
        let (target, label) = match &u.status {
            UpdateStatus::UpToDate => ("-".to_string(), "up to date"),
            UpdateStatus::UpdateAvailable { target, .. } => (target.clone(), "update available"),
            UpdateStatus::Pinned => ("-".to_string(), "pinned"),
            UpdateStatus::OutOfRange { latest, .. } => (latest.clone(), "out of range"),
            UpdateStatus::Unknown { .. } => ("?".to_string(), "unknown"),
        };
        println!("{:<40} {:<12} {:<12} {}", u.id, u.current, target, label);
    }
    Ok(())
}
