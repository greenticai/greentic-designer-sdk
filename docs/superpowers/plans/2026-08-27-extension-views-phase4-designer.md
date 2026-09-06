# Extension Views — Phase 4 (Designer, production) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Designer honour the placement and visibility a tenant admin configured, instead of showing every view at its author's default.

**Architecture:** The Designer reads resolved view configuration from the Admin console through the client layer it already has, caches it per tenant, and serves the author's defaults when Admin is unreachable. Nothing about the render path changes.

**Spec:** `docs/superpowers/specs/2026-08-26-extension-views-phase3-admin-design.md` — Admin is the single source of truth for both surfaces, and its resolution cascade is defined there.

## What already exists, and why this phase is small

The Designer renders views today (`src/ui/routes/extension_views.rs`): an asset route, `GET /api/views`, a sidebar group, and `ExtensionViewHost` with the bridge. What it does not do is ask anyone where a view should go — it reports `placement.slot` and `order` straight from `describe.json`.

**The cross-service coupling this needs is not new.** The parent spec called it "the first runtime coupling in that direction" and that was wrong. `state.admin_endpoint` already exists, `src/admin/` holds roughly thirty client modules, and `src/admin/chronicle.rs` is the exact shape this phase needs: a per-slug `DashMap` TTL cache that **serves a stale snapshot on a transient failure**, with `chronicle_tests.rs` showing the wiremock idiom including `a_500_with_a_cached_snapshot_serves_stale`.

So this is not new machinery. It is one more client module beside twenty-nine siblings, and the plan below is mostly "follow `chronicle.rs`".

## Global Constraints

- Repo `greentic-designer`, integration branch `develop`.
- **Copy `src/admin/chronicle.rs`'s shape.** Per-tenant cache, stale-on-failure, the same error type, the same test idiom. Do not invent a second caching or fallback strategy — the value here is that an operator debugging a stale view behaves the same as one debugging a stale chronicle index.
- A `husky` pre-commit hook runs `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets --all-features -- -D warnings`. Let it pass; do not bypass it.
- No `unwrap()` / `panic!()` in library code. Tests may.
- Conventional commits. Never run `git stash` — this machine shares stash stacks across sessions.

---

### Task 1: The Admin client module

**Files:**
- Create: `src/admin/views.rs`, `src/admin/views_tests.rs`
- Modify: `src/admin/mod.rs`

**Interfaces:**
- Consumes: the existing `AdminClient`, `AdminClientError`, and whatever `chronicle.rs` uses for its cache.
- Produces: `AdminClient::designer_views(tenant_slug) -> Result<Option<ViewConfigSnapshot>, AdminClientError>`.

Admin serves the resolved list for the calling user, tenant and team. The Designer passes its own tenant context and receives views already filtered — it does not re-run the cascade, and it must not try to.

- [ ] **Step 1: Read the model first**

Read `src/admin/chronicle.rs` and `src/admin/chronicle_tests.rs` end to end before writing anything. This task is a transposition of them, and the closer it stays the better.

- [ ] **Step 2: Write the failing tests**

Mirror `chronicle_tests.rs`, including its wiremock setup. Four behaviours:

```rust
#[tokio::test]
async fn a_snapshot_is_fetched_and_cached_per_tenant() { … }

#[tokio::test]
async fn different_slugs_are_cached_independently() { … }

/// The property this whole phase turns on. A Designer that cannot reach
/// Admin must keep showing views, not lose them — a degraded console that
/// silently drops half its navigation is worse than a stale one.
#[tokio::test]
async fn a_500_with_a_cached_snapshot_serves_stale() { … }

/// And with no cache at all, the caller gets `None` rather than an error,
/// so the route can fall back to the author's defaults.
#[tokio::test]
async fn a_500_with_no_cached_snapshot_yields_none_not_an_error() { … }
```

- [ ] **Step 3: Run them and watch them fail**

Run: `cargo test --lib admin::views`
Expected: FAIL — the module does not exist.

- [ ] **Step 4: Implement**

Follow `chronicle.rs` line for line where you can. Where you deviate, say why in a comment.

- [ ] **Step 5: Run the tests, then commit**

```bash
git commit -m "feat(admin): client for tenant-configured view placement"
```

---

### Task 2: Honour configuration in `GET /api/views`

**Files:**
- Modify: `src/ui/routes/extension_views.rs`, `src/ui/routes/extension_views/tests.rs`

**Interfaces:**
- Consumes: `AdminClient::designer_views` from Task 1.

- [ ] **Step 1: Write the failing tests**

```rust
/// Configuration wins over the author's suggestion.
#[test]
fn a_configured_placement_overrides_the_describe_default() { … }

/// A view the tenant disabled does not appear at all.
#[test]
fn a_view_disabled_for_this_tenant_is_omitted() { … }

/// The fallback that keeps a degraded Designer usable.
#[test]
fn with_no_admin_configuration_the_authors_defaults_are_served() { … }

/// Ordering stays total and stable even when configuration supplies the
/// numbers — two extensions on the same `order` must not swap between
/// renders.
#[test]
fn ordering_is_stable_when_configuration_supplies_the_order() { … }
```

- [ ] **Step 2: Implement**

Fetch the snapshot, overlay it on the describe-derived list, drop what is disabled, and keep the existing `(order, ext_id, view_id)` sort. When the snapshot is `None` — Admin unconfigured, unreachable, or with nothing to say — serve exactly what is served today.

That last case is not an edge case. A Designer running without an Admin endpoint at all is a supported configuration, and it must keep working.

- [ ] **Step 3: Run the tests, then commit**

---

### Task 3: Honour the slot in the sidebar

**Files:**
- Modify: `web/src/components/layout/Sidebar.tsx`, `web/src/api/hooks/useExtensionViews.ts`

Today every view lands in one "Extension views" group regardless of `slot` and `path`. Now that configuration decides those, place them.

- [ ] **Step 1: Place by slot and path**

`designer.sidebar` with an empty `path` goes to the top level; a `path` names the group to place under. An unresolvable slot or path falls back to the "Extensions" group **and surfaces a diagnostic** — never dropped silently. That fallback is a promise made to extension authors in the contract, not a convenience.

- [ ] **Step 2: Test both directions**

A resolvable path lands where configured; an unresolvable one lands in the fallback group and reports why.

- [ ] **Step 3: Commit**

---

### Task 4: Raise the compat floor

**Files:**
- Modify: `crates/greentic-extension-sdk-contract/src/compat.rs` **in the SDK repo**, not this one

This is the last piece of a debt taken deliberately in Phase 1, and it is easy to forget because it lives in a different repository.

`MIN_DESIGNER_VERSION` still says `1.2.0`. A view-bearing extension therefore claims it loads on any designer speaking v2, when in fact `Contributions` is `deny_unknown_fields` and such a describe fails to parse on every designer released before view support. The constant's doc comment records the exception and says to raise it when host support ships.

- [ ] **Step 1: Determine the version that actually ships this**

The Designer release carrying Phases 2 and 4. Do not guess it — find the release that includes them.

- [ ] **Step 2: Raise the constant and remove the exception note**

Replace the "Exception" paragraph with the real floor. The exception existed only because the floor was unknowable; once it is known, leaving the note is worse than deleting it.

- [ ] **Step 3: Update `docs/authoring-views.md` and the README**

Both currently tell authors the floor is not what `min_designer_version` says. That warning must go when it stops being true, or it trains readers to ignore warnings.

- [ ] **Step 4: Ship it as a release**

Follow `docs/releasing.md`: check nobody else is mid-release, land on `research` first, bump the four things, update the README floor, run the gate, and sync the README back after the tag.

---

### Task 5: The gate and the PR

- [ ] **Step 1: Run the full check in the foreground and paste real output**
- [ ] **Step 2: Verify against a running Designer**

Not a build. An extension with a view installed, an Admin endpoint configured, a placement changed in Admin, and the Designer's sidebar reflecting it. Then kill the Admin endpoint and confirm the views are still there, at their author defaults.

That second half is the one worth doing carefully. It is the behaviour the design promises and the one nobody tests.

- [ ] **Step 3: PR against `develop`**

## What this plan deliberately leaves out

- **Wiring `invokeTool` to the real runtime.** The Designer has an extension runtime, so unlike Admin it *could* execute contributed tools. That is a real feature with its own permission questions, and folding it into a placement change would hide it.
- **A second caching strategy.** If `chronicle.rs`'s TTL turns out wrong for this data, change it there too or not at all.
