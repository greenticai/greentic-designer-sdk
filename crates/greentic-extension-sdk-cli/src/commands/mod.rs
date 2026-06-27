pub mod component;
pub mod dev;
pub mod disable;
pub mod doctor;
pub mod enable;
pub mod info;
pub mod install;
pub mod keygen;
pub mod lint;
pub mod list;
pub mod login;
pub mod new;
pub mod outdated;
pub mod publish;
pub mod registries;
pub mod search;
pub mod sign;
pub mod uninstall;
pub mod update;
pub mod validate;
pub mod verify;

use std::path::Path;

use anyhow::Result;
use greentic_extension_sdk_contract::ExtensionKind;
use greentic_extension_sdk_registry::config;
use greentic_extension_sdk_registry::storage::Storage;

pub fn load_config(home: &Path) -> Result<config::GtdxConfig> {
    config::load(&home.join("config.toml")).map_err(|e| anyhow::anyhow!("config: {e}"))
}

pub fn save_config(home: &Path, cfg: &config::GtdxConfig) -> Result<()> {
    config::save(&home.join("config.toml"), cfg).map_err(|e| anyhow::anyhow!("config save: {e}"))
}

/// One installed extension discovered on disk.
pub struct InstalledExt {
    pub kind: ExtensionKind,
    pub id: String,
    pub version: String,
    /// Human-readable summary from the extension's describe.json metadata.
    #[allow(dead_code)]
    pub summary: String,
}

/// All extension kinds, in display order.
pub const ALL_KINDS: [ExtensionKind; 5] = [
    ExtensionKind::Design,
    ExtensionKind::Bundle,
    ExtensionKind::Deploy,
    ExtensionKind::Provider,
    ExtensionKind::WasixMcpRouter,
];

/// Enumerate installed extensions under the given kinds by reading each
/// `<kind>/<name>-<version>/describe.json`.
pub fn scan_installed(storage: &Storage, kinds: &[ExtensionKind]) -> Result<Vec<InstalledExt>> {
    let mut out = Vec::new();
    for kind in kinds {
        let dir = storage.kind_dir(*kind);
        if !dir.exists() {
            continue;
        }
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let describe_path = entry.path().join("describe.json");
            if !describe_path.exists() {
                continue;
            }
            let bytes = std::fs::read(&describe_path)?;
            let value: serde_json::Value = serde_json::from_slice(&bytes)?;
            let d: greentic_extension_sdk_contract::DescribeJson = serde_json::from_value(value)?;
            out.push(InstalledExt {
                kind: *kind,
                id: d.metadata.id.clone(),
                version: d.metadata.version.clone(),
                summary: d.metadata.summary.default().to_string(),
            });
        }
    }
    Ok(out)
}
