# Extension-contributed views (custom pages) in Designer and Admin

Date: 2026-08-26
Status: design approved, implementation not started

## Problem

An extension today can contribute node types, tools, recipes, knowledge,
prompts, schemas, DW providers and guardrails. It cannot contribute a screen.
Anything an extension wants a human to look at has to be squeezed into a node
inspector form or shipped as a separate product.

We want an extension to be able to contribute a whole page — its own HTML, JS
and CSS — into the Greentic Designer and, with much finer targeting, into the
Greentic Admin console: available to some tenants and not others, to some teams
and not others, placed under a chosen section or sub-tab, and orderable
alongside the pages the product itself ships.

## Non-goals

- A page builder. Authors write their own HTML/JS; we do not render a page from
  a declarative widget vocabulary.
- Replacing `JsonSchemaForm.tsx`. Node inspectors keep their schema-driven forms.
- Anything to do with the Designer's existing `/pages` route (an unshipped
  block-based content editor for end-user flows). Different concept, and the
  reason this feature is named `views` rather than `pages`.
- A global Content-Security-Policy for Designer or Admin. We add CSP headers on
  the new asset route only. Retrofitting CSP across two apps that have never had
  one is its own project and would break existing pages.

## Decisions

| Question | Decision |
|---|---|
| New `ExtensionKind`? | No. A new `contributions.views[]` slot, usable by any kind. |
| Where does the UI come from? | Author-supplied HTML/JS/CSS bundled in the `.gtxpack`. |
| How is it isolated? | `<iframe sandbox="allow-scripts">`, no `allow-same-origin` → opaque origin. |
| Data access | Host bridge over `postMessage`: extension's own tools, platform REST, and proxied third-party HTTP. |
| Who decides placement? | Two layers: platform admin grants the tenant, tenant admin places it and scopes it per team. |
| Config source of truth | Admin's Postgres, for both surfaces. Designer pulls over an API. |

### Why a contribution slot and not a new kind

A new `ExtensionKind` would have to be added to `ExtensionKind::ALL`, to
`scan_kind_dir` discovery in `greentic-ext-runtime`, to `EXTENSION_KIND_ALLOWED`
in the Admin store-artifact validator, to the store server's publish allowlist,
and to the `kind` enum in `describe-v2.json` — five hand-maintained lists across
four repositories. This project has already been bitten by exactly that: `gtdx
uninstall` shipped a hand-written kind list that omitted `Provider`, so provider
extensions could not be removed at all while the command reported success.

The argument for a separate kind is that a view-bearing artifact contains
third-party browser code and a reviewer should see that immediately. But a
`DesignExtension` that also contributes a view ships the same browser code. The
risk travels with the contribution, not with the install directory. So the
review signal belongs on the contribution and in `runtime.permissions`, which is
where a reviewer already looks.

### Why opaque origin rather than a separate origin

A dedicated hostname is stronger isolation, but it is a DNS/TLS/deploy decision
across two products, not a code change. `sandbox="allow-scripts"` without
`allow-same-origin` gives the frame an opaque origin regardless of which host
serves the bytes: no access to host cookies, `localStorage`, or the parent DOM.

The cost is stated plainly in "Security model" below: an opaque origin means the
host cannot pin `targetOrigin` on `postMessage`, and it means the page's own
`fetch()` sends `Origin: null`.

## Contract changes (greentic-designer-sdk)

### `contributions.views[]`

New module `crates/greentic-extension-sdk-contract/src/describe/contributions/view.rs`,
wired into `Contributions` alongside the existing eight lists. `Contributions` is
`deny_unknown_fields`, and `describe-v2.json` sets `additionalProperties: false`
on `contributions`, so both need the explicit addition.

```rust
pub struct View {
    /// Unique within the extension. The host namespaces it as
    /// `<extension_id>/<id>`.
    pub id: String,
    /// Which host this view targets. A view that belongs in both declares
    /// two entries — placement differs per surface anyway.
    pub surface: Surface,
    /// i18n key resolved against the existing top-level `localization` block,
    /// with a literal fallback for when the key is missing.
    pub title_key: String,
    pub title_fallback: String,
    pub icon: Option<String>,
    /// Entry HTML inside the pack, relative to `assets/views/<id>/`.
    pub entry: String,
    /// The author's *suggested* placement. Every layer below may override it.
    pub placement: Placement,
    /// Floor on who may see this view at all. The real gate is admin config.
    pub min_visibility: Visibility,
    /// Names of this extension's own contributed tools the view may invoke.
    pub tools: Vec<String>,
}

pub enum Surface { Designer, Admin }

pub enum Visibility { Member, TenantAdmin, PlatformAdmin }

pub struct Placement {
    /// Host-defined mount point: "designer.sidebar", "admin.sidebar",
    /// "admin.tenantDetail".
    pub slot: String,
    /// Section/group path under the slot. Empty means top level of the slot.
    pub path: Vec<String>,
    /// Sort hint within the parent. Ties broken by extension id, then view id,
    /// so ordering is total and stable.
    pub order: Option<i32>,
}
```

`slot` and `path` are strings, not closed enums. The Admin's tab set is a
hand-written TypeScript array that changes with the product, while
`describe.json` is signed and immutable once published. A closed enum in a signed
artifact rots exactly the way the kind lists did. The safety net is behavioural,
not type-level, and is specified under "Unresolvable placement" below.

`min_visibility` deliberately has three values rather than mirroring either
host's vocabulary. The Designer speaks `role` / `is_operator` / `is_admin`; the
Admin speaks tiers (`platform` / `partnership` / `tenant`) plus capabilities.
Unifying those two in the contract would be a permanent tax for no benefit,
because the author's declaration is only a floor — the operative gate is tenant
and team configuration.

Mapping is fixed by the host, not by the author: `Member` → any authenticated
user of the tenant; `TenantAdmin` → Admin tier `tenant` and above, which
includes `partnership`; `PlatformAdmin` → Admin tier `platform`, or
`is_operator` in the Designer. `partnership` gets no floor of its own — an
author cannot express "resellers but not tenants", and if that need turns out
to be real it is a fourth value, not a reinterpretation of these three.

### `runtime.permissions.ui`

`Permissions` is `deny_unknown_fields`, so this is an explicit struct + schema
addition:

```rust
pub struct UiPermissions {
    /// Hosts the view may reach through the host's server-side proxy.
    /// Same validation as `permissions.network`: https only, loopback and
    /// link-local rejected.
    pub fetch_hosts: Vec<String>,
    /// Platform REST endpoints the view may call through the bridge.
    pub platform_api: Vec<ApiGrant>,
}

pub struct ApiGrant {
    pub method: String,     // GET | POST | PUT | PATCH | DELETE
    pub path_pattern: String,
}
```

`permissions.network` is **not** reused. It authorises `http.fetch` from inside
the WASM guest, where the caller is the extension's own logic. `ui.fetch_hosts`
authorises requests a *human clicking in a browser* can trigger, and the response
lands in browser-executed code. Same SSRF validation rules, different blast
radius, so a reviewer gets to see them as two separate lines.

Grants are declared extension-wide rather than per view so that a reviewer reads
one block instead of N. Per-view least privilege is a future refinement, not a
launch requirement.

### Pack layout

The packer already walks `assets/`, `i18n/`, `schemas/` and `prompts/` and
copies them verbatim into the `.gtxpack`
(`crates/greentic-extension-sdk-cli/src/dev/packer/mod.rs:249-269`), and
`manifest.json` already records sha256 and byte length for every entry. No
packer change is needed, and view assets get tamper-evidence for free. As of
today no extension anywhere uses `assets/` — this is the first consumer.

```
assets/views/<view-id>/index.html
assets/views/<view-id>/app.js
assets/views/<view-id>/style.css
```

The existing caps apply unchanged: `MAX_ENTRY_BYTES` 64 MiB, `MAX_ARCHIVE_BYTES`
256 MiB.

### `gtdx lint` rules

- `entry` must resolve to an entry that actually exists in the pack. **Error.**
  A view whose HTML is missing is a broken install, not a warning.
- `views[].id` unique within the extension. **Error.**
- `<script src="http…">` / `<link href="http…">` in an entry HTML. **Error.**
  Assets must ship inside the pack; otherwise `manifest.json` integrity is
  theatre — the sha256 covers a file that then pulls unverified code at runtime.
- Unknown `placement.slot`. **Warning**, listing the known slots. The SDK carries
  a snapshot of the host slot catalogue and snapshots go stale; a stale snapshot
  must not fail a build.
- `tools[]` naming a tool the extension does not contribute. **Error.**

### Scaffold

`gtdx new` gains a `--with-view` flag that adds `assets/views/<id>/` with a
working example page, the `views[]` entry in `describe.json.tmpl`, and a vendored
`bridge.js`. Following the 1.2.7/1.2.8 precedent, the example must actually do
something and ship a test — a scaffold that renders an empty page teaches
nothing.

## Bridge protocol (v1)

The view never receives credentials. It asks for results.

Host → page, once, on load:

```jsonc
{ "v": 1, "type": "init",
  "locale": "id",
  "theme": "dark",
  "surface": "admin",
  "context": { "tenantId": "...", "teamId": "...", "displayName": "..." },
  "config": { /* per-placement config set by the tenant admin */ } }
```

Page → host, correlated by `id`:

```jsonc
{ "v": 1, "id": "c1", "type": "invokeTool", "name": "sync_contacts", "args": {} }
{ "v": 1, "id": "c2", "type": "callApi", "method": "GET", "path": "/api/flows" }
{ "v": 1, "id": "c3", "type": "fetch", "url": "https://api.example.com/things" }
{ "v": 1, "type": "resize", "height": 1240 }
{ "v": 1, "type": "navigate", "to": "/tenants/abc" }
{ "v": 1, "type": "toast", "level": "error", "message": "…" }
```

Host → page: `{ "v": 1, "id": "c1", "type": "result", "ok": true, "data": … }`
or `{ …, "ok": false, "error": { "code": "permission_denied", "message": "…" } }`.

Three rules that are not negotiable:

1. **`init` carries nothing secret.** An opaque origin cannot be named as a
   `targetOrigin`, so the host must post with `"*"`. Anything in `init` is
   readable by any code that ends up in that frame.
2. **The host verifies `event.source === iframeEl.contentWindow`**, not
   `event.origin` — the origin is the string `"null"` and proves nothing.
3. **Effective grant = declared allowlist ∩ the calling user's RBAC.** The
   bridge is never a privilege-escalation path. An extension declaring
   `GET /api/admin/tenants/*`, opened by an ordinary tenant user, still sees only
   that user's own tenant. The bridge re-enters the existing guard chain
   (`auth_middleware` → `operator_authz_guard`) rather than bypassing it.

`fetch` is proxied server-side rather than issued by the frame. This is forced by
the opaque origin: a frame without `allow-same-origin` sends `Origin: null`, which
most third-party APIs reject at CORS. Proxying also keeps any credential on the
server and makes third-party calls auditable. The proxy reuses the
`permissions.network` SSRF validation (https only; loopback, link-local and
metadata addresses rejected) and enforces `ui.fetch_hosts`.

## Asset delivery

### Designer — serve from the unpacked directory

The Designer has the pack on disk at
`~/.greentic/extensions/<kind>/<name>-<version>/`.

```
GET /api/extensions/{ext_id}/views/{view_id}/*path
```

- Path normalisation reusing the zip-slip guard already in `src/ui/bundled.rs`.
- sha256 checked against `manifest.json` before bytes are returned.
- Content-Type from an extension allowlist; `X-Content-Type-Options: nosniff`.
- `Content-Security-Policy` set **on this route only** — `default-src 'none'`,
  `script-src 'self'`, `style-src 'self' 'unsafe-inline'`, `img-src 'self' data:`,
  `connect-src 'none'` (all network egress goes through the bridge), and
  `frame-ancestors 'self'` so only the host app may frame it. Note that
  `X-Frame-Options: DENY` must **not** be set here: it would block the very
  iframe this feature depends on. `frame-ancestors` is the control that
  expresses the intent without that trap.

### Admin — materialise at sync time

The Admin never has the pack. It holds metadata rows in Postgres synced from the
store server (`src/routes/admin/extensions/sync.rs`); its `describe.json` parsing
(`src/store_artifact/extension.rs`) serves the operator upload/validation path
only.

View assets are therefore materialised during catalog sync into blob storage,
keyed `(extension_id, version, path)`, sha256-verified against the pack manifest
on write. This reuses the storage pattern already behind `/uploads/{id}`
(`src/routes/assets.rs:353`) — Postgres BLOB with S3 redirect. A new
`migrations_pg/` migration adds the table.

The rejected alternative is per-request proxying to the store server. It needs no
new storage, but it makes every page load depend on a third service being up.
That is a failure that shows up in production, not in development.

## Navigation

No dynamic route table is needed. One catch-all route per host:

```tsx
<Route path="/x/:extId/:viewId" element={<ExtensionViewHost />} />
```

What is dynamic is the menu. `GET /api/views` (Designer) and
`GET /api/admin/views` (Admin) return the views already resolved for the current
user, tenant and team — filtered server-side, so the frontend does not restate
RBAC rules. The frontend adds one hook, `useExtensionViews()`, merged into
`NAV_GROUPS` (`greentic-designer/web/src/components/layout/navItems.ts`) and
`navGroups()` (`greentic-designer-admin/web/src/lib/navItems.ts`), plus support
for the `admin.tenantDetail` slot through the existing `GroupNavLayout.tsx`.

This is also a small repayment of existing debt: `navItems.ts` in the Admin
carries a comment warning that its `tiers` / `capability` fields must be kept "in
step" by hand with the Rust route classifier. For views, the server decides and
there is no hand-mirrored copy to drift.

### Slot catalogue

Each host serves `GET /api/views/slots`. The SDK embeds a snapshot for `gtdx
lint`. Snapshots go stale by construction, which is why an unknown slot is a lint
warning rather than an error.

### Unresolvable placement

If `placement.slot` or `placement.path` does not resolve on the host, the view is
mounted under an "Extensions" section at the top level of its surface and a
diagnostic is recorded. It is never silently dropped. A page that vanishes with
no trace is the failure mode this codebase has been burned by before — a
339-line wall of `✓` in `gtdx doctor` once concealed an entire unchecked kind.

## Placement and visibility configuration

### Layer 1 — platform admin (nothing new)

Whether a tenant may have an extension at all is already `tenant_extensions`,
with `PlatformOnly` routes. Unchanged.

### Layer 2 — tenant admin (two new tables)

```
tenant_view_placements (tenant_id, extension_id, view_id,
                        slot, path, sort_order, enabled, config_json)
team_view_overrides    (team_id, extension_id, view_id, enabled)
```

Routes follow the split `team_extensions.rs` already established: these are
`TenantScoped`, so a tenant admin arranges their own house without touching
platform entitlement.

### Resolution

For "does user U, in tenant T, team G, see view V?":

1. Is the extension entitled to T? (`team_extensions` → `tenant_extensions` →
   global default.) If not, stop.
2. Enabled? `team_view_overrides` → `tenant_view_placements.enabled` → the
   author's default (enabled).
3. Does U clear `min_visibility`?
4. Placement = `tenant_view_placements` row if present, else the `placement`
   from `describe.json`.

Step 2 deliberately mirrors the existing extension cascade — team overrides
tenant overrides default — so operators learn one rule, not two.

### Cross-service configuration

Admin's Postgres is the single source of truth for **both** surfaces. The
Designer reads `designer`-surface configuration from an Admin API, caches it, and
falls back to the author's declared defaults when Admin is unreachable. A
degraded Designer shows extension views in their default placement rather than
losing them.

## Security model

| Threat | Control |
|---|---|
| View script reads host session/cookies/DOM | `sandbox="allow-scripts"` without `allow-same-origin` → opaque origin |
| View escalates privilege via the bridge | Effective grant = declared allowlist ∩ caller's RBAC; bridge re-enters the existing guard chain |
| Secrets leak into the browser | `init` carries nothing secret; all credentialed calls execute host-side |
| Tampered assets | sha256 verified against the signed `manifest.json` on every serve |
| Path traversal out of the asset dir | Normalisation reusing the existing zip-slip guard |
| Assets pulling unverified remote code | Lint error on remote `<script>`/`<link>`; CSP on the asset route |
| SSRF through the fetch proxy | `ui.fetch_hosts` allowlist + the existing `permissions.network` address rules |
| Spoofed bridge messages | `event.source === iframeEl.contentWindow` |

Accepted residual risk: a malicious view can still exhaust the user's own
permissions — it can call anything the signed-in user could have called by hand.
Containment for that is entitlement (do not grant the tenant an extension you do
not trust) and audit, not the sandbox.

## Testing

- **Contract**: round-trip and `deny_unknown_fields` tests for `View`,
  `Placement`, `UiPermissions`, matching the existing per-contribution test files
  (`tests/contributions_*.rs`); v2 schema validation for a describe carrying
  `views[]`; a fixture rejecting a duplicate `view_id`.
- **CLI**: lint tests for each rule above, both the failing and the passing
  direction. A lint rule with only a passing test pins nothing.
- **Packer**: a pack containing `assets/views/**` round-trips byte-identically
  and every asset appears in `manifest.json`.
- **Asset route** (both hosts): traversal attempt rejected; sha256 mismatch
  rejected; correct Content-Type; CSP header present.
- **Bridge**: message from a foreign window rejected; `callApi` outside the
  declared allowlist rejected; `callApi` inside the allowlist but outside the
  user's RBAC rejected — that third one is the case that matters and the one
  easiest to omit.
- **Resolution**: a table-driven test over the four-step cascade, covering
  team-override-beats-tenant and tenant-beats-default.
- **Unresolvable placement**: view lands in "Extensions" and emits a diagnostic;
  assert it is not dropped.

## Work breakdown by repository

1. **greentic-designer-sdk** — `View`/`Placement`/`UiPermissions` types, v2 schema,
   lint rules, `--with-view` scaffold + `bridge.js`, docs. Self-contained and
   independently shippable; everything else depends on it.
2. **greentic-designer-extensions** (`greentic-ext-runtime`) — surface `views[]`
   through the loader so hosts can read it. No new kind directory to scan.
3. **greentic-designer-admin** — sync-time asset materialisation + migration,
   asset route, bridge endpoints, the two config tables and their `TenantScoped`
   routes, `/api/admin/views`, nav merge, `ExtensionViewHost`, tenant-admin
   placement UI. Note `EXTENSION_KIND_ALLOWED` needs **no** change — that is the
   point of not adding a kind. What does need checking is that the operator
   upload validator in `src/store_artifact/extension.rs` accepts a describe
   carrying `contributions.views[]` rather than rejecting the unknown field.
4. **greentic-designer** — asset route over the unpacked dir, bridge endpoints,
   `/api/views` reading config from Admin with cache + fallback, nav merge,
   `ExtensionViewHost`.
5. **store server** — carry view assets through publish and expose them to Admin
   sync.

Suggested order: 1 → 2 → 3 → 4, with 5 folded into 3. The Admin surface goes
first because that is where the advanced targeting requirement actually lives;
the Designer surface reuses the bridge and host component wholesale.

## Open risks

- **Slot catalogue drift.** The SDK snapshot and the host catalogue will diverge.
  Mitigated to a warning, but authors will still ship views into slots that no
  longer exist. Worth a store-side check at publish once the catalogue stabilises.
- **Asset size in Admin's Postgres.** Materialising every version's assets grows
  unboundedly. Needs a retention rule — likely "keep the versions any tenant is
  entitled to" — before this ships to a large catalogue.
- **No per-team RBAC axis exists.** `OperatorCtx` stops at tenant granularity.
  Team-level view visibility piggybacks on tenant-scoped routes exactly as
  `team_extensions.rs` does. If a real per-team role model ever lands, this needs
  revisiting.
- **The Designer → Admin dependency is new.** The fallback keeps views visible,
  but this is the first runtime coupling in that direction and deserves an
  explicit timeout and circuit-breaker rather than a bare HTTP call.
