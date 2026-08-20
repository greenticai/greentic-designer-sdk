use std::collections::HashMap;
use std::path::Path;

use clap::Args as ClapArgs;
use greentic_extension_sdk_registry::lifecycle::{InstallOptions, TrustPolicy};
use greentic_extension_sdk_registry::storage::Storage;
use greentic_extension_sdk_registry::update::{UpdateStatus, check_updates, upgrade};
use greentic_extension_sdk_state::ExtensionState;

use super::{ALL_KINDS, scan_installed};

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Extension id to update (omit and pass --all to update everything)
    pub target: Option<String>,
    /// Update every installed extension that has an update available
    #[arg(long)]
    pub all: bool,
    /// Registry name from config (defaults to [default].registry)
    #[arg(long)]
    pub registry: Option<String>,
    /// Skip permission prompt
    #[arg(long, short = 'y')]
    pub yes: bool,
}

pub async fn run(args: Args, home: &Path) -> anyhow::Result<()> {
    if args.target.is_none() && !args.all {
        anyhow::bail!("specify an extension id or pass --all");
    }

    let _cfg = super::load_config(home)?;
    let storage = Storage::new(home);
    let installed = scan_installed(&storage, &ALL_KINDS)?;

    if installed.is_empty() {
        println!("Nothing to update: no extensions installed.");
        return Ok(());
    }

    // Shared resolver: honours `gtdx login`'s stored credentials and the
    // built-in greentic-store URL, neither of which the inline lookup did.
    let reg = super::resolve_store_registry(args.registry.as_deref(), home)?;

    let state = ExtensionState::load(home).unwrap_or_default();
    let mut constraints: HashMap<String, String> = HashMap::new();
    for ext in &installed {
        constraints.insert(ext.id.clone(), state.constraint_for(&ext.id).to_string());
    }

    let triples: Vec<_> = installed
        .iter()
        .map(|e| (e.kind, e.id.clone(), e.version.clone()))
        .collect();
    let updates = check_updates(&reg, &triples, &constraints).await;

    let opts = InstallOptions {
        trust_policy: TrustPolicy::Normal,
        accept_permissions: args.yes,
        force: false,
    };

    let mut did_any = false;
    let mut attempted = 0usize;
    let mut failed = 0usize;
    for u in &updates {
        if let Some(want) = args.target.as_deref()
            && want != u.id
        {
            continue;
        }
        let UpdateStatus::UpdateAvailable { target, .. } = &u.status else {
            continue;
        };
        did_any = true;
        attempted += 1;
        let target = target.clone();
        match upgrade(&storage, &reg, u.kind, &u.id, &u.current, &target, opts).await {
            Ok(()) => {
                println!("updated {}@{} -> {}", u.id, u.current, target);
                let id = u.id.clone();
                ExtensionState::update(home, |s| {
                    if let Some(p) = s.default.policies.get_mut(&id) {
                        p.last_failed = None;
                    }
                })
                .ok();
            }
            Err(e) => {
                failed += 1;
                eprintln!("failed to update {}: {e}", u.id);
                let id = u.id.clone();
                let reason = e.to_string();
                ExtensionState::update(home, |s| s.record_failed(&id, &target, &reason)).ok();
            }
        }
    }

    if !did_any {
        println!("Nothing to update: all selected extensions are up to date.");
        return Ok(());
    }
    println!("{} updated, {failed} failed", attempted - failed);
    if failed > 0 {
        // Every failure was printed above but the command still returned
        // Ok(()), so `gtdx update --all -y` could fail on every extension and
        // leave a CI pipeline green.
        anyhow::bail!("{failed} of {attempted} update(s) failed; see the errors above");
    }
    Ok(())
}
