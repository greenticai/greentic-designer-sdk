//! Registry-backed update helpers: enumerate installed extensions against the
//! registry and apply atomic upgrades.
//!
//! The pure version-resolution core (`resolve` + `UpdateStatus`) lives in the
//! contract-free `greentic-extension-sdk-state` crate so hosts that cannot adopt
//! the registry's contract version can still classify updates. It is re-exported
//! here so existing `greentic_extension_sdk_registry::update::{resolve,
//! UpdateStatus}` paths keep working.

pub use greentic_extension_sdk_state::update::{UpdateStatus, resolve};

use std::collections::HashMap;

use greentic_extension_sdk_contract::ExtensionKind;

use crate::error::RegistryError;
use crate::lifecycle::{InstallOptions, Installer};
use crate::registry::ExtensionRegistry;
use crate::storage::Storage;

/// One installed extension's update status against the registry.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ExtensionUpdate {
    pub id: String,
    pub kind: ExtensionKind,
    pub current: String,
    #[serde(flatten)]
    pub status: UpdateStatus,
}

/// For each installed `(kind, id, current_version)`, look up the registry's
/// versions and classify against the per-id constraint (default `"*"`).
///
/// A registry error for one extension yields `Unknown`; it never panics and
/// never returns a false `UpToDate`.
///
/// Yanked versions are not present in the list returned by `list_versions`
/// (the Greentic store filters them server-side). If a registry implementation
/// does return them, `resolve`'s caller chain ultimately hits the yanked-guard
/// in `lifecycle.rs` before any artifact is written to disk.
pub async fn check_updates<R: ExtensionRegistry + ?Sized, S: ::std::hash::BuildHasher>(
    registry: &R,
    installed: &[(ExtensionKind, String, String)],
    constraints: &HashMap<String, String, S>,
) -> Vec<ExtensionUpdate> {
    let mut out = Vec::with_capacity(installed.len());
    for (kind, id, current) in installed {
        let constraint = constraints.get(id).map_or("*", String::as_str);
        let status = match registry.list_versions(id).await {
            Ok(versions) => resolve(current, &versions, constraint),
            Err(e) => UpdateStatus::Unknown {
                reason: e.to_string(),
            },
        };
        out.push(ExtensionUpdate {
            id: id.clone(),
            kind: *kind,
            current: current.clone(),
            status,
        });
    }
    out
}

/// Install `target_version`, then remove the previous version's directory so a
/// single version per id remains on disk (deterministic restart). No-op when
/// the installed version already equals the target.
///
/// The old directory is removed **only after** the new install is committed, so
/// a failed install never leaves the extension absent from disk.
pub async fn upgrade<R: ExtensionRegistry + ?Sized>(
    storage: &Storage,
    registry: &R,
    kind: ExtensionKind,
    name: &str,
    current_version: &str,
    target_version: &str,
    opts: InstallOptions,
) -> Result<(), RegistryError> {
    // Compare by semver precedence, not by string. Two reasons:
    //
    // 1. There was no ordering guard at all, so a lower target installed and
    //    then `remove_extension(current_version)` deleted the newer install —
    //    a silent rollback.
    // 2. `1.0.0+build.1` and `1.0.0` are the same version but different text,
    //    so the string check let the function "upgrade" onto itself and then
    //    delete what it had just written.
    //
    // Build metadata is stripped before comparing: the semver *spec* says it is
    // ignored for precedence, but the `semver` crate's `Ord` does compare it
    // (`1.0.0+build.1 > 1.0.0`), which would resurrect defect 2 as a spurious
    // "downgrade".
    match (
        parse_precedence(current_version),
        parse_precedence(target_version),
    ) {
        (Some(current), Some(target)) => match target.cmp(&current) {
            std::cmp::Ordering::Equal => return Ok(()),
            std::cmp::Ordering::Less => {
                return Err(RegistryError::Storage(format!(
                    "refusing to downgrade {name} from {current_version} to {target_version}; \
                     uninstall it first if that is intended"
                )));
            }
            std::cmp::Ordering::Greater => {}
        },
        // Unparsable on either side (a hand-made install dir): fall back to the
        // old textual no-op check rather than blocking the update outright.
        _ => {
            if current_version == target_version {
                return Ok(());
            }
        }
    }
    let installer = Installer::new(storage.clone_shallow(), registry);
    installer.install(name, target_version, opts).await?;
    // Remove the old version only after the new one is committed on disk.
    storage.remove_extension(kind, name, current_version)?;
    Ok(())
}

/// A version with build metadata stripped, for precedence comparison.
fn parse_precedence(v: &str) -> Option<semver::Version> {
    let mut parsed = semver::Version::parse(v).ok()?;
    parsed.build = semver::BuildMetadata::EMPTY;
    Some(parsed)
}
