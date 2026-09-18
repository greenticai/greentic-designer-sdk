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
pub mod openapi;
pub mod outdated;
pub mod publish;
pub mod registries;
pub mod search;
pub mod sign;
pub mod uninstall;
pub mod update;
pub mod validate;
pub mod verify;
pub mod yank;

use std::path::Path;

use anyhow::Result;
use greentic_extension_sdk_contract::ExtensionKind;
use greentic_extension_sdk_registry::config;
use greentic_extension_sdk_registry::config::{GREENTIC_STORE_NAME, GREENTIC_STORE_URL};
use greentic_extension_sdk_registry::credentials::Credentials;
use greentic_extension_sdk_registry::storage::Storage;
use greentic_extension_sdk_registry::store::GreenticStoreRegistry;

pub fn load_config(home: &Path) -> Result<config::GtdxConfig> {
    config::load(&home.join("config.toml")).map_err(|e| anyhow::anyhow!("config: {e}"))
}

pub fn save_config(home: &Path, cfg: &config::GtdxConfig) -> Result<()> {
    config::save(&home.join("config.toml"), cfg).map_err(|e| anyhow::anyhow!("config save: {e}"))
}

/// Resolve a bearer token for `name`: the registry entry's `token_env`
/// variable wins when set and non-empty, otherwise `~/.greentic/credentials.toml`
/// (where `gtdx login` writes).
pub fn resolve_registry_token(home: &Path, name: &str, token_env: Option<&str>) -> Option<String> {
    if let Some(var) = token_env
        && let Ok(v) = std::env::var(var)
        && !v.is_empty()
    {
        return Some(v);
    }
    let creds = Credentials::load(&home.join("credentials.toml")).ok()?;
    creds.get(name).map(str::to_string)
}

/// Build an authenticated Greentic Store client for a configured registry.
///
/// A user-configured entry always wins; otherwise the canonical
/// `greentic-store` name falls back to its built-in URL, so the public store
/// works without `gtdx registries add`.
pub fn resolve_store_registry(
    registry: Option<&str>,
    home: &Path,
) -> Result<GreenticStoreRegistry> {
    let cfg = load_config(home)?;
    let wanted = registry.unwrap_or(&cfg.default.registry);
    let (name, url, token_env) = match cfg.registries.iter().find(|r| r.name == wanted) {
        Some(entry) => (
            entry.name.clone(),
            entry.url.clone(),
            entry.token_env.clone(),
        ),
        None if wanted == GREENTIC_STORE_NAME => (
            GREENTIC_STORE_NAME.to_string(),
            GREENTIC_STORE_URL.to_string(),
            None,
        ),
        None => {
            return Err(anyhow::anyhow!(
                "no registry named '{wanted}' in {}/config.toml. Add one with: gtdx registries add {wanted} <url>",
                home.display()
            ));
        }
    };
    let token = resolve_registry_token(home, &name, token_env.as_deref());
    Ok(GreenticStoreRegistry::new(&name, &url, token)
        .with_insecure_allowed(crate::registry_security::insecure_registry_opt_in()))
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
