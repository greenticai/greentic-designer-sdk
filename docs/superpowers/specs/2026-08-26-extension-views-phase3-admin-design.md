# Extension views in the Admin console (Phase 3)

Date: 2026-08-26
Status: design approved, implementation not started
Parent: `docs/superpowers/specs/2026-08-26-extension-views-design.md`

## Problem

Phase 1 shipped `contributions.views[]` in the SDK (v1.2.10): an author can
declare, scaffold, lint, validate and pack a UI page. Nothing renders one.

This phase makes the Admin console render them, with the targeting the feature
exists for: a view available to some tenants and not others, to some teams and
not others, placed where a tenant admin decides rather than where its author
guessed.

Admin goes before the Designer because this is where the advanced targeting
lives. The Designer (Phase 4) reuses the bridge and the host component
wholesale.

## Non-goals

- Rendering views in the Designer. That is Phase 4.
- Executing extension tools in Admin. See "The `invokeTool` gap" — deliberately
  deferred to a Phase 5, for reasons recorded there.
- A global Content-Security-Policy for Admin. CSP is set on the new asset route
  only; retrofitting it across an app that has never had one is its own project.
- Changing anything in the shipped v1.2.10 contract. Every decision below was
  chosen partly so the contract stays as published.

## Prerequisites, outside this phase's own work

Both are already-known blockers, not discoveries to be made during
implementation:

1. **`greentic-store-server` rejects view-bearing packs at publish.** It keeps
   its own copy of `describe-v2.json` in which `contributions` is
   `additionalProperties: false` without `views`. Until that copy gains `views`
   and `runtime.permissions.ui`, no view can reach the store at all. A fix is in
   flight separately.
2. **The store must expose view assets.** See "Store-server contract" below.
   That work is part of this phase.

## Decisions

| Question | Decision |
|---|---|
| Asset storage | The `/uploads` pattern: Postgres BLOB, S3 redirect when configured |
| Which versions kept | Catalog's current version, plus the immediately-previous one |
| Placement power | Tenant admin may move slots freely; platform admin may lock a placement |
| "Essential" views | No such concept — everything is hideable |
| Bridge `callApi` | Re-enters the existing guard chain; the extension's allowlist only narrows |
| Bridge `invokeTool` | Unsupported on this surface, as a typed error. Phase 5. |
| Audit | Own table, everything logged including outbound fetch, 90-day retention |
| Static asset gate | Authenticated operator; no tenant scoping on the bytes |
| Placement UI | A cross-extension Navigation tab, not a per-extension list |

## Data model

Four new tables, all in `migrations_pg/`.

### `extension_view_assets`

Materialised during catalog sync.

```
PRIMARY KEY (extension_id, version, path)
sha256, size_bytes, content_type
bytes    BYTEA NULL   -- the /uploads pattern: inline blob…
s3_key   TEXT  NULL   -- …or a pointer, when S3 was configured at write time
```

Retention is one rule, and it is simple only because of a fact worth stating:
**Admin does not track per-tenant extension versions.** `TenantExtensionRow`
flattens the catalog row, and the catalog holds exactly one `version` per
extension. No tenant can be pinned to an older version, so "keep what is
entitled" collapses to "keep the current version".

At the end of a successful sync pass, delete rows whose `version` is neither the
catalog's current version nor the immediately-previous one. Keeping one previous
version covers a page that was already open when the sync ran.

### `tenant_view_placements`

Written by tenant admins.

```
PRIMARY KEY (tenant_id, extension_id, view_id)
slot TEXT, path JSONB, sort_order INT, enabled BOOL
locked BOOL          -- platform admin only
config_json JSONB    -- passed to the view in the bridge `init`
```

### `team_view_overrides`

A narrow override, mirroring the existing `team_extensions` table.

```
PRIMARY KEY (team_id, extension_id, view_id)
enabled BOOL
```

### `extension_bridge_audit`

Separate from Admin's existing audit table on purpose: read calls through a
bridge are high-volume — one dashboard can make dozens per page open — and
mixing them in would bury the events operators currently go to the audit area to
find.

```
id, at, operator_id, tenant_id, extension_id, view_id
call_kind ('invoke_tool' | 'call_api' | 'fetch')
method, target       -- path for call_api, URL host+path for fetch
outcome, status_code, duration_ms
```

Retention 90 days, by a reaper. Sized from day one rather than added after the
table swells.

### Resolution

For "does operator O, in tenant T, team G, see view V?":

1. Is the extension entitled to T? (`team_extensions` → `tenant_extensions` →
   global default.) If not, stop.
2. Enabled? `team_view_overrides` → `tenant_view_placements.enabled` → the
   author's default.
3. Does O clear `min_visibility`? Host mapping is fixed, not author-chosen:
   `member` → any authenticated operator with reach to T; `tenant_admin` → tier
   `tenant` and above, which includes `partnership`; `platform_admin` → tier
   `platform`.
4. Placement = the `tenant_view_placements` row if present, else the
   `placement` from `describe.json`.

Steps 1 and 2 deliberately reuse the cascade the extension-enablement feature
already uses, so operators learn one rule rather than two.

`locked` freezes step 4, not step 2. A platform admin controls *where* a view
appears; a tenant admin may still switch it off. Locking visibility would force
something into a tenant's navigation, which is the opposite of the two-layer
model this feature is built on.

## Asset path

### Store-server contract

Packs are stored whole, as a single `.gtxpack` blob in S3 keyed
`{id}/{version}/{sha}.gtxpack`; nothing is unpacked at publish. Reading one
entry out of a stored pack is an established pattern —
`handlers/agentic_workers/pack_files.rs` does `blob.get(key)` →
`zip::ZipArchive` → `by_name(...)`. The repack step already preserves every
non-describe entry byte-for-byte, so `assets/` survives publish untouched today.

One endpoint, deliberately **bulk** rather than per-file:

```
GET /api/v1/extensions/{name}/{version}/view-assets
    → an archive of assets/views/** plus a JSON manifest
      (path, sha256, size, content_type)
```

Per-file would mean fetching the whole blob — capped at 100 MiB — once per CSS
file, and the store has no ETag or `Cache-Control` on any blob route to soften
that. One fetch per version is the right shape for sync-time materialisation,
which is the only consumer.

Public and anonymous, like the existing `artifact` route, and through the same
`gate::ensure_entitled` so paid listings stay gated.

### Materialisation, in `run_sync_pass`

For each extension whose describe carries `views[]`: fetch the archive, verify
each file's sha256, upsert into `extension_view_assets`, then run the retention
delete.

**Admin verifies, not the store.** The store never opens `manifest.json` — it
only checks that `manifestSha256` is 64 hex characters. So Admin hashes what it
received against the manifest inside the archive; trusting the store's field
would be trusting a number nobody checked.

Skip files whose sha256 already exists — content-addressed, so a republished
version with unchanged assets costs nothing.

**A failed fetch for one extension must not fail the sync pass.** The catalog row
still updates; that extension simply has no assets for this version, and
`/api/admin/views` omits its views with a diagnostic. Sync processes many
extensions and one bad one cannot be allowed to hold the rest.

### The route

```
GET /api/admin/extensions/{ext_id}/views/{view_id}/*path
```

Path normalisation rejects `..`, absolute paths and backslashes **before** the
lookup, so a traversal attempt is reported as traversal rather than as a missing
file. Then serve `bytes` inline, or 307 to a presigned S3 URL.

Two rules carried over from `/uploads`, both already paid for there:

- **Branch on the row's `s3_key` alone, never on current config.** A row written
  while S3 was on has `bytes = NULL`; if S3 is later turned off, that row must
  answer **503**, not 404. The bytes exist and we cannot reach them, and 404
  would read to a tenant as "your page is gone" when it is a config regression.
- **Verify at write, trust at read.** `set_s3_key` cannot confirm its write
  landed, and re-hashing on every read buys nothing against the threats we have.

Headers, on this route only:

```
Content-Type: <from an extension allowlist>
X-Content-Type-Options: nosniff
Content-Security-Policy: default-src 'none'; script-src 'self';
  style-src 'self' 'unsafe-inline'; img-src 'self' data:;
  connect-src 'none'; frame-ancestors 'self'
```

`connect-src 'none'` is deliberate: all network egress goes through the bridge.
`X-Frame-Options` is deliberately **absent** — it would block the very iframe
this feature depends on; `frame-ancestors` states the intent without that trap.

**Gate:** authenticated operator, no tenant scoping. View assets are published
extension code, already public in the store — they are not tenant data. The gate
that matters is on the bridge, which carries data. Scoping the bytes would add a
query per file for a page that fetches many, and protect nothing that is not
already public.

## The bridge

### `callApi`, without duplicating RBAC

The rule is: effective grant = declared allowlist ∩ the caller's own RBAC. The
tempting implementation re-checks permissions inside the bridge handler — and it
would drift from `classify_admin_route` within weeks, exactly as `navItems.ts`
warns its own `tiers`/`capability` fields drift from the Rust classifier.

Instead, the handler synthesises a `Request` and dispatches it through the same
Admin router as a `tower::Service`, carrying the same `OperatorCtx`.
`auth_middleware` and `operator_authz_guard` then run unchanged. The bridge adds
exactly one filter *before* that: the path must match the extension's
`permissions.ui.platformApi`. No rule is restated; the bridge can only narrow,
never widen.

### `fetch`

Server-side proxy against `permissions.ui.fetchHosts`, reusing the address
validation `permissions.network` already applies: https only, loopback,
link-local and cloud-metadata addresses rejected. The response streams back
through the bridge. No credential ever reaches the browser.

### The `invokeTool` gap

Admin has **no executor for design-extension tools.** It carries no
`greentic-ext-runtime`. It does have a wasm executor — `greentic-mcp-exec`, and
`register_local_wasm` already performs store-fetch → Ed25519 verify → wasmtime
`list_tools` — but that is the MCP-router world, a different WIT world from
`greentic:extension-design/tools.invoke-tool`.

This phase returns a typed `E_TOOL_UNSUPPORTED_ON_SURFACE` for `invokeTool` on
the admin surface, surfaced by `/api/admin/views` so a tenant admin sees "this
view needs tool execution that Admin does not provide yet" rather than a page
that silently does nothing. `gtdx lint` should warn an author who declares
`tools` on an `admin`-surface view.

**This is a deferral, not the end state**, and the reason matters enough to
record: `callApi` and `fetch` cannot use a tenant's secret. A proxied fetch is
issued by our server but injects no tenant credential, so a page needing that
tenant's API token has only two options — receive the token in the browser,
which violates the rule this whole design rests on, or not work. `invokeTool` is
the only path where logic runs inside the sandbox with the host's `secrets.get`
and the browser sees only a result. The most obvious admin-surface pages —
"configure extension X for this tenant", "usage dashboard for X per tenant" —
need exactly that.

So Admin should eventually gain the ability to execute design-extension tools.
It is deferred because doing so means Admin providing host implementations for
`secrets`, `http`, `llm`, `broker` and `i18n`, plus the verification path and
capability registry — effectively making Admin a second extension host. Building
that speculatively, before one real view runs, is the most expensive possible way
to find out whether we built the right thing. Adding the executor later is purely
additive: the contract does not change, only the host's capability grows.

### Audit

Every bridge call writes one `extension_bridge_audit` row: extension, view,
operator, tenant, call kind, method, target, outcome, duration.

## Navigation

One catch-all route per surface, and that is the only route-table change:

```tsx
<Route path="/x/:extId/:viewId" element={<ExtensionViewHost />} />
```

What is dynamic is the menu. `GET /api/admin/views` returns views already
resolved for the current operator, tenant and team — the four-step cascade runs
server-side, and only what the caller may see reaches the frontend.

That is deliberate beyond tidiness. `navItems.ts` carries a comment warning that
its `tiers` and `capability` fields must be kept "in step" by hand with the Rust
classifier: two copies of one permission rule, held together by discipline. For
views there is no second copy to drift.

Two merge points, one per slot:

- `admin.sidebar` → merged into `navGroups()` ahead of the `tiers` /
  `hiddenByCapability()` filters. Server-side filtering means they pass through.
- `admin.tenantDetail` → merged into `TAB_GROUPS` in `TenantDetail.tsx`, rendered
  by the existing `GroupNavLayout`. The `/tenants/:id/:tab` route already supplies
  a tenant, so the bridge `init` carries `tenantId` with no extra machinery.

Ordering breaks on `sort_order`, then extension id, then view id — total and
stable, so two extensions choosing the same number never swap between renders.
Two views in the same slot and path is not an error: both appear, ordered.
Rejecting a collision would let an extension installed later block one already
there.

An unresolvable slot mounts the view under an "Extensions" section at the top
level of its surface, with a diagnostic. Never dropped silently. `gtdx lint` only
*warns* on an unknown slot precisely because its snapshot goes stale by
construction; the host serves `GET /api/views/slots` as the catalogue's source of
truth.

Two things that are easy to miss:

- Views must reach the **search palette**. A page nobody can find effectively
  does not exist, and Admin already maintains `SETTINGS_NAV` for exactly this.
- A view whose **assets failed to sync** is omitted with a diagnostic rather than
  rendered as an empty iframe.

## Placement UI

One decision shapes the rest: **placement is not arranged from a per-extension
list.**

The existing `ExtensionsTab.tsx` handles enablement — "this extension, for this
tenant: on or off". That is a per-extension task and stays there, unchanged.
Arranging navigation is a *cross-extension* task: you cannot order a section
while looking at one extension. So a new **Navigation** sub-tab under Tenant
Detail shows the resolved navigation tree with every extension's views in place —
move between sections, reorder, toggle per view.

Three things surface in that UI, each with an existing precedent to follow:

- A view **locked** by a platform admin shows a lock and the reason, not a
  control that quietly does nothing.
- A view whose **assets failed to sync** shows its diagnostic in place, so the
  person seeing it is the person who can act.
- **Team scoping** per view follows the `TeamMembersDialog` / `TeamsTab` pattern
  already used for extension overrides.

## Security model

| Threat | Control |
|---|---|
| View script reads host session, cookies or DOM | `sandbox="allow-scripts"` without `allow-same-origin` → opaque origin |
| View escalates privilege through the bridge | Request re-enters `auth_middleware` → `operator_authz_guard`; the allowlist only narrows |
| Secrets reach the browser | `init` carries nothing secret; no credentialed path exists on this surface at all |
| Tampered assets | sha256 verified by Admin at sync against the manifest inside the archive |
| Path traversal out of the asset dir | Normalisation before lookup; traversal reported as traversal |
| Assets pulling unverified remote code | CSP on the asset route; `E_VIEW_REMOTE_ASSET` at lint time |
| SSRF through the fetch proxy | `ui.fetchHosts` allowlist plus the existing `permissions.network` address rules |
| Spoofed bridge messages | Page verifies `event.source`; host verifies the frame's identity |
| Silent misuse | Every bridge call audited, including outbound fetch |

Accepted residual risk: a malicious view can exhaust the permissions of the
operator viewing it — anything that operator could have done by hand. Containment
is entitlement and audit, not the sandbox.

## Testing

- **Migrations**: each table's constraints, and that the retention delete removes
  only non-current, non-previous versions.
- **Sync**: assets materialise; a failed fetch for one extension leaves the others
  intact and the catalog row updated; a sha256 mismatch rejects the file; a
  re-sync with unchanged assets fetches nothing.
- **Asset route**: traversal rejected as traversal; an `s3_key` row with S3
  disabled answers 503 and not 404; correct Content-Type; CSP present;
  `X-Frame-Options` absent.
- **Bridge**: a `callApi` inside the allowlist but outside the operator's RBAC is
  rejected — that is the case that matters and the easiest to omit; a path
  outside the allowlist is rejected; `invokeTool` returns the typed error;
  `fetch` to a host outside `fetchHosts` is rejected, as is one resolving to a
  loopback or metadata address.
- **Resolution**: table-driven over the four steps, covering team-beats-tenant,
  tenant-beats-default, and `locked` freezing placement while leaving `enabled`
  writable.
- **Navigation**: an unresolvable slot lands in "Extensions" and emits a
  diagnostic — assert it is not dropped; ordering is stable across renders when
  two views share a `sort_order`.
- **Audit**: every bridge call kind writes a row; the reaper removes rows past 90
  days and nothing newer.

## Work breakdown

1. **store-server** — the `view-assets` endpoint. Independent of everything else
   here; can start immediately. (The schema fix that unblocks publishing is
   already in flight separately.)
2. **Admin, backend** — migrations, sync-time materialisation and retention, the
   asset route, the bridge with its audit, `/api/admin/views` and
   `/api/views/slots`, the `TenantScoped` config routes.
3. **Admin, frontend** — `ExtensionViewHost`, the nav merge into `navGroups()` and
   `TAB_GROUPS`, the search-palette entries, and the Navigation sub-tab.

2 and 3 share a repo but not a language; they can run in parallel once the
`/api/admin/views` response shape is fixed. Fix that shape first, in writing.

4. **greentic-designer-sdk** — one small follow-up this phase creates: `gtdx
   lint` should warn when a view declares `tools` on the `admin` surface, since
   Admin cannot execute them. Trivial next to the rest, and easy to forget
   because it lives in a repo this phase otherwise does not touch.

## Open risks

- **Admin becomes a second extension host** if the `invokeTool` deferral turns out
  to be untenable sooner than expected. The deferral is cheap to reverse but not
  cheap to implement; the trigger to watch for is authors shipping admin views
  that cannot do their job without a tenant credential.
- **Schema drift keeps recurring.** The store-server copy of `describe-v2.json`
  already drifted and blocked publishing. The remedy is for the store to consume
  the schema embedded in `greentic-extension-sdk-contract` rather than keep a
  copy — worth doing during this phase, since the store is being changed anyway.
- **Audit volume.** Logging every read is a deliberate choice with a real cost.
  The 90-day reaper is sized from a guess, not from measurement; revisit once
  there is traffic.
- **Bridge re-entry through the router** is the right design and also the
  least-travelled path in Axum. If synthesising a request through
  `tower::Service` proves fragile, the fallback is an explicit internal dispatch
  table — but that reintroduces the duplication this design exists to avoid, so
  it needs a deliberate decision rather than a quiet slide.
