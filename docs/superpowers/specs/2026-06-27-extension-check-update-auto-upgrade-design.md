# Extension Check-Update & Policy-Driven Auto-Upgrade — Design

- **Date:** 2026-06-27
- **Status:** Approved (slice 1 scope), pending implementation plan
- **Owner SDK repo:** `greentic-designer-sdk` (shared core lives here)
- **Surfaces touched:** `greentic-designer` (UI + backend), `greentic-extension-sdk-cli` (`gtdx`)
- **Spans:** `greentic-designer-sdk`, `greentic-designer`, `greentic-store-server` (read-only, no change needed)

## 1. Problem

Extensions across all families (`design`, `mcp`, `bundle`, `provider`, `deploy`) are
distributed as `.gtxpack` artifacts via the Greentic Store and installed to disk as
versioned directories. Today there is **no version-check** (nothing compares an
installed version against what the store offers) and **no upgrade path** beyond a
fresh manual `gtdx install`. Users cannot discover that an installed extension is
out of date, and there is no controlled upgrade mechanism.

This design adds **check-update** (detect "update available") and a **manual,
atomic, rollback-safe upgrade** for SDK-managed extensions, plus the schema
foundation for later **policy-driven auto-upgrade** (admin-controlled).

## 2. Goals (slice 1 = Fase 0+1)

- Detect update status per installed extension against the store, honoring a
  per-extension semver constraint (Cargo-like: `^2.0`, `~2.1`, `=2.0.1`, `*`).
- Surface "update available" in the Designer UI (badge) and CLI (`gtdx outdated`)
  for **all families**.
- Provide a **manual** upgrade action (Designer button, `gtdx update`) that is
  **atomic with auto-rollback** to the previous version on any failure. Hot-swap
  upgrade action is enabled for `design` + `mcp`; other families are status-only.
- Land the policy schema (constraint + update mode) so later phases need no
  re-migration.

## 3. Non-goals (deferred)

- Background policy-driven auto-upgrade (Fase 3).
- Admin-controlled tenant policy distribution — populating the reserved `tenants`
  map via `greentic-designer-admin` (Fase 4).
- Hot-swap upgrade for `bundle` / `provider` / `deploy` (status-only in slice 1).
- Garbage collection of old on-disk versions.
- Full constraint/mode editor UI (slice 1 displays constraint read-only).

## 4. Requirements (decisions captured during brainstorming)

| Decision | Choice |
|---|---|
| Scope | All extension families |
| Automation model | Policy-driven, admin-controlled (mirrors `store_reconcile.rs`) |
| Version policy | Per-extension semver constraint string (Cargo-like) |
| Failure handling | Atomic upgrade + auto-rollback to previous version |
| Build strategy | Extend the shared SDK (`greentic-designer-sdk`) |
| First slice | Core + check/notify + manual upgrade |
| Per-family action scope (slice 1) | Status for all; hot-upgrade action for `design` + `mcp` only |

## 5. Existing building blocks (reuse-first)

The install/distribution layer already exists in `greentic-designer-sdk` and is
shared by all families **including the runner** (the crate name says "designer"
but it is the shared extension SDK):

| Primitive | Location |
|---|---|
| Shared installer (fetch → verify integrity → verify signature → stage → commit, **already atomic**) | `greentic-extension-sdk-registry/src/lifecycle.rs` (`Installer::install`, ~51-157) |
| Versioned on-disk layout `~/.greentic/extensions/<kind>/<name>-<version>/` (versions coexist → rollback by keeping old dir) | `greentic-extension-sdk-registry/src/storage.rs` |
| `list_versions(name) -> Vec<String>` (all versions, for constraint resolution) | `greentic-extension-sdk-registry/src/registry.rs:28` |
| Store endpoint returning all versions + `artifact_sha256` + `yanked` | `greentic-store-server` `GET /api/v1/extensions/{name}` |
| Hot-reload watcher (designer + runner design-ext) | `greentic-designer-extensions/crates/greentic-ext-runtime` |
| Per-entry state file (`enabled: bool` only today; `tenants` map reserved/empty) | `greentic-extension-sdk-state/src/state.rs` (`extensions-state.json`) |

What is **missing** and built here: (1) version-resolver, (2) policy schema,
(3) upgrade orchestrator (load-probe + rollback), (4) surfaces (UI badge / CLI /
backend route).

## 6. Architecture

### 6.1 Policy schema (`greentic-extension-sdk-state`)

Add a new per-extension policy map keyed by **`id`** (version-independent), kept
separate from the existing `enabled` map (which is keyed by `id@version`).
Additive and backward-compatible; bump schema `"1.0"` → `"1.1"` while still
reading `"1.0"` files (missing `policies` → defaults).

```rust
/// Keyed by extension id (NOT id@version).
pub struct ExtensionPolicy {
    /// Cargo-like semver requirement: "^2.0", "~2.1", "=2.0.1", "*". None => "*".
    pub constraint: Option<String>,
    /// Manual today; `Auto` is honored starting Fase 3.
    pub mode: UpdateMode,
    /// Set after a failed auto-upgrade to suppress auto-retry of a broken
    /// version (manual retry still allowed). Light alternative to quarantine.
    pub last_failed: Option<FailedUpgrade>,
}

pub enum UpdateMode { Manual, Auto }

pub struct FailedUpgrade { pub version: String, pub reason: String }
```

`mode: Auto` ships in the schema now but is inert until Fase 3 — no re-migration
later.

### 6.2 VersionResolver (new module `update.rs` in `greentic-extension-sdk-registry`)

A **pure function** over inputs (no I/O), so it is fully unit-testable. A thin
async wrapper performs the `list_versions` calls and feeds the resolver.

```
resolve(installed_version, available_versions, constraint) -> UpdateStatus
```

```rust
pub enum UpdateStatus {
    UpToDate    { current: Version },
    UpdateAvailable { current: Version, target: Version, is_major_jump: bool },
    Pinned      { current: Version },                       // exact-match constraint
    OutOfRange  { current: Version, latest: Version, constraint: String },
    Unknown     { reason: String },                         // registry/network error
}
```

Rules:
- Parse available versions with the `semver` crate (already a dependency).
- Drop yanked versions; drop prereleases unless the constraint explicitly opts in.
- Apply `VersionReq` from `constraint` (default `*`); pick the highest match as
  `target`.
- A registry/network failure yields `Unknown { reason }` — **never** a false
  `UpToDate`.

### 6.3 Upgrade executor (extend `lifecycle.rs`)

```
upgrade(id, kind, target_version):
  1. resolve target artifact (reuse Installer::install fetch + verify + stage)
  2. install to NEW versioned dir <name>-<target>/   (old dir untouched)
  3. load-probe(<target>): instantiate the new component WITHOUT making it active
        fail  -> delete staged dir; old stays active; return Err   (rollback)
        ok    -> continue
  4. commit/swap: flip active in extensions-state.json
        (enabled[id@target]=true, enabled[id@current]=false)
        watcher reloads
  5. post-commit health check:
        unhealthy -> flip back to old version (still on disk)
                     + set policy.last_failed{version,reason}
                     (manual retry allowed; auto-retry suppressed)
```

The previous version is never deleted during upgrade, so rollback is always a
state flip back to an on-disk directory.

### 6.4 Surfaces (Fase 1)

**Designer backend** (`greentic-designer/src/ui/routes/store.rs`):
- Extend `list_installed()` to compute and return `updateStatus` + `target` per
  installed extension (calls the resolver).
- Add `POST /api/store/extensions/{id}/upgrade` → upgrade executor (gated to
  `design`/`mcp`; returns a "status-only" response for other kinds).

**Designer frontend** (`web/src/features/extensions/ExtensionCard.tsx`,
`InstalledCard`):
- "Update available → vX.Y" badge using an **inline SVG line-icon** (no emoji).
- "Upgrade" button for `design`/`mcp`; for other families show status + a hint on
  how the update is applied (re-pack / next deploy / restart).
- Constraint is displayed read-only in slice 1.

**CLI** (`greentic-extension-sdk-cli` / `gtdx`):
- `gtdx outdated` — table: id, kind, current, target, status.
- `gtdx update <id>` / `gtdx update --all` — manual upgrade with rollback.

### 6.5 Per-family reload behavior

| Kind | Reload | Slice-1 action |
|---|---|---|
| `design` | hot-reload via watcher | hot-upgrade ✅ |
| `mcp` | same runtime | hot-upgrade ✅ |
| `bundle` | embedded in `.gtpack` | status-only; upgrade ≈ re-pack |
| `provider` | post-install hook | status-only; may need restart |
| `deploy` | consumed by greentic-deployer | status-only; applies at next deploy |

## 7. Data flow

**Check (read):**
```
Designer "Installed" tab / `gtdx outdated`
  -> Storage.scan() -> [(id, kind, current_version)]
  -> registry.list_versions(id)      (store: GET /api/v1/extensions/{id})
  -> VersionResolver(current, versions, policy.constraint)
  -> UpdateStatus per ext            -> UI badge / CLI table
```

**Manual upgrade (write, design/mcp):** see §6.3.

## 8. Error handling

- No silent failure (project standard): check-time registry/network errors →
  explicit `Unknown` status, never a false `UpToDate`.
- Signature/integrity failure → abort upgrade, explicit error, old version intact.
- Probe/health failure → auto-rollback + `last_failed` marker.
- `thiserror`/`anyhow`; English logs/traces; no `unwrap()` / `panic!()` on
  production paths.

## 9. Testing

- `VersionResolver`: unit-test matrix (version lists × constraints → expected
  status), including yanked, prerelease, out-of-range, and unknown.
- Upgrade executor: integration test with a mock registry — **must** cover
  probe-fail → rollback and commit-then-unhealthy → flip-back.
- State schema: serde round-trip + backward-compat read of `"1.0"`.
- Designer route test (`list_installed` status field + upgrade endpoint).
- CLI test (`outdated` / `update`).
- Non-`design`/`mcp` family: assert upgrade action is gated to status-only.

## 10. Open follow-ups (later phases)

- Fase 3: background reconciler honoring `constraint` + `mode: Auto` (extend the
  `store_reconcile.rs` pattern); honor `last_failed` to avoid retry loops.
- Fase 4: admin-controlled policy distribution into the reserved `tenants` map via
  `greentic-designer-admin`.
- GC of old on-disk versions (keep N).
- Full constraint/mode editor UI.
