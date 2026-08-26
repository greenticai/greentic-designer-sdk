# Extension Views — Phase 3b (Admin backend) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the Admin console everything a browser needs to render an extension-contributed view for the right tenant and team — assets, resolution, a bridge, and an audit trail — with none of it hand-mirrored into the frontend.

**Architecture:** Assets are materialised into Postgres during catalog sync and served from a route that carries its own CSP. Placement and visibility live in two cascading tables that mirror the extension-enablement cascade already in the repo. The bridge re-enters the existing guard chain rather than re-checking permissions, so `classify_admin_route` stays the single source of truth.

**Tech Stack:** Rust, Axum, `sqlx` (Postgres), `tower`, `zip`, `reqwest`.

**Spec:** `docs/superpowers/specs/2026-08-26-extension-views-phase3-admin-design.md` — read it whole. It records *why* for every decision below, and the reasons are load-bearing.

## Global Constraints

- Repo: `greentic-designer-admin`. Integration branch is `develop`. Postgres via `sqlx::PgPool` (`src/state.rs`); migrations in `migrations_pg/`, applied by `run_pg_migrations` (`src/db.rs`) at startup and by the `migrate` subcommand.
- Routes live in `src/routes/admin/`, each module exposing `pub fn router() -> Router<AppState>`, merged in `src/routes/admin/mod.rs`.
- **`classify_admin_route` (`src/auth/authz/mod.rs`) is a hand-written path matcher.** A new route with no class is rejected. Every route added here needs a deliberate class, and the spec says which.
- Non-test source files stay under 500 lines. `src/routes/admin/mod.rs` is already large; put new work in new modules.
- No `unwrap()` / `panic!()` in library code. Tests may.
- Conventional commits. Never run `git stash` — this machine shares stash stacks across sessions.
- A demo slice already exists on branch `BimaPangestu28/extension-view-render`: a migration, a seed script, an asset route, `/api/admin/views`, and an `ExtensionViewHost`. **It is a demo, not this.** Read it for what it proves — the render path works and the headers are right — then replace its seeded shortcut with the real sync. Its migration is a reasonable starting shape.

## Prerequisite

Phase 3a adds `GET /api/v1/extensions/{name}/{version}/view-assets` to the store server, returning an archive of `assets/views/**` plus a `greentic.view-assets/v1` manifest (`path`, `sha256`, `size`, `content_type`). Task 2 consumes exactly that shape. If it has not landed, Tasks 1, 3, 4 and 5 do not depend on it — build those first and integrate.

---

### Task 1: The four tables

**Files:**
- Create: `migrations_pg/<timestamp>_extension_views.sql`
- Test: `tests/` — a migration smoke test if the repo has one; otherwise assert the schema from a repo test that already opens a pool

**Interfaces:**
- Produces: `extension_view_assets`, `extension_views`, `tenant_view_placements`, `team_view_overrides`, `extension_bridge_audit`.

The spec's "Data model" section gives every column. Two things it explains that the DDL cannot:

`extension_view_assets` carries both `bytes BYTEA NULL` and `s3_key TEXT NULL` because it mirrors `uploaded_assets` (`src/repo/uploads.rs`, served by `src/routes/assets.rs`). Only `bytes` is used at first; `s3_key` exists from day one so moving to object storage later is a data migration, not a schema one.

`tenant_view_placements.locked` is writable by platform admins only. It freezes *placement*, not *visibility* — a tenant admin may still disable a locked view. Put that in a column comment; it is the kind of rule that gets inverted by a well-meaning later change.

- [ ] **Step 1: Write the migration**

Follow the naming and style of the newest file in `migrations_pg/`. Copy the column set from the spec verbatim. Add a comment block at the top explaining that assets are materialised at sync — the demo branch's migration has one worth reusing, minus its "DEMO SHORTCUT" note, which no longer applies.

- [ ] **Step 2: Apply and verify**

Run: `cargo run --bin greentic-admin -- migrate` against a scratch database.
Expected: applies cleanly. Then confirm every table and index exists.

- [ ] **Step 3: Commit**

```bash
git add migrations_pg/
git commit -m "feat(views): tables for view assets, placement, team overrides and bridge audit"
```

---

### Task 2: Sync-time materialisation

**Files:**
- Create: `src/repo/extension_views.rs` (or extend the demo branch's version)
- Modify: `src/routes/admin/extensions/sync.rs` (`run_sync_pass`, around line 294)
- Modify: `src/routes/admin/extensions/store_client.rs`
- Test: alongside each

**Interfaces:**
- Consumes: the store's `view-assets` endpoint; the existing sync pass.
- Produces: rows in `extension_view_assets` and `extension_views`.

- [ ] **Step 1: Write the failing tests**

Four behaviours, and the third is the one that matters most:

```rust
#[tokio::test]
async fn assets_materialise_for_an_extension_that_declares_views() { … }

/// Content-addressed: a re-sync of an unchanged version must not refetch.
#[tokio::test]
async fn a_resync_with_unchanged_assets_fetches_nothing() { … }

/// The failure that must not cascade. Sync processes many extensions; one
/// bad archive cannot be allowed to hold up the rest, and the catalog row
/// must still update.
#[tokio::test]
async fn a_failed_asset_fetch_does_not_fail_the_sync_pass() { … }

/// Admin verifies what it received. The store never opens `manifest.json` —
/// it only checks `manifestSha256` is 64 hex characters — so trusting its
/// numbers would be trusting something nobody checked.
#[tokio::test]
async fn a_sha256_mismatch_rejects_the_file() { … }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p greentic-admin extension_views`
Expected: FAIL — the functions do not exist.

- [ ] **Step 3: Implement**

In `store_client.rs`, a `fetch_view_assets(name, version)` returning the archive bytes. In `extension_views.rs`, a pure `extract_and_verify(archive) -> Result<Vec<ViewAsset>, _>` that reads the `view-manifest.json` entry, hashes each file, and rejects a mismatch — pure so it is testable without HTTP or a database.

In `run_sync_pass`, after the catalog row upsert, for each extension whose describe carries `views[]`: fetch, verify, upsert, and skip files whose sha256 is already present. Wrap the whole per-extension block so a failure records a diagnostic and continues.

- [ ] **Step 4: Retention**

At the end of a successful pass, delete `extension_view_assets` rows whose `version` is neither the catalog's current version nor the immediately-previous one.

This is one rule and not a reaper, because of a fact worth restating: **Admin does not track per-tenant extension versions.** `TenantExtensionRow` flattens the catalog row and the catalog holds one `version` per extension, so "keep what is entitled" collapses to "keep the current one". Keeping one previous version covers a page open while a sync runs.

- [ ] **Step 5: Run the tests, then commit**

```bash
git commit -m "feat(views): materialise view assets during catalog sync"
```

---

### Task 3: The asset route

**Files:**
- Create: `src/routes/admin/extension_view_assets.rs`
- Modify: `src/routes/admin/mod.rs`, `src/auth/authz/mod.rs`
- Test: alongside

**Interfaces:**
- Produces: `GET /api/admin/extensions/{ext_id}/views/{view_id}/*path`.

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn a_traversal_attempt_is_rejected_as_traversal_not_as_a_missing_file() { … }

/// The rule `/uploads` already paid for: a row written while S3 was on has
/// `bytes = NULL`. If S3 is later turned off, that row must answer 503 —
/// the bytes exist and we cannot reach them. A 404 reads to a tenant as
/// "your page is gone" when it is a config regression.
#[tokio::test]
async fn an_s3_row_with_s3_disabled_answers_503_not_404() { … }

#[tokio::test]
async fn the_response_carries_the_route_scoped_csp_and_no_x_frame_options() { … }
```

That last one matters more than it looks: `X-Frame-Options: DENY` would block the very iframe this feature exists for. `frame-ancestors 'self'` states the intent without the trap.

- [ ] **Step 2: Implement**

Normalise the path **before** the lookup. Serve `bytes` inline or 307 to a presigned S3 URL, **branching on the row's `s3_key` alone, never on current config** — `src/routes/assets.rs:366` carries the comment explaining why; read it.

Headers exactly as the spec lists them.

- [ ] **Step 3: Classify the route**

Add it to `classify_admin_route`. Authenticated operator, no tenant scoping: view assets are published extension code, already public in the store. The gate that matters is on the bridge, which carries data. Scoping the bytes would add a query per file for a page that fetches many, and protect nothing that is not already public. Put that reasoning in a comment beside the arm.

- [ ] **Step 4: Run the tests, then commit**

---

### Task 4: Resolution and `/api/admin/views`

**Files:**
- Create: `src/repo/view_resolution.rs`, `src/routes/admin/views.rs`
- Modify: `src/routes/admin/mod.rs`, `src/auth/authz/mod.rs`
- Test: alongside

**Interfaces:**
- Produces: `GET /api/admin/views`, `GET /api/views/slots`, and the `TenantScoped` config routes for placements and team overrides.

- [ ] **Step 1: Write the failing tests**

Table-driven over the four-step cascade from the spec. Cover team-beats-tenant, tenant-beats-default, `min_visibility` filtering, and — the one most likely to be got wrong — that `locked` freezes placement while leaving `enabled` writable by the tenant admin.

- [ ] **Step 2: Implement resolution**

Server-side, returning views already filtered for the caller. The frontend does no permission logic at all. That is deliberate: `web/src/lib/navItems.ts` carries a comment warning that its `tiers` and `capability` fields must be kept "in step" by hand with the Rust classifier — two copies of one rule held together by discipline. Views get one copy.

- [ ] **Step 3: The config routes**

`TenantScoped`, following the split `team_extensions.rs` already establishes: tenant-level extension routes are `PlatformOnly`, team-level are `TenantScoped`, so a tenant admin arranges their own house without touching platform entitlement. `locked` is writable only by a platform admin — enforce that in the handler, and test it.

- [ ] **Step 4: Slot catalogue**

`GET /api/views/slots` returns the slots this host publishes. It is the source of truth the SDK's lint snapshot is taken from.

- [ ] **Step 5: Run the tests, then commit**

---

### Task 5: The bridge and its audit

**Files:**
- Create: `src/routes/admin/view_bridge.rs`
- Modify: `src/routes/admin/mod.rs`, `src/auth/authz/mod.rs`
- Test: alongside

**Interfaces:**
- Produces: the bridge endpoints backing `callApi`, `fetch` and `invokeTool`.

- [ ] **Step 1: Write the failing tests**

```rust
/// THE test. A grant the extension declared, exercised by an operator who
/// does not have it, must still be refused. This is the case that matters
/// and the easiest to leave out.
#[tokio::test]
async fn a_call_inside_the_allowlist_but_outside_the_callers_rbac_is_refused() { … }

#[tokio::test]
async fn a_path_outside_the_declared_allowlist_is_refused() { … }

#[tokio::test]
async fn invoke_tool_returns_tool_unsupported_on_surface() { … }

#[tokio::test]
async fn a_fetch_to_a_loopback_or_metadata_address_is_refused() { … }

#[tokio::test]
async fn every_call_kind_writes_an_audit_row() { … }
```

- [ ] **Step 2: Implement `callApi` by re-entering the router**

Synthesise a `Request` and dispatch it through the same Admin router as a `tower::Service`, carrying the same `OperatorCtx`, so `auth_middleware` and `operator_authz_guard` run unchanged. The bridge adds exactly one filter *before* that: the path must match the extension's `permissions.ui.platformApi`.

Do not re-check permissions inside the handler. It would drift from `classify_admin_route` within weeks — the same way `navItems.ts` drifts — and the whole point of this design is that the bridge can only narrow, never widen.

The spec flags this as the least-travelled path in Axum. If synthesising a request proves genuinely fragile, stop and report rather than falling back to an explicit dispatch table: that fallback reintroduces the duplication this design exists to avoid, and it needs a deliberate decision, not a quiet slide.

- [ ] **Step 3: Implement `fetch` as a server-side proxy**

Against `permissions.ui.fetchHosts`, reusing the address validation `permissions.network` already applies: https only, loopback, link-local and cloud-metadata addresses rejected.

- [ ] **Step 4: `invokeTool` returns a typed refusal**

`E_TOOL_UNSUPPORTED_ON_SURFACE`. Admin has no executor for design-extension tools — it carries `greentic-mcp-exec` for the MCP-router world, a different WIT world. Faking a result would demo something that cannot exist. The spec's "The `invokeTool` gap" section records why this is a deferral rather than the end state; reference it in the code comment so the next reader finds the argument rather than re-deriving it.

- [ ] **Step 5: Audit every call**

One `extension_bridge_audit` row per call: extension, view, operator, tenant, call kind, method, target, outcome, duration. Separate from Admin's existing audit table, because read calls are high-volume and would bury what operators go there to find. Add the 90-day reaper.

- [ ] **Step 6: Run the tests, then commit**

---

### Task 6: The gate

- [ ] **Step 1: Run the repo's full check**

`ci/local_check.sh` drives `web/` npm checks and wants a live MinIO for DB integration tests. Run what applies, in the foreground, and paste real output. At minimum `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and the workspace test suite.

- [ ] **Step 2: Open the PR against `develop`**

Say what is real and what is not: this is the production sync path replacing the demo branch's seed, `invokeTool` is deliberately unsupported on this surface, and placement UI is Phase 3c.

## What this plan deliberately leaves out

- **Any frontend.** That is Phase 3c and can run in parallel — the `/api/admin/views` response shape is fixed by Task 4 and by the spec, which is what makes the parallelism safe.
- **An executor for design-extension tools.** Phase 5, with the reasoning recorded in the spec.
- **Per-view `ui` permissions.** Grants are extension-wide so a reviewer reads one block.
