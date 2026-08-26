use std::collections::HashMap;
use std::path::Path;

use clap::Args as ClapArgs;
use greentic_extension_sdk_registry::storage::Storage;
use greentic_extension_sdk_registry::update::{ExtensionUpdate, UpdateStatus, check_updates};
use greentic_extension_sdk_state::ExtensionState;

use greentic_extension_sdk_contract::ExtensionKind;

use super::scan_installed;

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Registry name from config (defaults to [default].registry)
    #[arg(long)]
    pub registry: Option<String>,
}

pub async fn run(args: Args, home: &Path) -> anyhow::Result<()> {
    let cfg = super::load_config(home)?;
    let storage = Storage::new(home);
    let installed = scan_installed(&storage, ExtensionKind::ALL)?;

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
    // Resolve through the shared helper so the built-in `greentic-store` URL
    // and a `gtdx login` token are both picked up. Hand-rolling the lookup
    // against `config.toml` alone reported the public store as "not
    // configured" on any home without an explicit `registries add` — which the
    // README says is never required.
    let updates: Vec<ExtensionUpdate> =
        match super::resolve_store_registry(args.registry.as_deref(), home) {
            Ok(reg) => check_updates(&reg, &triples, &constraints).await,
            Err(e) => {
                // An unresolvable registry is reported per-extension rather than
                // returned, so `outdated` still exits 0 and prints the table.
                let reg_name = args.registry.as_deref().unwrap_or(&cfg.default.registry);
                eprintln!("warning: {e}; update status is unknown.");
                triples
                    .iter()
                    .map(|(kind, id, current)| ExtensionUpdate {
                        id: id.clone(),
                        kind: *kind,
                        current: current.clone(),
                        status: UpdateStatus::Unknown {
                            reason: format!("registry '{reg_name}' could not be resolved"),
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
