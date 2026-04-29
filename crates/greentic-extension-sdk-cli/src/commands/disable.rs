//! `gtdx disable <id>[@<version>]` — set an installed extension as disabled.
//!
//! Warns (does NOT block) when other installed extensions declare a
//! `capabilities.required` entry that matches a `capabilities.offered`
//! entry from the target. Cascade resolution is intentionally out of
//! scope for MVP.

use anyhow::{Context, Result};
use clap::Args;
use greentic_extension_sdk_state::ExtensionState;
use std::path::Path;

use super::enable::{installed_versions, parse_target, verify_installed};

#[derive(Debug, Args)]
pub struct DisableArgs {
    /// Extension id, optionally with @version (e.g. greentic.foo@0.1.0).
    pub target: String,
}

pub fn run(args: &DisableArgs, home: &Path) -> Result<()> {
    let (id, version) = parse_target(&args.target, home)?;
    verify_installed(home, &id, &version)?;

    warn_dependents(home, &id)?;

    let mut state = ExtensionState::load(home).context("loading state")?;
    state.set_enabled(&id, &version, false);
    state.save_atomic(home).context("saving state")?;

    tracing::info!(ext_id = %id, version = %version, action = "disable", "extension state changed");
    println!("Disabled: {id}@{version} (designer will reload)");
    Ok(())
}

fn warn_dependents(home: &Path, target_id: &str) -> Result<()> {
    let target_offered = read_offered_capabilities(home, target_id)?;
    if target_offered.is_empty() {
        return Ok(());
    }

    for kind in ["design", "deploy", "bundle", "provider"] {
        let kind_dir = home.join("extensions").join(kind);
        if !kind_dir.exists() {
            continue;
        }
        for entry in std::fs::read_dir(&kind_dir)? {
            let entry = entry?;
            let describe_path = entry.path().join("describe.json");
            if !describe_path.exists() {
                continue;
            }
            let describe: serde_json::Value = match std::fs::read_to_string(&describe_path) {
                Ok(s) => match serde_json::from_str(&s) {
                    Ok(v) => v,
                    Err(_) => continue,
                },
                Err(_) => continue,
            };
            let id = describe["metadata"]["id"].as_str().unwrap_or("");
            if id == target_id {
                continue;
            }
            let required = describe["capabilities"]["required"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            for cap in required {
                if let Some(cap_str) = cap.as_str()
                    && target_offered.iter().any(|c| c == cap_str)
                {
                    eprintln!(
                        "warning: extension {id} requires capability '{cap_str}' from {target_id}. Disabling may break it."
                    );
                }
            }
        }
    }
    Ok(())
}

fn read_offered_capabilities(home: &Path, target_id: &str) -> Result<Vec<String>> {
    let versions = installed_versions(home, target_id)?;
    if versions.is_empty() {
        return Ok(vec![]);
    }
    for kind in ["design", "deploy", "bundle", "provider"] {
        for v in &versions {
            let path = home
                .join("extensions")
                .join(kind)
                .join(format!("{target_id}-{v}"))
                .join("describe.json");
            if !path.exists() {
                continue;
            }
            let describe: serde_json::Value = match std::fs::read_to_string(&path) {
                Ok(s) => match serde_json::from_str(&s) {
                    Ok(v) => v,
                    Err(_) => continue,
                },
                Err(_) => continue,
            };
            return Ok(describe["capabilities"]["offered"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|v| v.as_str().map(std::string::ToString::to_string))
                .collect());
        }
    }
    Ok(vec![])
}
