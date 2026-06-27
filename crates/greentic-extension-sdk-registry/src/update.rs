//! Pure version-resolution for extension updates: given the installed version,
//! the versions the registry offers, and a Cargo-like semver constraint,
//! classify whether an update is available.

use semver::{Op, Version, VersionReq};
use serde::Serialize;

/// Outcome of comparing an installed extension against the registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum UpdateStatus {
    /// On the highest version permitted by the constraint.
    UpToDate,
    /// A newer permitted version exists.
    UpdateAvailable { target: String, is_major_jump: bool },
    /// Constraint is an exact pin and the installed version matches it.
    Pinned,
    /// A newer version exists but is excluded by the constraint.
    OutOfRange { latest: String, constraint: String },
    /// Could not determine status (unparsable input or registry error upstream).
    Unknown { reason: String },
}

/// Normalize loose constraint spellings to a real `VersionReq`.
fn parse_constraint(constraint: &str) -> Result<VersionReq, String> {
    let trimmed = constraint.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("latest") || trimmed == "*" {
        return Ok(VersionReq::STAR);
    }
    VersionReq::parse(trimmed).map_err(|e| format!("invalid constraint '{constraint}': {e}"))
}

/// Classify the installed version against the registry's offered versions.
#[must_use]
pub fn resolve(current: &str, available: &[String], constraint: &str) -> UpdateStatus {
    let current = match Version::parse(current) {
        Ok(v) => v,
        Err(e) => {
            return UpdateStatus::Unknown {
                reason: format!("unparsable current version '{current}': {e}"),
            };
        }
    };
    let req = match parse_constraint(constraint) {
        Ok(r) => r,
        Err(reason) => return UpdateStatus::Unknown { reason },
    };

    let parsed: Vec<Version> = available
        .iter()
        .filter_map(|s| Version::parse(s).ok())
        .collect();
    if parsed.is_empty() {
        return UpdateStatus::Unknown {
            reason: "no parsable versions from registry".to_string(),
        };
    }

    // For `OutOfRange` reporting we only consider stable (non-prerelease) versions as
    // "latest", matching the conventional expectation that a prerelease cannot
    // displace a stable current install with an out-of-range signal.
    let latest_stable = parsed.iter().filter(|v| v.pre.is_empty()).max().cloned();

    let target = parsed.iter().filter(|v| req.matches(v)).max().cloned();

    // Early exit: exact pin where current == pin supersedes any newer out-of-range version.
    if is_exact_pin(&req)
        && let Some(ref t) = target
        && t == &current
    {
        return UpdateStatus::Pinned;
    }

    match target {
        Some(t) if t > current => UpdateStatus::UpdateAvailable {
            is_major_jump: t.major > current.major,
            target: t.to_string(),
        },
        // On (or above) the highest permitted version, or constraint excludes everything.
        Some(_) | None => {
            if let Some(ref ls) = latest_stable
                && ls > &current
            {
                return UpdateStatus::OutOfRange {
                    latest: ls.to_string(),
                    constraint: constraint.to_string(),
                };
            }
            UpdateStatus::UpToDate
        }
    }
}

fn is_exact_pin(req: &VersionReq) -> bool {
    req.comparators.len() == 1 && req.comparators[0].op == Op::Exact
}

// ── Async registry helpers ────────────────────────────────────────────────────

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
    if current_version == target_version {
        return Ok(());
    }
    let installer = Installer::new(storage.clone_shallow(), registry);
    installer.install(name, target_version, opts).await?;
    // Remove the old version only after the new one is committed on disk.
    storage.remove_extension(kind, name, current_version)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn patch_update_available_under_caret() {
        let s = resolve("2.0.0", &v(&["2.0.0", "2.0.1", "2.1.0"]), "^2.0");
        assert_eq!(
            s,
            UpdateStatus::UpdateAvailable {
                target: "2.1.0".into(),
                is_major_jump: false
            }
        );
    }

    #[test]
    fn up_to_date_when_on_highest_in_range() {
        let s = resolve("2.1.0", &v(&["2.0.0", "2.1.0"]), "^2.0");
        assert_eq!(s, UpdateStatus::UpToDate);
    }

    #[test]
    fn major_bump_is_out_of_range_under_caret() {
        let s = resolve("2.1.0", &v(&["2.1.0", "3.0.0"]), "^2.0");
        assert_eq!(
            s,
            UpdateStatus::OutOfRange {
                latest: "3.0.0".into(),
                constraint: "^2.0".into()
            }
        );
    }

    #[test]
    fn major_jump_flagged_when_constraint_allows() {
        let s = resolve("2.1.0", &v(&["2.1.0", "3.0.0"]), "*");
        assert_eq!(
            s,
            UpdateStatus::UpdateAvailable {
                target: "3.0.0".into(),
                is_major_jump: true
            }
        );
    }

    #[test]
    fn exact_pin_reports_pinned() {
        let s = resolve("2.0.1", &v(&["2.0.1", "2.1.0"]), "=2.0.1");
        assert_eq!(s, UpdateStatus::Pinned);
    }

    #[test]
    fn unparsable_current_is_unknown() {
        let s = resolve("not-a-version", &v(&["1.0.0"]), "*");
        assert!(matches!(s, UpdateStatus::Unknown { .. }));
    }

    #[test]
    fn no_parsable_versions_is_unknown() {
        let s = resolve("1.0.0", &v(&["garbage"]), "*");
        assert!(matches!(s, UpdateStatus::Unknown { .. }));
    }

    #[test]
    fn prereleases_excluded_by_default() {
        // Standard semver: a plain req does not match prereleases.
        let s = resolve("1.0.0", &v(&["1.0.0", "1.1.0-rc.1"]), "*");
        assert_eq!(s, UpdateStatus::UpToDate);
    }
}
