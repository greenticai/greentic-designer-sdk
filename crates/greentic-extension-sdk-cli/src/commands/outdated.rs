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
    #[arg(short = 'r', long)]
    pub registry: Option<String>,

    /// Emit machine-readable JSON instead of a table
    ///
    /// The fixed-width table silently truncates ids past 40 characters, which
    /// is a trap for anything parsing it with `awk`.
    #[arg(long)]
    pub json: bool,

    /// Exit 1 when any update is available (for CI gating, like `npm outdated`)
    #[arg(long)]
    pub exit_code: bool,
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
        let reg_name = args
            .registry
            .clone()
            .unwrap_or_else(|| cfg.default.registry.clone());
        if let Ok(reg) = super::resolve_store_registry(args.registry.as_deref(), home) {
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

    let rows: Vec<(String, String, String, &str)> = updates
        .iter()
        .map(|u| {
            let (target, label) = match &u.status {
                UpdateStatus::UpToDate => ("-".to_string(), "up to date"),
                UpdateStatus::UpdateAvailable { target, .. } => {
                    (target.clone(), "update available")
                }
                UpdateStatus::Pinned => ("-".to_string(), "pinned"),
                UpdateStatus::OutOfRange { latest, .. } => (latest.clone(), "out of range"),
                UpdateStatus::Unknown { .. } => ("?".to_string(), "unknown"),
            };
            (u.id.clone(), u.current.clone(), target, label)
        })
        .collect();

    if args.json {
        let payload: Vec<serde_json::Value> = rows
            .iter()
            .map(|(id, current, target, status)| {
                serde_json::json!({
                    "id": id,
                    "current": current,
                    "target": target,
                    "status": status,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("{:<40} {:<12} {:<12} STATUS", "ID", "CURRENT", "TARGET");
        for (id, current, target, label) in &rows {
            println!("{id:<40} {current:<12} {target:<12} {label}");
        }
    }

    if args.exit_code && rows.iter().any(|(_, _, _, s)| *s == "update available") {
        // Opt-in, so the default stays a read-only informational command.
        std::process::exit(1);
    }
    Ok(())
}
