# Extension Check-Update & Manual Upgrade — Implementation Plan (Fase 0+1)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Detect "update available" per installed extension against the Greentic
Store (honoring a per-extension semver constraint) and provide a manual, atomic
upgrade with runtime-level auto-rollback — surfaced in the Designer UI and the
`gtdx` CLI.

**Architecture:** A pure `VersionResolver` plus a per-extension policy schema live
in the shared `greentic-designer-sdk` (used by all families incl. the runner).
Surfaces (Designer backend + frontend, `gtdx` CLI) consume the shared core. Upgrade
reuses the existing atomic installer; rollback is automatic because the runtime
keeps the old version loaded when a new version fails to load.

**Tech Stack:** Rust 1.95 (`semver`, `serde`, `thiserror`, `anyhow`, `tokio`,
`async-trait`, `wiremock` for tests); React + TypeScript + react-query + vitest
(Designer frontend).

**Design spec:** `docs/superpowers/specs/2026-06-27-extension-check-update-auto-upgrade-design.md`

## Global Constraints

- Rust toolchain pinned to **1.95.0** via `rust-toolchain.toml` (do not edit).
- **No `unwrap()` / `panic!()`** on production paths — use `anyhow`/`thiserror`.
- **English only** in source, comments, tests, and tracing logs.
- `#![forbid(unsafe_code)]` is the crate-root norm.
- Path deps inside this workspace carry the `version = "1.2.14-research"` suffix.
- Conventional Commits (`feat:`, `fix:`, `refactor:`, `docs:`).
- Run `bash ci/local_check.sh` from the repo root before declaring work done;
  `Cargo.lock` is committed and CI uses `--locked`.
- Designer UI: **no emoji** — use inline SVG line-icons.
- Designer crate has a source-file convention; keep new files focused. `store.rs`
  already exceeds the 500-line cap, so new backend logic goes in a **new module**
  (`store_update.rs`), re-exported from `store.rs` (mirrors `store_reconcile.rs`).
- Runtime facts that constrain the design (verified): the runtime keys loaded
  extensions by **bare id** (one version live, last-writer-wins), does **not** read
  `extensions-state.json`, has **no dry-run probe** and **no failure event**, but
  **keeps the old version loaded if a new version fails to load**.

---

### Task 1: Policy schema in `greentic-extension-sdk-state`

Add a per-extension update policy (keyed by bare `id`) alongside the existing
`enabled` map (keyed by `id@version`). Additive and backward-compatible.

**Files:**
- Modify: `crates/greentic-extension-sdk-state/src/state.rs`
- Test: `crates/greentic-extension-sdk-state/tests/policy_roundtrip.rs` (create)

**Interfaces:**
- Produces: `ExtensionPolicy { constraint: Option<String>, mode: UpdateMode, last_failed: Option<FailedUpgrade> }`, `enum UpdateMode { Manual, Auto }`, `struct FailedUpgrade { version: String, reason: String }`; methods `ExtensionState::policy(&self, id: &str) -> Option<&ExtensionPolicy>`, `constraint_for(&self, id: &str) -> &str`, `set_policy(&mut self, id: &str, policy: ExtensionPolicy)`, `record_failed(&mut self, id: &str, version: &str, reason: &str)`.

- [ ] **Step 1: Write the failing test**

Create `crates/greentic-extension-sdk-state/tests/policy_roundtrip.rs`:

```rust
use greentic_extension_sdk_state::{ExtensionPolicy, ExtensionState, UpdateMode};
use tempfile::TempDir;

#[test]
fn policy_defaults_to_star_when_absent() {
    let state = ExtensionState::default();
    assert_eq!(state.constraint_for("greentic.foo"), "*");
    assert!(state.policy("greentic.foo").is_none());
}

#[test]
fn set_policy_then_query_and_persist() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    ExtensionState::update(home, |s| {
        s.set_policy(
            "greentic.foo",
            ExtensionPolicy {
                constraint: Some("^2.0".to_string()),
                mode: UpdateMode::Manual,
                last_failed: None,
            },
        );
    })
    .unwrap();

    let reloaded = ExtensionState::load(home).unwrap();
    assert_eq!(reloaded.constraint_for("greentic.foo"), "^2.0");
    assert_eq!(reloaded.policy("greentic.foo").unwrap().mode, UpdateMode::Manual);
}

#[test]
fn record_failed_sets_marker() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    ExtensionState::update(home, |s| {
        s.record_failed("greentic.foo", "2.1.0", "component failed to load");
    })
    .unwrap();
    let reloaded = ExtensionState::load(home).unwrap();
    let lf = reloaded.policy("greentic.foo").unwrap().last_failed.clone().unwrap();
    assert_eq!(lf.version, "2.1.0");
    assert_eq!(lf.reason, "component failed to load");
}

#[test]
fn reads_legacy_schema_1_0_without_policies() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("extensions-state.json");
    std::fs::write(
        &path,
        r#"{"schema":"1.0","default":{"enabled":{"greentic.foo@1.0.0":true}}}"#,
    )
    .unwrap();
    let state = ExtensionState::load(tmp.path()).unwrap();
    // Legacy file with no `policies` key loads cleanly; policy defaults apply.
    assert_eq!(state.constraint_for("greentic.foo"), "*");
    assert!(state.is_enabled("greentic.foo", "1.0.0"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-state --test policy_roundtrip`
Expected: FAIL — `ExtensionPolicy`, `UpdateMode`, `constraint_for`, etc. do not exist.

- [ ] **Step 3: Write minimal implementation**

In `crates/greentic-extension-sdk-state/src/state.rs`, add the new types near the
top (after the existing `use` lines) and extend `ScopeState`:

```rust
/// Per-extension update policy, keyed by bare extension `id` (NOT `id@version`).
/// Additive in schema v1.1; absent in legacy v1.0 files (defaults apply).
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionPolicy {
    /// Cargo-like semver requirement: "^2.0", "~2.1", "=2.0.1", "*". `None` => "*".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraint: Option<String>,
    #[serde(default)]
    pub mode: UpdateMode,
    /// Set after a failed upgrade to suppress auto-retry of a broken version
    /// (manual retry still allowed). Honored by the Fase 3 reconciler.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_failed: Option<FailedUpgrade>,
}

/// `Manual` today; `Auto` is stored now but only honored from Fase 3.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateMode {
    #[default]
    Manual,
    Auto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailedUpgrade {
    pub version: String,
    pub reason: String,
}
```

Add `policies` to `ScopeState`:

```rust
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ScopeState {
    #[serde(default)]
    pub enabled: HashMap<String, bool>,
    /// Per-extension update policy, keyed by bare `id`. Added in schema v1.1.
    #[serde(default)]
    pub policies: HashMap<String, ExtensionPolicy>,
}
```

Bump the schema default and add the policy methods inside `impl ExtensionState`:

```rust
fn default_schema() -> String {
    "1.1".to_string()
}
```

```rust
    /// The update policy for `ext_id`, if one has been set.
    #[must_use]
    pub fn policy(&self, ext_id: &str) -> Option<&ExtensionPolicy> {
        self.default.policies.get(ext_id)
    }

    /// The semver constraint for `ext_id`, defaulting to `"*"` (track latest).
    #[must_use]
    pub fn constraint_for(&self, ext_id: &str) -> &str {
        self.default
            .policies
            .get(ext_id)
            .and_then(|p| p.constraint.as_deref())
            .unwrap_or("*")
    }

    /// Set (replace) the update policy for `ext_id`.
    pub fn set_policy(&mut self, ext_id: &str, policy: ExtensionPolicy) {
        self.default.policies.insert(ext_id.to_string(), policy);
    }

    /// Record a failed upgrade attempt for `ext_id`, preserving any existing
    /// constraint/mode.
    pub fn record_failed(&mut self, ext_id: &str, version: &str, reason: &str) {
        let entry = self.default.policies.entry(ext_id.to_string()).or_default();
        entry.last_failed = Some(FailedUpgrade {
            version: version.to_string(),
            reason: reason.to_string(),
        });
    }
```

Re-export the new types from the crate root `crates/greentic-extension-sdk-state/src/lib.rs` wherever `ExtensionState` is exported (add `ExtensionPolicy, UpdateMode, FailedUpgrade` to that `pub use`).

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p greentic-extension-sdk-state --test policy_roundtrip`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/greentic-extension-sdk-state/
git commit -m "feat(state): add per-extension update policy (constraint + mode + last_failed)"
```

---

### Task 2: `VersionResolver` (pure) in `greentic-extension-sdk-registry`

A pure function mapping `(current, available, constraint)` to an `UpdateStatus`.
No I/O — fully unit-testable.

**Files:**
- Create: `crates/greentic-extension-sdk-registry/src/update.rs`
- Modify: `crates/greentic-extension-sdk-registry/src/lib.rs` (add `pub mod update;`)
- Test: inline `#[cfg(test)]` module in `update.rs`

**Interfaces:**
- Produces: `enum UpdateStatus` (serde-tagged) and `pub fn resolve(current: &str, available: &[String], constraint: &str) -> UpdateStatus`.

- [ ] **Step 1: Write the failing test**

Create `crates/greentic-extension-sdk-registry/src/update.rs`:

```rust
//! Pure version-resolution for extension updates: given the installed version,
//! the versions the registry offers, and a Cargo-like semver constraint,
//! classify whether an update is available.

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
            UpdateStatus::UpdateAvailable { target: "2.1.0".into(), is_major_jump: false }
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
            UpdateStatus::OutOfRange { latest: "3.0.0".into(), constraint: "^2.0".into() }
        );
    }

    #[test]
    fn major_jump_flagged_when_constraint_allows() {
        let s = resolve("2.1.0", &v(&["2.1.0", "3.0.0"]), "*");
        assert_eq!(
            s,
            UpdateStatus::UpdateAvailable { target: "3.0.0".into(), is_major_jump: true }
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-registry update::`
Expected: FAIL — `resolve` is not defined.

- [ ] **Step 3: Write minimal implementation**

Add the `resolve` function to `update.rs` (above the test module):

```rust
use semver::{Op, Version, VersionReq};

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
        Err(e) => return UpdateStatus::Unknown { reason: format!("unparsable current version '{current}': {e}") },
    };
    let req = match parse_constraint(constraint) {
        Ok(r) => r,
        Err(reason) => return UpdateStatus::Unknown { reason },
    };

    let parsed: Vec<Version> = available.iter().filter_map(|s| Version::parse(s).ok()).collect();
    if parsed.is_empty() {
        return UpdateStatus::Unknown { reason: "no parsable versions from registry".to_string() };
    }
    let latest = parsed.iter().max().cloned().unwrap_or_else(|| current.clone());
    let target = parsed.iter().filter(|v| req.matches(v)).max().cloned();

    match target {
        Some(t) if t > current => UpdateStatus::UpdateAvailable {
            is_major_jump: t.major > current.major,
            target: t.to_string(),
        },
        // On (or above) the highest permitted version.
        Some(_) => {
            if latest > current {
                UpdateStatus::OutOfRange { latest: latest.to_string(), constraint: constraint.to_string() }
            } else if is_exact_pin(&req) {
                UpdateStatus::Pinned
            } else {
                UpdateStatus::UpToDate
            }
        }
        // Constraint excludes everything available.
        None => {
            if latest > current {
                UpdateStatus::OutOfRange { latest: latest.to_string(), constraint: constraint.to_string() }
            } else {
                UpdateStatus::UpToDate
            }
        }
    }
}

fn is_exact_pin(req: &VersionReq) -> bool {
    req.comparators.len() == 1 && req.comparators[0].op == Op::Exact
}
```

Add `pub mod update;` to `crates/greentic-extension-sdk-registry/src/lib.rs`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p greentic-extension-sdk-registry update::`
Expected: PASS (8 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/greentic-extension-sdk-registry/src/update.rs crates/greentic-extension-sdk-registry/src/lib.rs
git commit -m "feat(registry): add pure VersionResolver for update detection"
```

---

### Task 3: Registry async check + upgrade helpers

Add an async `check_updates` (calls `list_versions` per extension) and an
`upgrade` helper (install target, then remove the old version dir on success).

**Files:**
- Modify: `crates/greentic-extension-sdk-registry/src/update.rs`
- Test: `crates/greentic-extension-sdk-registry/tests/update_flow.rs` (create)

**Interfaces:**
- Consumes: `ExtensionRegistry` (Task uses existing `list_versions`, `fetch`), `Storage`, `Installer`, `InstallOptions`, `ExtensionKind`, `resolve` (Task 2).
- Produces: `struct ExtensionUpdate { id: String, kind: ExtensionKind, current: String, status: UpdateStatus }`; `async fn check_updates<R>(registry, installed, constraints) -> Vec<ExtensionUpdate>`; `async fn upgrade<R>(storage, registry, kind, name, current_version, target_version, opts) -> Result<(), RegistryError>`.

- [ ] **Step 1: Write the failing test**

Create `crates/greentic-extension-sdk-registry/tests/update_flow.rs`:

```rust
use std::collections::HashMap;

use greentic_extension_sdk_contract::ExtensionKind;
use greentic_extension_sdk_registry::lifecycle::{InstallOptions, TrustPolicy};
use greentic_extension_sdk_registry::local::LocalFilesystemRegistry;
use greentic_extension_sdk_registry::storage::Storage;
use greentic_extension_sdk_registry::update::{check_updates, upgrade, UpdateStatus};
use greentic_extension_sdk_testing::ExtensionFixtureBuilder;
use greentic_extension_sdk_registry::pack::pack_directory; // re-export used by existing tests
use std::path::Path;

fn publish_pack(reg_dir: &Path, name: &str, version: &str) {
    let fixture = ExtensionFixtureBuilder::new(ExtensionKind::Design, name, version)
        .offer("greentic:perm/x", "1.0.0")
        .with_wasm(b"wasm".to_vec())
        .build()
        .unwrap();
    let pack = reg_dir.join(format!("{name}-{version}.gtxpack"));
    pack_directory(fixture.root(), &pack).unwrap();
}

#[tokio::test]
async fn check_updates_reports_available() {
    let tmp = tempfile::TempDir::new().unwrap();
    let reg_dir = tmp.path().join("reg");
    std::fs::create_dir_all(&reg_dir).unwrap();
    publish_pack(&reg_dir, "greentic.foo", "1.0.0");
    publish_pack(&reg_dir, "greentic.foo", "1.1.0");
    let reg = LocalFilesystemRegistry::new("test", &reg_dir);

    let installed = vec![(ExtensionKind::Design, "greentic.foo".to_string(), "1.0.0".to_string())];
    let constraints = HashMap::new(); // defaults to "*"
    let updates = check_updates(&reg, &installed, &constraints).await;

    assert_eq!(updates.len(), 1);
    assert_eq!(
        updates[0].status,
        UpdateStatus::UpdateAvailable { target: "1.1.0".into(), is_major_jump: false }
    );
}

#[tokio::test]
async fn upgrade_installs_target_and_removes_old() {
    let tmp = tempfile::TempDir::new().unwrap();
    let home = tmp.path().join("home");
    let reg_dir = tmp.path().join("reg");
    std::fs::create_dir_all(&reg_dir).unwrap();
    publish_pack(&reg_dir, "greentic.foo", "1.0.0");
    publish_pack(&reg_dir, "greentic.foo", "1.1.0");
    let reg = LocalFilesystemRegistry::new("test", &reg_dir);
    let storage = Storage::new(&home);

    // Seed the old version on disk.
    let opts = InstallOptions { trust_policy: TrustPolicy::Loose, accept_permissions: true, force: false };
    upgrade(&storage, &reg, ExtensionKind::Design, "greentic.foo", "0.0.0", "1.0.0", opts)
        .await
        .unwrap();
    assert!(storage.extension_dir(ExtensionKind::Design, "greentic.foo", "1.0.0").exists());

    // Upgrade 1.0.0 -> 1.1.0.
    upgrade(&storage, &reg, ExtensionKind::Design, "greentic.foo", "1.0.0", "1.1.0", opts)
        .await
        .unwrap();

    assert!(storage.extension_dir(ExtensionKind::Design, "greentic.foo", "1.1.0").exists());
    assert!(!storage.extension_dir(ExtensionKind::Design, "greentic.foo", "1.0.0").exists());
}
```

Note: if `pack_directory` is not re-exported at the crate root, import it from the
path the existing `tests/lifecycle.rs` uses (check that file's `use` lines and copy
the exact import).

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-registry --test update_flow`
Expected: FAIL — `check_updates` / `upgrade` not defined.

- [ ] **Step 3: Write minimal implementation**

Append to `crates/greentic-extension-sdk-registry/src/update.rs`:

```rust
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
/// A registry error for one extension yields `Unknown`, never a panic and never
/// a false `UpToDate`.
pub async fn check_updates<R: ExtensionRegistry + ?Sized>(
    registry: &R,
    installed: &[(ExtensionKind, String, String)],
    constraints: &HashMap<String, String>,
) -> Vec<ExtensionUpdate> {
    let mut out = Vec::with_capacity(installed.len());
    for (kind, id, current) in installed {
        let constraint = constraints.get(id).map(String::as_str).unwrap_or("*");
        let status = match registry.list_versions(id).await {
            Ok(versions) => resolve(current, &versions, constraint),
            Err(e) => UpdateStatus::Unknown { reason: e.to_string() },
        };
        out.push(ExtensionUpdate { id: id.clone(), kind: *kind, current: current.clone(), status });
    }
    out
}

/// Install `target_version`, then remove the previous version's directory so a
/// single version per id remains on disk (deterministic restart). No-op when the
/// installed version already equals the target. This is the on-disk swap used by
/// the CLI; the Designer adds a runtime load-probe on top (see its handler).
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
    // Only remove the old dir after the new one is committed.
    storage.remove_extension(kind, name, current_version)?;
    Ok(())
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p greentic-extension-sdk-registry --test update_flow`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/greentic-extension-sdk-registry/
git commit -m "feat(registry): add check_updates + on-disk upgrade helpers"
```

---

### Task 4: Shared CLI installed-scan + `gtdx outdated`

Extract a reusable installed-extension scan (the `read_dir` pattern from
`list.rs`) and add the `outdated` subcommand.

**Files:**
- Modify: `crates/greentic-extension-sdk-cli/src/commands/mod.rs` (add `scan_installed` + `InstalledExt`)
- Create: `crates/greentic-extension-sdk-cli/src/commands/outdated.rs`
- Modify: `crates/greentic-extension-sdk-cli/src/main.rs` (register subcommand)
- Test: `crates/greentic-extension-sdk-cli/tests/cli_outdated.rs` (create)

**Interfaces:**
- Consumes: `Storage`, `ExtensionKind`, `DescribeJson`, `GreenticStoreRegistry`, `ExtensionState`, `check_updates`, `UpdateStatus`.
- Produces: `pub struct InstalledExt { pub kind: ExtensionKind, pub id: String, pub version: String, pub summary: String }`, `pub fn scan_installed(storage: &Storage, kinds: &[ExtensionKind]) -> anyhow::Result<Vec<InstalledExt>>`.

- [ ] **Step 1: Write the failing test**

Create `crates/greentic-extension-sdk-cli/tests/cli_outdated.rs`:

```rust
use std::process::Command;

use greentic_extension_sdk_contract::ExtensionKind;
use greentic_extension_sdk_testing::ExtensionFixtureBuilder;
use greentic_extension_sdk_registry::pack::pack_directory;
use tempfile::TempDir;

fn gtdx_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_gtdx"))
}

#[test]
fn outdated_runs_with_no_extensions_installed() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();

    let output = Command::new(gtdx_bin())
        .arg("--home")
        .arg(&home)
        .arg("outdated")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn outdated_lists_installed_extension_as_unknown_without_registry() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    // Install a fixture extension directly into the design kind dir.
    let dir = home.join("extensions/design/greentic.foo-1.0.0");
    std::fs::create_dir_all(&dir).unwrap();
    let fixture = ExtensionFixtureBuilder::new(ExtensionKind::Design, "greentic.foo", "1.0.0")
        .offer("greentic:perm/x", "1.0.0")
        .with_wasm(b"wasm".to_vec())
        .build()
        .unwrap();
    let pack = tmp.path().join("p.gtxpack");
    pack_directory(fixture.root(), &pack).unwrap();
    // Unzip the fixture's describe.json into the install dir for scanning.
    std::fs::copy(fixture.describe_path.clone(), dir.join("describe.json")).unwrap();

    let output = Command::new(gtdx_bin())
        .arg("--home")
        .arg(&home)
        .arg("outdated")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert!(stdout.contains("greentic.foo"), "stdout was: {stdout}");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-cli --test cli_outdated`
Expected: FAIL — `outdated` subcommand not recognized (`gtdx` exits non-zero).

- [ ] **Step 3: Write minimal implementation**

Add the shared scan to `crates/greentic-extension-sdk-cli/src/commands/mod.rs`:

```rust
use greentic_extension_sdk_contract::ExtensionKind;
use greentic_extension_sdk_registry::storage::Storage;

/// One installed extension discovered on disk.
pub struct InstalledExt {
    pub kind: ExtensionKind,
    pub id: String,
    pub version: String,
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
pub fn scan_installed(storage: &Storage, kinds: &[ExtensionKind]) -> anyhow::Result<Vec<InstalledExt>> {
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
```

Create `crates/greentic-extension-sdk-cli/src/commands/outdated.rs`:

```rust
use std::collections::HashMap;
use std::path::Path;

use clap::Args as ClapArgs;
use greentic_extension_sdk_registry::store::GreenticStoreRegistry;
use greentic_extension_sdk_registry::storage::Storage;
use greentic_extension_sdk_registry::update::{check_updates, UpdateStatus};
use greentic_extension_sdk_state::ExtensionState;

use super::{scan_installed, ALL_KINDS};

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

    let reg_name = args.registry.as_deref().unwrap_or(&cfg.default.registry);
    let entry = cfg
        .registries
        .iter()
        .find(|r| r.name == reg_name)
        .ok_or_else(|| anyhow::anyhow!("no such registry: {reg_name}"))?;
    let token = entry.token_env.as_deref().and_then(|e| std::env::var(e).ok());
    let reg = GreenticStoreRegistry::new(&entry.name, &entry.url, token)
        .with_insecure_allowed(crate::registry_security::insecure_registry_opt_in());

    let state = ExtensionState::load(home).unwrap_or_default();
    let mut constraints = HashMap::new();
    for ext in &installed {
        constraints.insert(ext.id.clone(), state.constraint_for(&ext.id).to_string());
    }

    let triples: Vec<_> = installed
        .iter()
        .map(|e| (e.kind, e.id.clone(), e.version.clone()))
        .collect();
    let updates = check_updates(&reg, &triples, &constraints).await;

    println!("{:<40} {:<12} {:<12} {}", "ID", "CURRENT", "TARGET", "STATUS");
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
```

Register in `crates/greentic-extension-sdk-cli/src/main.rs`: add `Outdated(commands::outdated::Args)` to the `Command` enum (with a doc comment `/// Check installed extensions for available updates`) and the match arm `Command::Outdated(args) => commands::outdated::run(args, &home).await,`. Add `pub mod outdated;` to `commands/mod.rs`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p greentic-extension-sdk-cli --test cli_outdated`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/greentic-extension-sdk-cli/
git commit -m "feat(cli): add gtdx outdated + shared installed scan"
```

---

### Task 5: `gtdx update`

Add the `update` subcommand: upgrade a named extension or `--all` to the highest
permitted version, recording `last_failed` on error.

**Files:**
- Create: `crates/greentic-extension-sdk-cli/src/commands/update.rs`
- Modify: `crates/greentic-extension-sdk-cli/src/main.rs` (register subcommand)
- Modify: `crates/greentic-extension-sdk-cli/src/commands/mod.rs` (`pub mod update;`)
- Test: `crates/greentic-extension-sdk-cli/tests/cli_update.rs` (create)

**Interfaces:**
- Consumes: `scan_installed`, `ALL_KINDS` (Task 4), `check_updates`, `upgrade`, `UpdateStatus`, `ExtensionState`, `InstallOptions`, `TrustPolicy`, `GreenticStoreRegistry`.

- [ ] **Step 1: Write the failing test**

Create `crates/greentic-extension-sdk-cli/tests/cli_update.rs`:

```rust
use std::process::Command;

use tempfile::TempDir;

fn gtdx_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_gtdx"))
}

#[test]
fn update_with_nothing_installed_is_a_noop_success() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();

    let output = Command::new(gtdx_bin())
        .arg("--home")
        .arg(&home)
        .arg("update")
        .arg("--all")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.to_lowercase().contains("nothing"), "stdout: {stdout}");
}

#[test]
fn update_requires_target_or_all_flag() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();

    let output = Command::new(gtdx_bin())
        .arg("--home")
        .arg(&home)
        .arg("update")
        .output()
        .unwrap();

    assert!(!output.status.success(), "expected failure when neither target nor --all is given");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-cli --test cli_update`
Expected: FAIL — `update` subcommand not recognized.

- [ ] **Step 3: Write minimal implementation**

Create `crates/greentic-extension-sdk-cli/src/commands/update.rs`:

```rust
use std::collections::HashMap;
use std::path::Path;

use clap::Args as ClapArgs;
use greentic_extension_sdk_registry::lifecycle::{InstallOptions, TrustPolicy};
use greentic_extension_sdk_registry::store::GreenticStoreRegistry;
use greentic_extension_sdk_registry::storage::Storage;
use greentic_extension_sdk_registry::update::{check_updates, upgrade, UpdateStatus};
use greentic_extension_sdk_state::ExtensionState;

use super::{scan_installed, ALL_KINDS};

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
    let cfg = super::load_config(home)?;
    let storage = Storage::new(home);
    let installed = scan_installed(&storage, &ALL_KINDS)?;
    if installed.is_empty() {
        println!("Nothing to update: no extensions installed.");
        return Ok(());
    }

    let reg_name = args.registry.as_deref().unwrap_or(&cfg.default.registry);
    let entry = cfg
        .registries
        .iter()
        .find(|r| r.name == reg_name)
        .ok_or_else(|| anyhow::anyhow!("no such registry: {reg_name}"))?;
    let token = entry.token_env.as_deref().and_then(|e| std::env::var(e).ok());
    let reg = GreenticStoreRegistry::new(&entry.name, &entry.url, token)
        .with_insecure_allowed(crate::registry_security::insecure_registry_opt_in());

    let state = ExtensionState::load(home).unwrap_or_default();
    let mut constraints = HashMap::new();
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
        match upgrade(&storage, &reg, u.kind, &u.id, &u.current, target, opts).await {
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
                eprintln!("failed to update {}: {e}", u.id);
                let (id, target) = (u.id.clone(), target.clone());
                let reason = e.to_string();
                ExtensionState::update(home, |s| s.record_failed(&id, &target, &reason)).ok();
            }
        }
    }
    if !did_any {
        println!("Nothing to update: all selected extensions are up to date.");
    }
    Ok(())
}
```

Register in `main.rs`: add `Update(commands::update::Args)` to the `Command` enum
(doc comment `/// Update installed extensions to the latest permitted version`) and
the arm `Command::Update(args) => commands::update::run(args, &home).await,`. Add
`pub mod update;` to `commands/mod.rs`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p greentic-extension-sdk-cli --test cli_update`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/greentic-extension-sdk-cli/
git commit -m "feat(cli): add gtdx update with last_failed recording"
```

---

> The remaining tasks are in the **`greentic-designer`** repo (separate git repo).
> `cd ../greentic-designer` and branch from `research` before starting. Add the SDK
> crates as dependencies if missing (Task 6 Step 3).

### Task 6: Designer backend — `GET /api/store/installed/updates`

A new endpoint that returns update status per installed extension. Kept separate
from `list_installed` so the installed list stays fast (network check is lazy).

**Files:**
- Create: `greentic-designer/src/ui/routes/store_update.rs`
- Modify: `greentic-designer/src/ui/routes/store.rs` (re-export, mirror `store_reconcile`)
- Modify: `greentic-designer/src/ui/routes/router_build.rs` (register route)
- Modify: `greentic-designer/Cargo.toml` (ensure SDK deps present)
- Test: `greentic-designer/tests/store_update.rs` (create)

**Interfaces:**
- Consumes: `AppState` (`store_url: String`, `extension_runtime`), `update::resolve`, `update::UpdateStatus`, `ExtensionState`.
- Produces: `pub async fn check_installed_updates(State<Arc<AppState>>) -> impl IntoResponse` returning `[{ "id", "current", "status", ... }]`; helper `pub fn installed_triples(home: &Path) -> Vec<(String, String, String)>` returning `(kind_dir, id, version)`.

- [ ] **Step 1: Write the failing test**

Create `greentic-designer/tests/store_update.rs`:

```rust
use greentic_designer::ui::routes::store::installed_triples;

#[test]
fn installed_triples_reads_describe_across_kinds() {
    let tmp = tempfile::TempDir::new().unwrap();
    let home = tmp.path();
    let dir = home.join(".greentic/extensions/design/greentic.foo-1.0.0");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("describe.json"),
        r#"{"kind":"DesignExtension","metadata":{"id":"greentic.foo","name":"Foo","version":"1.0.0"}}"#,
    )
    .unwrap();

    let triples = installed_triples(home);
    assert_eq!(triples.len(), 1);
    assert_eq!(triples[0], ("design".to_string(), "greentic.foo".to_string(), "1.0.0".to_string()));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-designer --test store_update`
Expected: FAIL — `installed_triples` not defined / not re-exported.

- [ ] **Step 3: Write minimal implementation**

Ensure `greentic-designer/Cargo.toml` `[dependencies]` includes (add if missing,
matching the version other SDK deps use):

```toml
greentic-extension-sdk-registry = { version = "1.2.14-research" }
greentic-extension-sdk-state = { version = "1.2.14-research" }
```

Create `greentic-designer/src/ui/routes/store_update.rs`:

```rust
//! Update-check endpoint: classify installed extensions against the store.
//! Split from `store.rs` (which already exceeds the 500-line cap), re-exported
//! via `pub use` in `store.rs` so the public path is preserved.

use std::path::Path;
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use greentic_extension_sdk_registry::update::{resolve, UpdateStatus};
use greentic_extension_sdk_state::ExtensionState;
use serde_json::{json, Value};

use crate::ui::state::AppState;

const KIND_DIRS: [&str; 5] = ["design", "bundle", "deploy", "provider", "mcp"];

/// Enumerate installed extensions as `(kind_dir, id, version)` triples by reading
/// each `<home>/.greentic/extensions/<kind>/<dir>/describe.json`.
#[must_use]
pub fn installed_triples(home: &Path) -> Vec<(String, String, String)> {
    let root = home.join(".greentic").join("extensions");
    let mut out = Vec::new();
    for kind in KIND_DIRS {
        let kind_dir = root.join(kind);
        let Ok(entries) = std::fs::read_dir(&kind_dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let describe_path = entry.path().join("describe.json");
            let Ok(bytes) = std::fs::read(&describe_path) else {
                continue;
            };
            let Ok(describe) = serde_json::from_slice::<Value>(&bytes) else {
                continue;
            };
            let id = describe["metadata"]["id"].as_str().unwrap_or_default();
            let version = describe["metadata"]["version"].as_str().unwrap_or_default();
            if !id.is_empty() && !version.is_empty() {
                out.push((kind.to_string(), id.to_string(), version.to_string()));
            }
        }
    }
    out
}

/// GET /api/store/installed/updates — per-extension update status.
pub async fn check_installed_updates(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let Some(home) = dirs::home_dir() else {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":"home_dir_unavailable"})))
            .into_response();
    };
    let installed = installed_triples(&home);
    let ext_state = ExtensionState::load(&home).unwrap_or_default();
    let client = reqwest::Client::new();

    let mut results = Vec::with_capacity(installed.len());
    for (kind, id, current) in &installed {
        let constraint = ext_state.constraint_for(id).to_string();
        let status = fetch_versions(&client, &state.store_url, id)
            .await
            .map_or_else(
                |reason| UpdateStatus::Unknown { reason },
                |versions| resolve(current, &versions, &constraint),
            );
        let mut obj = serde_json::to_value(&status).unwrap_or_else(|_| json!({"status":"unknown"}));
        if let Value::Object(map) = &mut obj {
            map.insert("id".into(), json!(id));
            map.insert("kind".into(), json!(kind));
            map.insert("current".into(), json!(current));
            map.insert("constraint".into(), json!(constraint));
        }
        results.push(obj);
    }
    (StatusCode::OK, Json(json!(results))).into_response()
}

/// GET the store's version list for `id` (`/api/v1/extensions/{id}` -> `versions`).
async fn fetch_versions(
    client: &reqwest::Client,
    store_url: &str,
    id: &str,
) -> Result<Vec<String>, String> {
    let url = format!("{}/api/v1/extensions/{}", store_url, urlencoding::encode(id));
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("store request failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("store returned error: {e}"))?;
    let body: Value = resp.json().await.map_err(|e| format!("bad store body: {e}"))?;
    let versions = body["versions"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    Ok(versions)
}
```

In `greentic-designer/src/ui/routes/store.rs`, add near the existing
`store_reconcile` re-export:

```rust
mod store_update;
pub use store_update::{check_installed_updates, installed_triples};
```

In `greentic-designer/src/ui/routes/router_build.rs`, add inside `platform_routes()`
after the `/api/store/installed` line:

```rust
        .route("/api/store/installed/updates", get(store::check_installed_updates))
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p greentic-designer --test store_update`
Expected: PASS (1 test).

- [ ] **Step 5: Commit**

```bash
git add greentic-designer/
git commit -m "feat(designer): add installed-updates check endpoint"
```

---

### Task 7: Designer backend — `POST /api/store/extensions/{id}/upgrade`

Upgrade a `design`/`mcp` extension with a runtime load-probe and auto-rollback;
other kinds return a status-only response.

**Files:**
- Modify: `greentic-designer/src/ui/routes/store_update.rs`
- Modify: `greentic-designer/src/ui/routes/router_build.rs` (register route)
- Test: add to `greentic-designer/tests/store_update.rs`

**Interfaces:**
- Consumes: `installed_triples`, `fetch_versions`, `resolve`, `UpdateStatus`, `AppState.extension_runtime` (`greentic_ext_runtime::{ExtensionRuntime, ExtensionId}`), `AppState.store_url`, existing `store::install_artifact_bytes`, `greentic_extension_sdk_registry::storage::Storage`, `ExtensionState`.
- Produces: `pub async fn upgrade_extension(State<Arc<AppState>>, Path<String>) -> impl IntoResponse`.

- [ ] **Step 1: Write the failing test**

Add to `greentic-designer/tests/store_update.rs`:

```rust
#[test]
fn kind_for_dir_maps_design_and_mcp() {
    use greentic_designer::ui::routes::store::kind_for_dir;
    use greentic_extension_sdk_contract::ExtensionKind;
    assert_eq!(kind_for_dir("design"), Some(ExtensionKind::Design));
    assert_eq!(kind_for_dir("mcp"), Some(ExtensionKind::WasixMcpRouter));
    assert_eq!(kind_for_dir("bundle"), Some(ExtensionKind::Bundle));
    assert_eq!(kind_for_dir("nope"), None);
}

#[test]
fn is_hot_upgradeable_only_design_and_mcp() {
    use greentic_designer::ui::routes::store::is_hot_upgradeable;
    assert!(is_hot_upgradeable("design"));
    assert!(is_hot_upgradeable("mcp"));
    assert!(!is_hot_upgradeable("bundle"));
    assert!(!is_hot_upgradeable("provider"));
    assert!(!is_hot_upgradeable("deploy"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-designer --test store_update`
Expected: FAIL — `kind_for_dir` / `is_hot_upgradeable` not defined.

- [ ] **Step 3: Write minimal implementation**

Append to `greentic-designer/src/ui/routes/store_update.rs`:

```rust
use axum::extract::Path as AxumPath;
use greentic_extension_sdk_contract::ExtensionKind;
use greentic_extension_sdk_registry::storage::Storage;
use greentic_ext_runtime::ExtensionId;

/// Map an on-disk kind dir name to an `ExtensionKind`.
#[must_use]
pub fn kind_for_dir(kind_dir: &str) -> Option<ExtensionKind> {
    match kind_dir {
        "design" => Some(ExtensionKind::Design),
        "bundle" => Some(ExtensionKind::Bundle),
        "deploy" => Some(ExtensionKind::Deploy),
        "provider" => Some(ExtensionKind::Provider),
        "mcp" => Some(ExtensionKind::WasixMcpRouter),
        _ => None,
    }
}

/// Only `design` + `mcp` hot-reload via the runtime watcher in slice 1.
#[must_use]
pub fn is_hot_upgradeable(kind_dir: &str) -> bool {
    matches!(kind_dir, "design" | "mcp")
}

/// POST /api/store/extensions/{id}/upgrade — upgrade to the highest permitted
/// version. Hot-swap for design/mcp with load-probe + auto-rollback; other kinds
/// return 409 with guidance.
pub async fn upgrade_extension(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
) -> impl IntoResponse {
    let Some(home) = dirs::home_dir() else {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":"home_dir_unavailable"})))
            .into_response();
    };

    // Locate the installed extension + its kind dir.
    let installed = installed_triples(&home);
    let Some((kind_dir, _, current)) = installed.iter().find(|(_, eid, _)| eid == &id).cloned()
    else {
        return (StatusCode::NOT_FOUND, Json(json!({"error":"not_installed","id":id}))).into_response();
    };

    if !is_hot_upgradeable(&kind_dir) {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "error": "not_hot_upgradeable",
                "kind": kind_dir,
                "hint": "this extension kind updates on next pack rebuild / deploy / restart",
            })),
        )
            .into_response();
    }
    let Some(kind) = kind_for_dir(&kind_dir) else {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":"unknown_kind"}))).into_response();
    };

    // Resolve target via the same constraint the check endpoint uses.
    let ext_state = ExtensionState::load(&home).unwrap_or_default();
    let constraint = ext_state.constraint_for(&id).to_string();
    let client = reqwest::Client::new();
    let versions = match fetch_versions(&client, &state.store_url, &id).await {
        Ok(v) => v,
        Err(reason) => return (StatusCode::BAD_GATEWAY, Json(json!({"error":reason}))).into_response(),
    };
    let target = match resolve(&current, &versions, &constraint) {
        UpdateStatus::UpdateAvailable { target, .. } => target,
        other => {
            return (StatusCode::OK, Json(json!({"upgraded": false, "status": other}))).into_response();
        }
    };

    // Download + install the target version (its own versioned dir).
    let url = format!(
        "{}/api/v1/extensions/{}/{}/artifact",
        state.store_url,
        urlencoding::encode(&id),
        urlencoding::encode(&target)
    );
    let bytes = match client.get(&url).send().await.and_then(|r| r.error_for_status()) {
        Ok(r) => match r.bytes().await {
            Ok(b) => b,
            Err(e) => return (StatusCode::BAD_GATEWAY, Json(json!({"error":format!("read body failed: {e}")}))).into_response(),
        },
        Err(e) => return (StatusCode::BAD_GATEWAY, Json(json!({"error":format!("download failed: {e}")}))).into_response(),
    };
    let (home2, id2, target2) = (home.clone(), id.clone(), target.clone());
    let install_res = tokio::task::spawn_blocking(move || {
        super::install_artifact_bytes(&bytes, &home2, &id2, &target2).map(|_| ())
    })
    .await;
    match install_res {
        Err(join_err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("install task failed: {join_err}") })),
            )
                .into_response();
        }
        Ok(Err(install_err)) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("install failed: {install_err:?}") })),
            )
                .into_response();
        }
        Ok(Ok(())) => {}
    }

    // Probe: poll the runtime's loaded version for up to ~10s. The watcher loads
    // the new dir; if it fails to compile/parse the runtime keeps the old one.
    let ext_id = ExtensionId(id.clone());
    let mut loaded_target = false;
    for _ in 0..40 {
        if let Some(ext) = state.extension_runtime.loaded().get(&ext_id)
            && ext.describe.metadata.version == target
        {
            loaded_target = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }

    let storage = Storage::new(&home);
    if loaded_target {
        // Success: drop the old version dir so one version per id remains.
        let _ = storage.remove_extension(kind, &id, &current);
        let id3 = id.clone();
        let _ = ExtensionState::update(&home, |s| {
            if let Some(p) = s.default.policies.get_mut(&id3) {
                p.last_failed = None;
            }
        });
        (StatusCode::OK, Json(json!({"upgraded": true, "from": current, "to": target}))).into_response()
    } else {
        // Failure: the new version did not become live. Remove the broken dir;
        // the runtime never left the old version (auto-rollback).
        let _ = storage.remove_extension(kind, &id, &target);
        let reason = format!("version {target} failed to load within timeout");
        let (id3, target3, reason3) = (id.clone(), target.clone(), reason.clone());
        let _ = ExtensionState::update(&home, |s| s.record_failed(&id3, &target3, &reason3));
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"upgraded": false, "error": reason}))).into_response()
    }
}
```

Register in `router_build.rs` after the install route:

```rust
        .route(
            "/api/store/extensions/{id}/upgrade",
            post(store::upgrade_extension),
        )
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p greentic-designer --test store_update`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add greentic-designer/
git commit -m "feat(designer): add extension upgrade endpoint with load-probe + rollback"
```

---

### Task 8: Designer frontend — badge + Upgrade button

Surface "update available" in `InstalledCard` and wire an upgrade mutation.

**Files:**
- Modify: `greentic-designer/web/src/api/types.ts` (add `InstalledUpdate` type)
- Modify: `greentic-designer/web/src/api/hooks/useExtensions.ts` (add hooks)
- Modify: `greentic-designer/web/src/features/extensions/ExtensionCard.tsx` (badge + button)
- Test: `greentic-designer/web/src/features/extensions/ExtensionUpdates.test.tsx` (create)

**Interfaces:**
- Consumes: `fetchJSON`, `useQuery`, `useMutation`, `installedExtensionsKey`.
- Produces: `useExtensionUpdates()`, `useUpgradeExtension()`, `InstalledUpdate` type.

- [ ] **Step 1: Write the failing test**

Create `greentic-designer/web/src/features/extensions/ExtensionUpdates.test.tsx`:

```tsx
import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactNode } from 'react';
import { UpdateBadge } from './ExtensionCard';

function withQueryClient(node: ReactNode) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={qc}>{node}</QueryClientProvider>);
}

afterEach(() => vi.restoreAllMocks());

describe('UpdateBadge', () => {
  it('renders the target version when an update is available', () => {
    withQueryClient(
      <UpdateBadge update={{ id: 'greentic.foo', current: '1.0.0', kind: 'design', status: 'update_available', target: '1.1.0' }} />,
    );
    expect(screen.getByText(/1\.1\.0/)).toBeTruthy();
  });

  it('renders nothing when up to date', () => {
    const { container } = withQueryClient(
      <UpdateBadge update={{ id: 'greentic.foo', current: '1.0.0', kind: 'design', status: 'up_to_date' }} />,
    );
    expect(container.textContent).toBe('');
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd greentic-designer/web && npx vitest run src/features/extensions/ExtensionUpdates.test.tsx`
Expected: FAIL — `UpdateBadge` is not exported.

- [ ] **Step 3: Write minimal implementation**

Add to `greentic-designer/web/src/api/types.ts`:

```typescript
export interface InstalledUpdate {
  id: string;
  current: string;
  kind: string;
  /** Snake-case tag from the backend `UpdateStatus`. */
  status: 'up_to_date' | 'update_available' | 'pinned' | 'out_of_range' | 'unknown';
  target?: string;
  latest?: string;
  constraint?: string;
  reason?: string;
}
```

Add to `greentic-designer/web/src/api/hooks/useExtensions.ts`:

```typescript
import type { InstalledUpdate } from '@/api/types';

export const installedUpdatesKey = ['extensions', 'installed', 'updates'] as const;

export function useExtensionUpdates() {
  return useQuery({
    queryKey: installedUpdatesKey,
    queryFn: ({ signal }) =>
      fetchJSON<InstalledUpdate[]>('/store/installed/updates', { signal }),
    // Update checks hit the network store; refresh sparingly.
    staleTime: 60_000,
  });
}

export function useUpgradeExtension() {
  const qc = useQueryClient();
  return useMutation<{ upgraded: boolean; to?: string }, Error, { id: string }>({
    mutationFn: ({ id }) =>
      fetchJSON<{ upgraded: boolean; to?: string }>(
        `/store/extensions/${encodeURIComponent(id)}/upgrade`,
        { method: 'POST' },
      ),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: installedExtensionsKey });
      void qc.invalidateQueries({ queryKey: installedUpdatesKey });
    },
  });
}
```

In `greentic-designer/web/src/features/extensions/ExtensionCard.tsx`, export a
badge component (place near the other inline-SVG usage), using an inline SVG
up-arrow line-icon (no emoji):

```tsx
import type { InstalledUpdate } from '@/api/types';

export function UpdateBadge({ update }: { update?: InstalledUpdate }) {
  if (!update || update.status !== 'update_available') return null;
  return (
    <span
      title={`Update available: ${update.target}`}
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        gap: 4,
        padding: '2px 7px',
        borderRadius: 6,
        border: '1px solid var(--green-500)',
        color: 'var(--green-700)',
        fontSize: 11,
        fontWeight: 500,
      }}
    >
      <svg viewBox="0 0 12 12" width="11" height="11" aria-hidden="true">
        <path d="M6 9.5V2.5M6 2.5L3 5.5M6 2.5L9 5.5" stroke="var(--green-600)" strokeWidth="1.5" fill="none" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
      {`Update → v${update.target}`}
    </span>
  );
}
```

Wire it into `InstalledCard`: accept two new optional props
`update?: InstalledUpdate` and `onUpgrade?: () => void`, render `<UpdateBadge update={update} />` next to the version line (`ext-card-author`), and add an
"Upgrade" button in the `ext-card-meta` row, shown only when
`update?.status === 'update_available'` AND the kind is hot-upgradeable
(`ext.kind === 'DesignExtension'` or the mcp kind), calling `onUpgrade`. In the
parent list component, read `useExtensionUpdates()`, map updates by `id`, pass the
matching `update` to each `InstalledCard`, and pass `onUpgrade` wired to
`useUpgradeExtension().mutate({ id: ext.name })`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cd greentic-designer/web && npx vitest run src/features/extensions/ExtensionUpdates.test.tsx`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add greentic-designer/web/
git commit -m "feat(designer-ui): show update-available badge + upgrade button"
```

---

## Final verification

- [ ] In `greentic-designer-sdk`: `bash ci/local_check.sh` (fmt + clippy -D warnings + tests).
- [ ] In `greentic-designer`: `bash ci/local_check.sh` and `cd web && npm run test`.
- [ ] Manual smoke (optional): `gtdx outdated` against a dev store, then
  `gtdx update <id>`; confirm the new version dir replaces the old and
  `gtdx list --status` reflects it.
- [ ] Open separate PRs to `research` in each repo (`greentic-designer-sdk` first,
  since `greentic-designer` depends on the published SDK crates — coordinate the
  version bump / release-train if the new SDK symbols must be published before the
  designer PR can build against them).

## Cross-repo dependency note

Tasks 1–5 land in `greentic-designer-sdk`; Tasks 6–8 in `greentic-designer`. The
designer consumes the SDK crates by version. If the designer build resolves SDK
crates from a registry (not a path), the new symbols (`update::resolve`,
`check_installed_updates`, `ExtensionPolicy`, etc.) must be published first, or the
designer `Cargo.toml` temporarily pointed at the local path. Confirm which before
starting Task 6.
