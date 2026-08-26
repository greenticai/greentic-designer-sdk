# Extension Views — Phase 1 (SDK) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Teach the Greentic extension contract to carry a `contributions.views[]` entry — an author-supplied HTML/JS page shipped inside the `.gtxpack` — so that a describe declaring one validates, lints, round-trips and can be scaffolded, before any host learns to render it.

**Architecture:** A new typed contribution (`View`) next to the existing eight, a new `runtime.permissions.ui` grant block, matching JSON Schema, cross-field invariants in the `DescribeJson` deserializer, filesystem-aware `gtdx lint` rules in their own module, and a `gtdx new --with-view` scaffold overlay. The packer already ships `assets/` verbatim and `manifest.json` already hashes every entry, so asset integrity needs no new code.

**Tech Stack:** Rust 2024 edition, `serde` + `serde_json`, `jsonschema` (Draft 2020-12), `include_dir` for templates, `clap` for CLI args, `walkdir`, `tempfile` for tests.

**Spec:** `docs/superpowers/specs/2026-08-26-extension-views-design.md`

## Global Constraints

- Non-test source files stay **under 500 lines**. `crates/greentic-extension-sdk-cli/src/commands/lint/rules.rs` is already at 498 — new lint rules go in a **new module**, never appended there. This mirrors the existing `rules_secret_key.rs` split. Test files are not held to it in this repo (`packer/tests.rs` is 823 lines, `lint/tests.rs` 637), so append to those normally.
- Every contribution struct is `#[serde(deny_unknown_fields)]`. A typo in a describe must be a hard parse error, never a silently ignored field.
- Optional/additive fields use `#[serde(default, skip_serializing_if = ...)]` so an absent field stays absent on re-serialization. A re-serialized describe must not sprout nulls or defaults the store's schema never saw — `describe.json` gets signed, so any field the SDK invents on the way out changes the signed bytes.
- Inner contribution structs use **plain snake_case** JSON keys (`title_key`, `runtime_ref`). Only the top-level `Contributions` field names are camelCase (`nodeTypes`, `dwProviders`), via `rename_all = "camelCase"` on that struct alone.
- No `unwrap()` / `panic!()` in library code. Tests may.
- Lint codes are stable strings: `E_*` for errors, `W_*` for warnings. Tests assert on the code, not the message.
- Conventional commits (`feat:`, `fix:`, `test:`, `docs:`).
- Verify with `cargo test --workspace --all-features --locked`; full gate is `ci/local_check.sh` (fmt, clippy `-D warnings`, test, release build, publish dry-run).

### Contract vocabulary (fixed by the spec, copied verbatim)

- `Surface` — `designer` | `admin`
- `Visibility` — `member` | `tenant_admin` | `platform_admin`
- Known placement slots — `designer.sidebar`, `admin.sidebar`, `admin.tenantDetail`
- Asset location inside the pack — `assets/views/<view-id>/`
- Bridge protocol version — `1`

---

### Task 1: `View` contribution type

**Files:**
- Create: `crates/greentic-extension-sdk-contract/src/describe/contributions/view.rs`
- Modify: `crates/greentic-extension-sdk-contract/src/describe/contributions.rs`
- Modify: `crates/greentic-extension-sdk-contract/src/describe/mod.rs:9-12` (the `pub use contributions::{...}` re-export list)
- Test: `crates/greentic-extension-sdk-contract/tests/contributions_view.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `greentic_extension_sdk_contract::describe::{View, Surface, Visibility, Placement}`; `Contributions.views: Vec<View>`.

- [ ] **Step 1: Write the failing test**

Create `crates/greentic-extension-sdk-contract/tests/contributions_view.rs`:

```rust
//! `contributions.views[]` — a UI page an extension contributes to a host
//! surface. These tests pin the full field set, because a field missing from
//! this struct is unreachable in production no matter what the author writes:
//! `deny_unknown_fields` turns it into a parse error, and the describe is
//! signed, so it cannot be patched after the fact.

use greentic_extension_sdk_contract::describe::{Contributions, Surface, View, Visibility};

fn full_view_json() -> serde_json::Value {
    serde_json::json!({
        "id": "usage-dashboard",
        "surface": "admin",
        "title_key": "view.usage_dashboard.label",
        "title_fallback": "Usage",
        "icon": "bar-chart",
        "entry": "index.html",
        "placement": {
            "slot": "admin.tenantDetail",
            "path": ["access", "teams"],
            "order": 20
        },
        "min_visibility": "tenant_admin",
        "tools": ["fetch_usage"]
    })
}

#[test]
fn full_view_declaration_parses() {
    let v: View = serde_json::from_value(full_view_json()).expect("parses");
    assert_eq!(v.id, "usage-dashboard");
    assert_eq!(v.surface, Surface::Admin);
    assert_eq!(v.entry, "index.html");
    assert_eq!(v.placement.slot, "admin.tenantDetail");
    assert_eq!(v.placement.path, vec!["access", "teams"]);
    assert_eq!(v.placement.order, Some(20));
    assert_eq!(v.min_visibility, Visibility::TenantAdmin);
    assert_eq!(v.tools, vec!["fetch_usage"]);
}

#[test]
fn round_trip_preserves_every_field() {
    let original = full_view_json();
    let v: View = serde_json::from_value(original.clone()).expect("parses");
    let back = serde_json::to_value(&v).expect("serializes");
    assert_eq!(back, original);
}

/// The minimum an author must write. Everything else defaults, and the
/// defaults must not appear on the way back out — the describe is signed.
#[test]
fn minimal_view_parses_and_stays_minimal() {
    let minimal = serde_json::json!({
        "id": "hello",
        "surface": "designer",
        "title_key": "view.hello.label",
        "title_fallback": "Hello",
        "entry": "index.html",
        "placement": { "slot": "designer.sidebar" }
    });
    let v: View = serde_json::from_value(minimal.clone()).expect("parses");
    assert_eq!(v.min_visibility, Visibility::Member, "default floor is member");
    assert!(v.icon.is_none());
    assert!(v.tools.is_empty());
    assert!(v.placement.path.is_empty());
    assert!(v.placement.order.is_none());

    let back = serde_json::to_value(&v).expect("serializes");
    assert_eq!(
        back, minimal,
        "absent fields must stay absent — a re-serialized describe must not \
         sprout defaults the signature never covered"
    );
}

#[test]
fn unknown_view_field_is_rejected() {
    let typo = serde_json::json!({
        "id": "hello",
        "surface": "designer",
        "title_key": "k",
        "title_fallback": "Hello",
        "entry": "index.html",
        "placement": { "slot": "designer.sidebar" },
        "min_visibilty": "member"
    });
    let err = serde_json::from_value::<View>(typo).unwrap_err();
    assert!(
        err.to_string().contains("min_visibilty"),
        "the rejected field should be named: {err}"
    );
}

#[test]
fn unknown_surface_is_rejected() {
    let bad = serde_json::json!({
        "id": "hello",
        "surface": "mobile",
        "title_key": "k",
        "title_fallback": "Hello",
        "entry": "index.html",
        "placement": { "slot": "designer.sidebar" }
    });
    assert!(serde_json::from_value::<View>(bad).is_err());
}

/// `views` is additive: every describe written before it existed must still
/// parse, and an empty list must not serialize.
#[test]
fn contributions_without_views_parse_and_omit_the_key() {
    let c: Contributions = serde_json::from_value(serde_json::json!({})).expect("parses");
    assert!(c.views.is_empty());
    let s = serde_json::to_string(&c).expect("serializes");
    assert!(!s.contains("views"), "empty views must not serialize: {s}");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-contract --test contributions_view`
Expected: FAIL to compile — `unresolved import ... View`.

- [ ] **Step 3: Write the type**

Create `crates/greentic-extension-sdk-contract/src/describe/contributions/view.rs`:

```rust
//! `View` — a UI page an extension contributes to a Greentic host surface.
//!
//! The page's HTML/JS/CSS ship inside the `.gtxpack` under
//! `assets/views/<id>/`; the host serves them and renders the entry in a
//! sandboxed iframe with an opaque origin. Everything the page is allowed to
//! reach is declared in `runtime.permissions.ui`, not here — a reviewer reads
//! one grant block rather than one per view.

use serde::{Deserialize, Serialize};

/// Which host application the view targets. A view that belongs in both
/// declares two entries: placement differs per surface anyway, so a single
/// entry could never carry both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    Designer,
    Admin,
}

/// Floor on who may see the view at all. Deliberately three values rather
/// than mirroring either host's vocabulary (the Designer speaks
/// `role`/`is_operator`, the Admin speaks tiers plus capabilities): the
/// author's declaration is only a floor, and the operative gate is tenant and
/// team configuration held by the host.
///
/// Host mapping is fixed, not author-chosen: `Member` is any authenticated
/// user of the tenant, `TenantAdmin` is Admin tier `tenant` and above (which
/// includes `partnership`), `PlatformAdmin` is Admin tier `platform` or
/// `is_operator` in the Designer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    #[default]
    Member,
    TenantAdmin,
    PlatformAdmin,
}

/// Where the author suggests the view appears. Every configuration layer may
/// override it, so this is a default and not a demand.
///
/// `slot` and `path` are strings rather than a closed enum on purpose. The
/// hosts' tab sets are hand-written arrays that change with the product, while
/// `describe.json` is signed and immutable once published — a closed enum in a
/// signed artifact rots exactly the way this project's hard-coded kind lists
/// did. The safety net is behavioural: `gtdx lint` warns on an unknown slot,
/// and a host that cannot resolve a placement mounts the view under an
/// "Extensions" section with a diagnostic rather than dropping it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Placement {
    /// Host-defined mount point: `designer.sidebar`, `admin.sidebar`,
    /// `admin.tenantDetail`.
    pub slot: String,
    /// Section/group path under the slot. Empty means the top level of the
    /// slot.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<String>,
    /// Sort hint within the parent. Hosts break ties by extension id then view
    /// id, so ordering is total and stable even when two extensions pick the
    /// same number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct View {
    /// Unique within the extension. The host namespaces it as
    /// `<extension_id>/<id>`.
    pub id: String,
    pub surface: Surface,
    /// Key resolved against the top-level `localization` block.
    pub title_key: String,
    /// Literal shown when `title_key` has no entry for the active locale.
    pub title_fallback: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Entry HTML, relative to `assets/views/<id>/` inside the pack.
    pub entry: String,
    pub placement: Placement,
    #[serde(default, skip_serializing_if = "is_default_visibility")]
    pub min_visibility: Visibility,
    /// Names of this extension's own contributed tools the view may invoke
    /// through the host bridge. Every name must appear in
    /// `contributions.tools[].name`; the deserializer enforces that.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
}

fn is_default_visibility(v: &Visibility) -> bool {
    matches!(v, Visibility::Member)
}
```

- [ ] **Step 4: Wire it into `Contributions`**

In `crates/greentic-extension-sdk-contract/src/describe/contributions.rs`:

Change the module doc first line from `//! Typed `contributions` block. Eight children, each its own typed list, plus` to `//! Typed `contributions` block. Nine children, each its own typed list, plus`.

Add to the `pub mod` list (alphabetical position, after `tool`):

```rust
pub mod view;
```

Add to the `pub use` list:

```rust
pub use view::{Placement, Surface, View, Visibility};
```

Add the field to `struct Contributions`, after `guardrails`:

```rust
    /// UI pages contributed to a host surface. Rendered by the Designer or the
    /// Admin from assets shipped under `assets/views/<id>/` in the pack.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<View>,
```

In `crates/greentic-extension-sdk-contract/src/describe/mod.rs`, extend the re-export:

```rust
pub use contributions::{
    Contributions, DwProvider, Knowledge, NodeType, OutputPort, Placement, Prompt, Recipe, Schema,
    Surface, Tool, View, Visibility,
};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p greentic-extension-sdk-contract --test contributions_view`
Expected: PASS, 7 tests.

Then run the neighbours that assert on the whole `Contributions` shape, because adding a field can break them:
Run: `cargo test -p greentic-extension-sdk-contract --test contributions_shell --test describe_roundtrip --test describe_v2`
Expected: PASS. `empty_contributions_serialise_to_empty_object` in `contributions_shell.rs` is the one that would catch a missing `skip_serializing_if`.

- [ ] **Step 6: Commit**

```bash
git add crates/greentic-extension-sdk-contract/src/describe/contributions/view.rs \
        crates/greentic-extension-sdk-contract/src/describe/contributions.rs \
        crates/greentic-extension-sdk-contract/src/describe/mod.rs \
        crates/greentic-extension-sdk-contract/tests/contributions_view.rs
git commit -m "feat(contract): add contributions.views[] for extension-contributed UI pages"
```

---

### Task 2: `runtime.permissions.ui` grant block

**Files:**
- Modify: `crates/greentic-extension-sdk-contract/src/describe/mod.rs:263-288` (`struct Permissions`)
- Test: `crates/greentic-extension-sdk-contract/tests/permissions_ui.rs`

**Interfaces:**
- Consumes: nothing from Task 1.
- Produces: `greentic_extension_sdk_contract::describe::{UiPermissions, ApiGrant}`; `Permissions.ui: Option<UiPermissions>`.

- [ ] **Step 1: Write the failing test**

Create `crates/greentic-extension-sdk-contract/tests/permissions_ui.rs`:

```rust
//! `runtime.permissions.ui` — what a contributed view is allowed to reach.
//!
//! Kept separate from `permissions.network` on purpose. `network` authorises
//! `http.fetch` from inside the WASM guest, where the caller is the
//! extension's own logic. `ui.fetchHosts` authorises requests a human clicking
//! in a browser can trigger, and the response lands in browser-executed code.
//! Same SSRF rules, different blast radius — a reviewer must see two lines.

use greentic_extension_sdk_contract::describe::{ApiGrant, Permissions};

#[test]
fn ui_permissions_parse() {
    let p: Permissions = serde_json::from_value(serde_json::json!({
        "network": [],
        "secrets": [],
        "callExtensionKinds": [],
        "ui": {
            "fetchHosts": ["https://api.example.com/*"],
            "platformApi": [{"method": "GET", "path_pattern": "/api/flows"}]
        }
    }))
    .expect("parses");

    let ui = p.ui.expect("ui block present");
    assert_eq!(ui.fetch_hosts, vec!["https://api.example.com/*"]);
    assert_eq!(
        ui.platform_api,
        vec![ApiGrant {
            method: "GET".to_string(),
            path_pattern: "/api/flows".to_string()
        }]
    );
}

/// Additive: every describe written before `ui` existed must parse, and must
/// not gain the key on the way back out.
#[test]
fn permissions_without_ui_round_trip_unchanged() {
    let original = serde_json::json!({ "network": [], "secrets": [], "callExtensionKinds": [] });
    let p: Permissions = serde_json::from_value(original.clone()).expect("parses");
    assert!(p.ui.is_none());
    let back = serde_json::to_value(&p).expect("serializes");
    assert_eq!(back, original);
}

#[test]
fn unknown_ui_field_is_rejected() {
    let typo = serde_json::json!({
        "network": [], "secrets": [], "callExtensionKinds": [],
        "ui": { "fetchHost": ["https://api.example.com/*"] }
    });
    let err = serde_json::from_value::<Permissions>(typo).unwrap_err();
    assert!(
        err.to_string().contains("fetchHost"),
        "the rejected field should be named: {err}"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-contract --test permissions_ui`
Expected: FAIL to compile — `unresolved import ... ApiGrant`.

- [ ] **Step 3: Write the types**

In `crates/greentic-extension-sdk-contract/src/describe/mod.rs`, add the field to `struct Permissions` after `oauth_providers`:

```rust
    /// What a contributed `contributions.views[]` page may reach. Absent means
    /// the extension contributes no view, or contributes one that only renders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<UiPermissions>,
```

And immediately after the `Permissions` struct:

```rust
/// Grants that apply to browser-executed view code, not to the WASM guest.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiPermissions {
    /// Hosts a view may reach through the host's server-side proxy. The view
    /// never issues these itself: an iframe without `allow-same-origin` sends
    /// `Origin: null`, which most third-party APIs reject at CORS, and
    /// proxying keeps any credential on the server. Validated exactly like
    /// `permissions.network` — https only, loopback and link-local rejected.
    #[serde(rename = "fetchHosts", default, skip_serializing_if = "Vec::is_empty")]
    pub fetch_hosts: Vec<String>,
    /// Platform REST endpoints a view may call through the bridge. The host
    /// intersects this with the calling user's own RBAC, so the list can only
    /// ever narrow what that user could already do by hand.
    #[serde(
        rename = "platformApi",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub platform_api: Vec<ApiGrant>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiGrant {
    /// `GET`, `POST`, `PUT`, `PATCH` or `DELETE`. Constrained by the JSON
    /// Schema rather than by a Rust enum, so a describe naming a method this
    /// crate version does not know still round-trips instead of failing the
    /// whole parse.
    pub method: String,
    /// Path pattern, e.g. `/api/flows` or `/api/admin/tenants/*`.
    pub path_pattern: String,
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p greentic-extension-sdk-contract --test permissions_ui`
Expected: PASS, 3 tests.

Run: `cargo test -p greentic-extension-sdk-contract`
Expected: PASS — nothing else asserts on the full `Permissions` shape, but confirm rather than assume.

- [ ] **Step 5: Commit**

```bash
git add crates/greentic-extension-sdk-contract/src/describe/mod.rs \
        crates/greentic-extension-sdk-contract/tests/permissions_ui.rs
git commit -m "feat(contract): add runtime.permissions.ui grants for contributed views"
```

---

### Task 3: Cross-field invariants in the deserializer

**Files:**
- Modify: `crates/greentic-extension-sdk-contract/src/describe/mod.rs` (`impl TryFrom<DescribeJsonRaw> for DescribeJson`, the block that already validates `runtime_ref`)
- Test: `crates/greentic-extension-sdk-contract/tests/describe_views_invariants.rs`

**Interfaces:**
- Consumes: `Contributions.views` (Task 1), `Tool.name`.
- Produces: no new public API. Two new rejection reasons from `DescribeJson::deserialize`.

Why here and not in `gtdx lint`: the existing precedent is that structural cross-references (`node_type.runtime_ref`, `tool.runtime_ref`) are enforced in `TryFrom`, so **every** consumer gets them — the designer loading an installed describe, the store validating an upload, `gtdx validate`. A lint rule only protects authors who run lint. Lint gets the checks `TryFrom` cannot do, namely the ones that need the project directory on disk (Task 5).

- [ ] **Step 1: Write the failing test**

Create `crates/greentic-extension-sdk-contract/tests/describe_views_invariants.rs`:

```rust
//! Cross-field invariants for `contributions.views[]`, enforced at
//! deserialize time so every consumer gets them — not just authors who
//! remember to run `gtdx lint`.

use greentic_extension_sdk_contract::describe::DescribeJson;

fn describe_with(views: serde_json::Value, tools: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "apiVersion": "greentic.ai/v2",
        "kind": "DesignExtension",
        "compat": {
            "min_designer_version": ">=1.2.0",
            "min_runner_version": "^1.2.0",
            "contract_version": "1.2.0"
        },
        "metadata": {
            "id": "greentic.example",
            "name": "example",
            "version": "0.1.0",
            "summary": "s",
            "author": { "name": "a" },
            "license": "Apache-2.0"
        },
        "capabilities": { "offered": [], "required": [] },
        "runtime": {
            "components": {
                "main": {
                    "gtpack": {
                        "file": "extension.wasm",
                        "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
                        "pack_id": "greentic.example",
                        "component_version": "0.1.0"
                    },
                    "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
                    "world": "greentic:example/extension@1.0.0"
                }
            },
            "permissions": { "network": [], "secrets": [], "callExtensionKinds": [] }
        },
        "contributions": { "views": views, "tools": tools }
    })
}

fn view(id: &str, tools: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "surface": "designer",
        "title_key": "k",
        "title_fallback": "T",
        "entry": "index.html",
        "placement": { "slot": "designer.sidebar" },
        "tools": tools
    })
}

fn tool(name: &str) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "export": "greentic:extension-design/tools.invoke-tool",
        "runtime_ref": "main"
    })
}

#[test]
fn valid_views_accepted() {
    let d = describe_with(
        serde_json::json!([view("a", serde_json::json!(["fetch_usage"]))]),
        serde_json::json!([tool("fetch_usage")]),
    );
    let parsed: DescribeJson = serde_json::from_value(d).expect("parses");
    assert_eq!(parsed.contributions.views.len(), 1);
}

#[test]
fn duplicate_view_id_rejected() {
    let d = describe_with(
        serde_json::json!([
            view("dash", serde_json::json!([])),
            view("dash", serde_json::json!([]))
        ]),
        serde_json::json!([]),
    );
    let err = serde_json::from_value::<DescribeJson>(d).unwrap_err().to_string();
    assert!(err.contains("dash"), "the duplicate id should be named: {err}");
}

#[test]
fn view_naming_an_undeclared_tool_rejected() {
    let d = describe_with(
        serde_json::json!([view("dash", serde_json::json!(["ghost_tool"]))]),
        serde_json::json!([tool("fetch_usage")]),
    );
    let err = serde_json::from_value::<DescribeJson>(d).unwrap_err().to_string();
    assert!(
        err.contains("ghost_tool"),
        "the dangling tool should be named: {err}"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-contract --test describe_views_invariants`
Expected: `valid_views_accepted` PASSES; the other two FAIL (no error is raised).

- [ ] **Step 3: Add the invariants**

In `impl TryFrom<DescribeJsonRaw> for DescribeJson`, immediately after the existing `for tool in &raw.contributions.tools { ... }` loop and before `Ok(DescribeJson { ... })`:

```rust
        // View ids namespace to `<extension_id>/<view_id>` on the host, so a
        // duplicate would make two different pages collide on one route.
        let mut seen_views = std::collections::BTreeSet::new();
        for view in &raw.contributions.views {
            if !seen_views.insert(view.id.as_str()) {
                return Err(format!(
                    "contributions.views[] declares duplicate id {:?}",
                    view.id
                ));
            }
        }

        // A view may only invoke tools this same extension contributes. A
        // dangling name would fail at the bridge, at runtime, in the browser —
        // the worst place to discover it.
        let tool_names: std::collections::BTreeSet<&str> = raw
            .contributions
            .tools
            .iter()
            .map(|t| t.name.as_str())
            .collect();
        for view in &raw.contributions.views {
            for wanted in &view.tools {
                if !tool_names.contains(wanted.as_str()) {
                    return Err(format!(
                        "view {:?} lists tool {:?}, which is not in contributions.tools",
                        view.id, wanted
                    ));
                }
            }
        }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p greentic-extension-sdk-contract --test describe_views_invariants`
Expected: PASS, 3 tests.

Run: `cargo test -p greentic-extension-sdk-contract`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/greentic-extension-sdk-contract/src/describe/mod.rs \
        crates/greentic-extension-sdk-contract/tests/describe_views_invariants.rs
git commit -m "feat(contract): reject duplicate view ids and dangling view tool refs"
```

---

### Task 4: JSON Schema (`describe-v2.json`)

**Files:**
- Modify: `crates/greentic-extension-sdk-contract/schemas/describe-v2.json` (the `contributions.properties` object, and `runtime.properties.permissions.properties`)
- Test: `crates/greentic-extension-sdk-contract/tests/schema_v2_views.rs`

**Interfaces:**
- Consumes: the vocabulary from Tasks 1–2.
- Produces: schema validation for `views` and `permissions.ui` through the existing `validate_describe_json`.

Note `contributions` sets `additionalProperties: false`, so **without** this task a describe carrying `views` fails schema validation even though it deserializes fine. Task 1 alone is not shippable.

- [ ] **Step 1: Write the failing test**

Create `crates/greentic-extension-sdk-contract/tests/schema_v2_views.rs`:

```rust
//! `contributions` is `additionalProperties: false`, so a new contribution
//! slot is invisible to the schema until it is declared there. Without this,
//! a describe that deserializes perfectly still fails `gtdx validate`.

use greentic_extension_sdk_contract::schema::validate_describe_json;

fn base() -> serde_json::Value {
    serde_json::json!({
        "apiVersion": "greentic.ai/v2",
        "kind": "DesignExtension",
        "compat": {
            "min_designer_version": ">=1.2.0",
            "min_runner_version": "^1.2.0",
            "contract_version": "1.2.0"
        },
        "metadata": {
            "id": "greentic.example",
            "name": "example",
            "version": "0.1.0",
            "summary": "s",
            "author": { "name": "a" },
            "license": "Apache-2.0"
        },
        "capabilities": { "offered": [], "required": [] },
        "runtime": {
            "components": {
                "main": {
                    "gtpack": {
                        "file": "extension.wasm",
                        "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
                        "pack_id": "greentic.example",
                        "component_version": "0.1.0"
                    },
                    "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
                    "world": "greentic:example/extension@1.0.0"
                }
            },
            "permissions": { "network": [], "secrets": [], "callExtensionKinds": [] }
        },
        "contributions": {}
    })
}

#[test]
fn describe_with_a_view_validates() {
    let mut d = base();
    d["contributions"]["views"] = serde_json::json!([{
        "id": "usage-dashboard",
        "surface": "admin",
        "title_key": "view.usage.label",
        "title_fallback": "Usage",
        "entry": "index.html",
        "placement": { "slot": "admin.sidebar", "path": ["Governance"], "order": 10 },
        "min_visibility": "tenant_admin",
        "tools": []
    }]);
    validate_describe_json(&d).expect("a describe carrying views must validate");
}

#[test]
fn describe_with_ui_permissions_validates() {
    let mut d = base();
    d["runtime"]["permissions"]["ui"] = serde_json::json!({
        "fetchHosts": ["https://api.example.com/*"],
        "platformApi": [{ "method": "GET", "path_pattern": "/api/flows" }]
    });
    validate_describe_json(&d).expect("a describe carrying permissions.ui must validate");
}

#[test]
fn view_missing_required_field_is_rejected() {
    let mut d = base();
    d["contributions"]["views"] = serde_json::json!([{
        "id": "no-entry",
        "surface": "admin",
        "title_key": "k",
        "title_fallback": "T",
        "placement": { "slot": "admin.sidebar" }
    }]);
    assert!(
        validate_describe_json(&d).is_err(),
        "a view without `entry` must not validate"
    );
}

#[test]
fn unknown_surface_is_rejected_by_schema() {
    let mut d = base();
    d["contributions"]["views"] = serde_json::json!([{
        "id": "x",
        "surface": "mobile",
        "title_key": "k",
        "title_fallback": "T",
        "entry": "index.html",
        "placement": { "slot": "admin.sidebar" }
    }]);
    assert!(validate_describe_json(&d).is_err());
}

#[test]
fn unknown_api_grant_method_is_rejected_by_schema() {
    let mut d = base();
    d["runtime"]["permissions"]["ui"] = serde_json::json!({
        "platformApi": [{ "method": "TRACE", "path_pattern": "/api/flows" }]
    });
    assert!(validate_describe_json(&d).is_err());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-contract --test schema_v2_views`
Expected: `describe_with_a_view_validates` and `describe_with_ui_permissions_validates` FAIL (`additionalProperties`); the three rejection tests pass vacuously for the wrong reason — they will be meaningful only after Step 3.

- [ ] **Step 3: Add `views` to the schema**

In `crates/greentic-extension-sdk-contract/schemas/describe-v2.json`, inside `contributions.properties`, after the `guardrails` entry and before `connection_test`, insert:

```json
        "views": {
          "description": "UI pages the extension contributes to a host surface. The page's assets ship in the pack under `assets/views/<id>/`; the host serves them and renders `entry` in a sandboxed iframe with an opaque origin. What the page may reach is declared once in `runtime.permissions.ui`, not per view.",
          "type": "array",
          "items": {
            "type": "object",
            "additionalProperties": false,
            "required": ["id", "surface", "title_key", "title_fallback", "entry", "placement"],
            "properties": {
              "id": {
                "description": "Unique within the extension. The host namespaces it as `<extension_id>/<id>`.",
                "type": "string",
                "pattern": "^[a-z0-9][a-z0-9._-]*$"
              },
              "surface": {
                "description": "Which host application the view targets. A view that belongs in both declares two entries, because placement differs per surface.",
                "enum": ["designer", "admin"]
              },
              "title_key": {
                "description": "Key resolved against the top-level `localization` block.",
                "type": "string"
              },
              "title_fallback": {
                "description": "Literal shown when `title_key` has no entry for the active locale.",
                "type": "string"
              },
              "icon": {
                "description": "Host-resolved icon name.",
                "type": "string"
              },
              "entry": {
                "description": "Entry HTML relative to `assets/views/<id>/` inside the pack. `gtdx lint` reports a missing file as `E_VIEW_ENTRY_MISSING` and a path that escapes the view directory as `E_VIEW_ENTRY_PATH`.",
                "type": "string"
              },
              "placement": {
                "description": "The author's suggested placement. Every configuration layer may override it. `slot` and `path` are free strings rather than an enum because the hosts' tab sets change with the product while a published describe is signed and immutable; `gtdx lint` reports an unknown slot as the warning `W_VIEW_SLOT_UNKNOWN`, and a host that cannot resolve a placement mounts the view under an \"Extensions\" section with a diagnostic rather than dropping it.",
                "type": "object",
                "additionalProperties": false,
                "required": ["slot"],
                "properties": {
                  "slot": { "type": "string" },
                  "path": { "type": "array", "items": { "type": "string" } },
                  "order": { "type": "integer" }
                }
              },
              "min_visibility": {
                "description": "Floor on who may see the view. Only a floor: the operative gate is the tenant and team configuration the host holds. `tenant_admin` covers the Admin `partnership` tier as well.",
                "enum": ["member", "tenant_admin", "platform_admin"]
              },
              "tools": {
                "description": "Names of this extension's own contributed tools the view may invoke through the host bridge. Every name must appear in `contributions.tools[].name` — the Rust deserializer rejects a dangling one.",
                "type": "array",
                "items": { "type": "string" }
              }
            }
          }
        },
```

- [ ] **Step 4: Add `ui` to the permissions schema**

In the same file, inside `runtime.properties.permissions.properties`, after the `llmRoles` entry, insert:

```json
            "ui": {
              "description": "Grants that apply to browser-executed view code, not to the WASM guest. Kept separate from `network` on purpose: `network` authorises `http.fetch` from inside the guest, where the caller is the extension's own logic, while these authorise requests a human clicking in a browser can trigger, whose responses land in browser-executed code.",
              "type": "object",
              "additionalProperties": false,
              "properties": {
                "fetchHosts": {
                  "description": "Hosts a view may reach through the host's server-side proxy. The view never issues these itself: an iframe without `allow-same-origin` sends `Origin: null`, which most third-party APIs reject at CORS. Same address rules as `network` - https only, loopback and link-local rejected.",
                  "type": "array",
                  "items": { "type": "string" }
                },
                "platformApi": {
                  "description": "Platform REST endpoints a view may call through the bridge. The host intersects this with the calling user's own RBAC, so it can only narrow what that user could already do by hand - never widen it.",
                  "type": "array",
                  "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["method", "path_pattern"],
                    "properties": {
                      "method": { "enum": ["GET", "POST", "PUT", "PATCH", "DELETE"] },
                      "path_pattern": { "type": "string" }
                    }
                  }
                }
              }
            },
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p greentic-extension-sdk-contract --test schema_v2_views`
Expected: PASS, 5 tests.

Run: `cargo test -p greentic-extension-sdk-contract`
Expected: PASS — `schema_v2_validate.rs` and `templates_schema_conformance.rs` both exercise this schema and must stay green.

- [ ] **Step 6: Commit**

```bash
git add crates/greentic-extension-sdk-contract/schemas/describe-v2.json \
        crates/greentic-extension-sdk-contract/tests/schema_v2_views.rs
git commit -m "feat(contract): validate contributions.views[] and permissions.ui in describe-v2 schema"
```

---

### Task 5: `gtdx lint` rules for views

**Files:**
- Create: `crates/greentic-extension-sdk-cli/src/commands/lint/rules_views.rs`
- Modify: `crates/greentic-extension-sdk-cli/src/commands/lint/mod.rs` (module doc, `mod` list, `use`, `collect_violations` signature, `run`)
- Test: `crates/greentic-extension-sdk-cli/src/commands/lint/tests.rs` (append)

**Interfaces:**
- Consumes: the `views` shape from Task 1; `super::Violation`, `Violation::error`, `Violation::warning` from `lint/mod.rs`.
- Produces: `pub(super) const KNOWN_SLOTS: [&str; 3]`; `pub(super) fn check_views(describe: &serde_json::Value, dir: &Path) -> Vec<Violation>`. New codes: `E_VIEW_ENTRY_MISSING`, `E_VIEW_ENTRY_PATH`, `E_VIEW_REMOTE_ASSET`, `W_VIEW_SLOT_UNKNOWN`.
- **Signature change other callers must follow:** `collect_violations` gains a `dir: &Path` parameter, becoming `fn collect_violations(describe: &serde_json::Value, dir: &Path, home: &Path, publish: bool) -> Vec<Violation>`.

These are exactly the checks Task 3's deserializer cannot do, because they need the project directory on disk. `rules.rs` is at 498 of a 500-line budget, so this is a new module — the same reason `rules_secret_key.rs` exists.

- [ ] **Step 1: Write the failing tests**

Append to `crates/greentic-extension-sdk-cli/src/commands/lint/tests.rs`:

```rust
use rules_views::check_views;

fn view_project(entry: &str, html: Option<&str>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    if let Some(body) = html {
        let asset_dir = dir.path().join("assets/views/hello");
        std::fs::create_dir_all(&asset_dir).unwrap();
        std::fs::write(asset_dir.join(entry), body).unwrap();
    }
    dir
}

fn describe_with_view(entry: &str, slot: &str) -> serde_json::Value {
    json!({
        "contributions": {
            "views": [{
                "id": "hello",
                "surface": "designer",
                "title_key": "k",
                "title_fallback": "Hello",
                "entry": entry,
                "placement": { "slot": slot }
            }]
        }
    })
}

#[test]
fn view_entry_present_is_clean() {
    let dir = view_project("index.html", Some("<h1>hi</h1><script src=\"app.js\"></script>"));
    let d = describe_with_view("index.html", "designer.sidebar");
    assert!(check_views(&d, dir.path()).is_empty());
}

#[test]
fn view_entry_missing_is_an_error() {
    let dir = view_project("index.html", None);
    let d = describe_with_view("index.html", "designer.sidebar");
    let v = check_views(&d, dir.path());
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].code, "E_VIEW_ENTRY_MISSING");
}

#[test]
fn view_entry_escaping_its_directory_is_an_error() {
    let dir = view_project("index.html", Some("<h1>hi</h1>"));
    let d = describe_with_view("../../../etc/passwd", "designer.sidebar");
    let v = check_views(&d, dir.path());
    assert!(
        v.iter().any(|x| x.code == "E_VIEW_ENTRY_PATH"),
        "traversal must be reported before the file is looked up: {v:?}"
    );
}

#[test]
fn remote_script_in_the_entry_is_an_error() {
    let dir = view_project(
        "index.html",
        Some("<script src=\"https://cdn.example.com/x.js\"></script>"),
    );
    let d = describe_with_view("index.html", "designer.sidebar");
    let v = check_views(&d, dir.path());
    assert!(
        v.iter().any(|x| x.code == "E_VIEW_REMOTE_ASSET"),
        "manifest integrity is theatre if the page pulls unverified code: {v:?}"
    );
}

#[test]
fn unknown_slot_is_a_warning_not_an_error() {
    let dir = view_project("index.html", Some("<h1>hi</h1>"));
    let d = describe_with_view("index.html", "admin.notARealSlot");
    let v = check_views(&d, dir.path());
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].code, "W_VIEW_SLOT_UNKNOWN");
    assert_eq!(
        v[0].severity,
        Severity::Warning,
        "the SDK's slot list is a snapshot and goes stale by construction — a \
         stale snapshot must never fail a build"
    );
}

#[test]
fn describe_without_views_is_clean() {
    let dir = tempfile::tempdir().expect("tempdir");
    let d = json!({ "contributions": {} });
    assert!(check_views(&d, dir.path()).is_empty());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p greentic-extension-sdk-cli --lib lint`
Expected: FAIL to compile — `unresolved import ... rules_views`.

- [ ] **Step 3: Write the rules module**

Create `crates/greentic-extension-sdk-cli/src/commands/lint/rules_views.rs`:

```rust
//! Lint rules for `contributions.views[]` — the checks that need the project
//! directory on disk, which neither the JSON Schema nor the `DescribeJson`
//! deserializer can perform. Structural cross-references (duplicate ids,
//! dangling tool names) live in the deserializer instead, so every consumer
//! gets them and not only authors who run lint.
//!
//! In its own module because `rules.rs` sits at 498 lines against a 500-line
//! budget — the same reason `rules_secret_key.rs` exists.

use std::path::Path;

use super::Violation;

/// Placement slots the hosts publish today.
///
/// This is a snapshot. Hosts serve the live list at `/api/views/slots`, and a
/// snapshot embedded in a released CLI goes stale by construction — which is
/// exactly why an unknown slot is a warning and never an error. An author on
/// an older `gtdx` must still be able to target a slot shipped last week.
pub(super) const KNOWN_SLOTS: [&str; 3] = [
    "designer.sidebar",
    "admin.sidebar",
    "admin.tenantDetail",
];

/// Markers for a remote asset reference in an entry HTML. Deliberately a
/// substring scan and not an HTML parse: this is a lint meant to catch the
/// obvious mistake, not a security boundary. The real boundary is the CSP the
/// host sets on the asset route.
const REMOTE_MARKERS: [&str; 6] = [
    "src=\"http",
    "src='http",
    "src=\"//",
    "href=\"http",
    "href='http",
    "href=\"//",
];

pub(super) fn check_views(describe: &serde_json::Value, dir: &Path) -> Vec<Violation> {
    let Some(views) = describe
        .pointer("/contributions/views")
        .and_then(|v| v.as_array())
    else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for view in views {
        let id = view.get("id").and_then(|v| v.as_str()).unwrap_or("<unnamed>");

        if let Some(slot) = view.pointer("/placement/slot").and_then(|v| v.as_str())
            && !KNOWN_SLOTS.contains(&slot)
        {
            out.push(Violation::warning(
                "W_VIEW_SLOT_UNKNOWN",
                format!(
                    "view {id:?} targets unknown slot {slot:?}; known slots are {}. \
                     This list is a snapshot — if the host has since added the slot, \
                     ignore this warning.",
                    KNOWN_SLOTS.join(", ")
                ),
            ));
        }

        let Some(entry) = view.get("entry").and_then(|v| v.as_str()) else {
            continue;
        };

        if entry.starts_with('/') || entry.split('/').any(|seg| seg == "..") {
            out.push(Violation::error(
                "E_VIEW_ENTRY_PATH",
                format!(
                    "view {id:?} entry {entry:?} escapes assets/views/{id}/; \
                     entry must be a relative path inside the view's own directory"
                ),
            ));
            continue;
        }

        let path = dir.join("assets/views").join(id).join(entry);
        let Ok(html) = std::fs::read_to_string(&path) else {
            out.push(Violation::error(
                "E_VIEW_ENTRY_MISSING",
                format!(
                    "view {id:?} declares entry {entry:?} but {} does not exist; \
                     a view whose HTML is missing is a broken install",
                    path.display()
                ),
            ));
            continue;
        };

        let lowered = html.to_lowercase();
        if let Some(marker) = REMOTE_MARKERS.iter().find(|m| lowered.contains(*m)) {
            out.push(Violation::error(
                "E_VIEW_REMOTE_ASSET",
                format!(
                    "view {id:?} entry {entry:?} references a remote asset ({marker}…); \
                     assets must ship inside the pack, otherwise the manifest sha256 \
                     covers a file that then pulls unverified code at runtime"
                ),
            ));
        }
    }
    out
}
```

- [ ] **Step 4: Wire the rule into `lint/mod.rs`**

Add to the module doc, after the `S3/D2 key-format rules` block:

```rust
//! View rules (August 2026), for `contributions.views[]`:
//! - `E_VIEW_ENTRY_PATH` — `entry` escapes `assets/views/<id>/`
//! - `E_VIEW_ENTRY_MISSING` — `entry` names a file that is not in the project
//! - `E_VIEW_REMOTE_ASSET` — the entry HTML pulls a script or stylesheet from
//!   a remote origin, which would defeat the pack manifest's integrity
//! - `W_VIEW_SLOT_UNKNOWN` — `placement.slot` is not in the CLI's snapshot of
//!   host slots. A warning, because the snapshot goes stale by construction.
```

Add to the `mod` list:

```rust
mod rules_views;
```

Add the import next to `use rules_secret_key::check_secret_key_canonical;`:

```rust
use rules_views::check_views;
```

Change `collect_violations` to take the project directory, and call the new rule:

```rust
fn collect_violations(
    describe: &serde_json::Value,
    dir: &Path,
    home: &Path,
    publish: bool,
) -> Vec<Violation> {
    let mut out = Vec::new();
    out.extend(check_version_semver(describe));
    out.extend(check_runtime_refs(describe));
    out.extend(check_capability_cycle(describe));
    out.extend(check_describe_diff_breaking(describe, home));
    out.extend(check_schema_host(describe));
    out.extend(check_export_form(describe));
    out.extend(check_engine_deprecated(describe));
    out.extend(check_id_pattern(describe));
    out.extend(check_tool_naming(describe));
    out.extend(check_sha256_zero(describe, publish));
    out.extend(check_perms_secrets_plain_key(describe));
    out.extend(check_secret_key_canonical(describe));
    out.extend(check_views(describe, dir));
    out
}
```

And update the single call site in `run`:

```rust
    let violations = collect_violations(&value, &args.dir, home, args.publish);
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p greentic-extension-sdk-cli --lib lint`
Expected: PASS, including the six new tests. Any existing test calling `collect_violations` directly must be updated to pass a directory — use `home.path()` for those, since they carry no view.

Run: `cargo test -p greentic-extension-sdk-cli --test lint_smoke --test lint_governance`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/greentic-extension-sdk-cli/src/commands/lint/rules_views.rs \
        crates/greentic-extension-sdk-cli/src/commands/lint/mod.rs \
        crates/greentic-extension-sdk-cli/src/commands/lint/tests.rs
git commit -m "feat(cli): lint contributions.views[] entry paths, assets and placement slots"
```

---

### Task 6: `gtdx new --with-view` scaffold and authoring docs

**Files:**
- Create: `crates/greentic-extension-sdk-cli/templates/view-addon/assets/views/hello/index.html.tmpl`
- Create: `crates/greentic-extension-sdk-cli/templates/view-addon/assets/views/hello/bridge.js.tmpl`
- Create: `crates/greentic-extension-sdk-cli/templates/view-addon/assets/views/hello/app.js.tmpl`
- Create: `crates/greentic-extension-sdk-cli/src/commands/new/view_addon.rs`
- Create: `docs/authoring-views.md`
- Modify: `crates/greentic-extension-sdk-cli/src/scaffold/template.rs` (new `include_dir!` static + loader fn)
- Modify: `crates/greentic-extension-sdk-cli/src/commands/new/mod.rs` (`--with-view` arg, `mod view_addon`, `render_templates` call site, mcp rejection)
- Test: `crates/greentic-extension-sdk-cli/tests/cli_new/views.rs`
- Modify: `crates/greentic-extension-sdk-cli/tests/cli_new/main.rs` (register the new module)

**Interfaces:**
- Consumes: `template::TemplateEntry`, `template::write_file`, `Context::render` (existing); the `views` and `permissions.ui` shapes from Tasks 1–2 and 4.
- Produces: `pub fn load_templates_view_addon() -> Vec<TemplateEntry>` in `scaffold::template`; `pub(super) fn add_view_to_describe(describe_json: &str, view_id: &str) -> anyhow::Result<String>` in `commands::new::view_addon`.

The describe is patched after rendering rather than shipped as an overlay template, because `overlay()` replaces whole files: a `view-addon/describe.json.tmpl` would have to duplicate every kind's describe template and would drift from all of them. `commands::openapi::author_describe_json` already establishes post-render describe authoring as the pattern here.

- [ ] **Step 1: Write the failing test**

Create `crates/greentic-extension-sdk-cli/tests/cli_new/views.rs`:

```rust
//! `gtdx new --with-view` must produce a project that lints and validates on
//! the first run. A scaffold that emits an empty page teaches nothing — the
//! same lesson 1.2.7 and 1.2.8 already paid for on the other kinds.

use std::process::Command;

use crate::fixtures::{gtdx_bin, run};

#[test]
fn scaffold_with_view_produces_a_lintable_project() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let target = tmp.path().join("viewy");

    let (ok, _out, err) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("viewy")
        .arg("--kind")
        .arg("design")
        .arg("--with-view")
        .arg("--no-git")
        .arg("--dir")
        .arg(&target));
    assert!(ok, "scaffold failed: {err}");

    let entry = target.join("assets/views/hello/index.html");
    assert!(entry.exists(), "example page must exist at {}", entry.display());
    assert!(target.join("assets/views/hello/bridge.js").exists());
    assert!(target.join("assets/views/hello/app.js").exists());

    let describe: serde_json::Value =
        serde_json::from_slice(&std::fs::read(target.join("describe.json")).expect("read describe"))
            .expect("parse describe");
    let views = describe["contributions"]["views"]
        .as_array()
        .expect("views array");
    assert_eq!(views.len(), 1);
    assert_eq!(views[0]["id"], "hello");
    assert_eq!(views[0]["entry"], "index.html");
    assert!(
        describe["runtime"]["permissions"]["ui"].is_object(),
        "a scaffolded view must come with its permissions.ui block"
    );

    let (lint_ok, _o, lint_err) = run(Command::new(gtdx_bin()).arg("lint").arg("--dir").arg(&target));
    assert!(lint_ok, "a fresh --with-view scaffold must lint clean: {lint_err}");
}

#[test]
fn scaffold_without_the_flag_ships_no_view() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let target = tmp.path().join("plain");

    let (ok, _out, err) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("plain")
        .arg("--kind")
        .arg("design")
        .arg("--no-git")
        .arg("--dir")
        .arg(&target));
    assert!(ok, "scaffold failed: {err}");

    assert!(!target.join("assets").exists(), "no view means no assets dir");
    let describe: serde_json::Value =
        serde_json::from_slice(&std::fs::read(target.join("describe.json")).expect("read describe"))
            .expect("parse describe");
    assert!(describe["contributions"].get("views").is_none());
}

#[test]
fn with_view_is_rejected_for_kind_mcp() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let target = tmp.path().join("routery");

    let (ok, _out, err) = run(Command::new(gtdx_bin())
        .arg("new")
        .arg("routery")
        .arg("--kind")
        .arg("mcp")
        .arg("--with-view")
        .arg("--no-git")
        .arg("--dir")
        .arg(&target));
    assert!(!ok, "mcp artifacts carry no contributions block at all");
    assert!(
        err.contains("--with-view"),
        "the error must name the flag it rejected: {err}"
    );
}
```

Register it in `crates/greentic-extension-sdk-cli/tests/cli_new/main.rs` by adding `mod views;` next to the other `mod` declarations.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo build -p greentic-extension-sdk-cli && cargo test -p greentic-extension-sdk-cli --test cli_new views`
Expected: FAIL — `error: unexpected argument '--with-view'`.

- [ ] **Step 3: Write the template files**

`crates/greentic-extension-sdk-cli/templates/view-addon/assets/views/hello/index.html.tmpl`:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <title>{{name}}</title>
    <link rel="stylesheet" href="style.css" />
  </head>
  <body>
    <main>
      <h1 id="title">{{name}}</h1>
      <p id="status">Connecting to the host…</p>
      <button id="ping" type="button">Call the extension</button>
      <pre id="output"></pre>
    </main>
    <script src="bridge.js"></script>
    <script src="app.js"></script>
  </body>
</html>
```

`crates/greentic-extension-sdk-cli/templates/view-addon/assets/views/hello/bridge.js.tmpl`:

```javascript
// Greentic view bridge, protocol v1.
//
// The page runs in an iframe with an opaque origin, so it can reach nothing on
// its own: no host cookies, no localStorage, no parent DOM, and its own
// fetch() would send `Origin: null`. Everything goes through the host, which
// holds the credentials and applies the caller's own permissions to each
// request. The bridge asks for results; it never receives keys.
(function (global) {
  const PROTOCOL = 1;
  const pending = new Map();
  let nextId = 0;
  let readyResolve;
  const ready = new Promise((resolve) => {
    readyResolve = resolve;
  });

  function send(message) {
    // targetOrigin cannot be pinned: an opaque origin has no name to pin to.
    // This is exactly why the host puts nothing secret in what it sends back.
    global.parent.postMessage(Object.assign({ v: PROTOCOL }, message), "*");
  }

  function call(type, payload) {
    const id = "c" + ++nextId;
    return new Promise((resolve, reject) => {
      pending.set(id, { resolve, reject });
      send(Object.assign({ id: id, type: type }, payload));
    });
  }

  global.addEventListener("message", (event) => {
    const msg = event.data;
    if (!msg || msg.v !== PROTOCOL) return;

    if (msg.type === "init") {
      readyResolve(msg);
      return;
    }
    if (msg.type === "result") {
      const slot = pending.get(msg.id);
      if (!slot) return;
      pending.delete(msg.id);
      if (msg.ok) slot.resolve(msg.data);
      else slot.reject(new Error(msg.error ? msg.error.message : "bridge call failed"));
    }
  });

  global.greentic = {
    protocol: PROTOCOL,
    /** Resolves with the host's `init` message: locale, theme, surface, context. */
    ready: ready,
    /** Invoke one of this extension's own tools, as listed in `views[].tools`. */
    invokeTool: (name, args) => call("invokeTool", { name: name, args: args || {} }),
    /** Call a platform endpoint listed in `permissions.ui.platformApi`. */
    callApi: (method, path, body) => call("callApi", { method: method, path: path, body: body }),
    /** Fetch a host listed in `permissions.ui.fetchHosts`, proxied server-side. */
    fetch: (url, options) => call("fetch", { url: url, options: options || {} }),
    /** Tell the host how tall the page is, so it can size the frame. */
    resize: (height) => send({ type: "resize", height: height }),
    /** Ask the host to navigate its own router. */
    navigate: (to) => send({ type: "navigate", to: to }),
    toast: (level, message) => send({ type: "toast", level: level, message: message }),
  };
})(window);
```

`crates/greentic-extension-sdk-cli/templates/view-addon/assets/views/hello/app.js.tmpl`:

```javascript
// Example view for {{name}}. Replace this with your own page.
(async function () {
  const status = document.getElementById("status");
  const output = document.getElementById("output");

  const init = await window.greentic.ready;
  status.textContent =
    "Connected — surface " + init.surface + ", locale " + init.locale + ".";
  window.greentic.resize(document.body.scrollHeight);

  document.getElementById("ping").addEventListener("click", async () => {
    try {
      const result = await window.greentic.invokeTool("echo", { message: "hello" });
      output.textContent = JSON.stringify(result, null, 2);
    } catch (err) {
      output.textContent = "Tool call failed: " + err.message;
      window.greentic.toast("error", err.message);
    }
    window.greentic.resize(document.body.scrollHeight);
  });
})();
```

Also create `crates/greentic-extension-sdk-cli/templates/view-addon/assets/views/hello/style.css.tmpl`:

```css
body {
  font-family: system-ui, -apple-system, "Segoe UI", sans-serif;
  margin: 0;
  padding: 1.5rem;
  color: #1a1a1a;
  background: transparent;
}
h1 { font-size: 1.25rem; margin: 0 0 0.5rem; }
button { padding: 0.4rem 0.9rem; cursor: pointer; }
pre { background: #f4f4f5; padding: 0.75rem; overflow-x: auto; }
@media (prefers-color-scheme: dark) {
  body { color: #f4f4f5; }
  pre { background: #27272a; }
}
```

- [ ] **Step 4: Register the template tree**

In `crates/greentic-extension-sdk-cli/src/scaffold/template.rs`, add the static next to the others:

```rust
static TEMPLATES_VIEW_ADDON: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/templates/view-addon");
```

And the loader, next to `load_templates_common`:

```rust
/// Assets for `gtdx new --with-view`. An additive overlay rather than a kind:
/// a view is a contribution, so it layers onto whichever kind the author chose
/// instead of replacing it.
pub fn load_templates_view_addon() -> Vec<TemplateEntry> {
    collect(&TEMPLATES_VIEW_ADDON)
}
```

- [ ] **Step 5: Write the describe patcher**

Create `crates/greentic-extension-sdk-cli/src/commands/new/view_addon.rs`:

```rust
//! Post-render describe authoring for `gtdx new --with-view`.
//!
//! The view is patched into the rendered `describe.json` rather than shipped
//! as a template overlay because `overlay()` replaces whole files: a
//! `view-addon/describe.json.tmpl` would have to duplicate every kind's
//! describe template and would drift from all of them. `commands::openapi`
//! already authors a describe this way.

/// Insert the example view and its `permissions.ui` block into a rendered
/// describe. Returns the re-serialized document.
pub(super) fn add_view_to_describe(describe_json: &str, view_id: &str) -> anyhow::Result<String> {
    let mut describe: serde_json::Value = serde_json::from_str(describe_json)
        .map_err(|e| anyhow::anyhow!("parse rendered describe.json: {e}"))?;

    let contributions = describe
        .get_mut("contributions")
        .and_then(|c| c.as_object_mut())
        .ok_or_else(|| anyhow::anyhow!("rendered describe.json has no contributions object"))?;

    contributions.insert(
        "views".to_string(),
        serde_json::json!([{
            "id": view_id,
            "surface": "designer",
            "title_key": format!("view.{view_id}.label"),
            "title_fallback": "Hello",
            "entry": "index.html",
            "placement": { "slot": "designer.sidebar" },
            "tools": ["echo"]
        }]),
    );

    let permissions = describe
        .pointer_mut("/runtime/permissions")
        .and_then(|p| p.as_object_mut())
        .ok_or_else(|| anyhow::anyhow!("rendered describe.json has no runtime.permissions"))?;
    permissions.insert(
        "ui".to_string(),
        serde_json::json!({ "fetchHosts": [], "platformApi": [] }),
    );

    Ok(serde_json::to_string_pretty(&describe)? + "\n")
}
```

- [ ] **Step 6: Wire the flag into `gtdx new`**

In `crates/greentic-extension-sdk-cli/src/commands/new/mod.rs`:

Add the module declaration next to `mod wizard;`:

```rust
mod view_addon;
```

Add the flag to `struct Args`, after `force`:

```rust
    /// Scaffold an example contributed view (a UI page) alongside the extension.
    #[arg(long, default_value_t = false)]
    pub with_view: bool,
```

Reject it for `mcp`, next to the existing `--from-openapi` guard (`mod.rs:566`):

```rust
    if args.with_view && kind == "mcp" {
        anyhow::bail!(
            "--with-view is not valid with --kind mcp: `wasix:mcp/router` artifacts \
             carry no contributions block at all"
        );
    }
```

Extend `render_templates` to layer the addon and patch the describe:

```rust
fn render_templates(
    ctx: &Context,
    kind: &str,
    target: &Path,
    with_view: bool,
) -> anyhow::Result<usize> {
    let mut files_written = 0usize;
    for entry in template::load_templates_common() {
        let dst = target.join(&entry.dst_rel);
        let rendered = ctx.render(std::str::from_utf8(entry.src_bytes)?)?;
        template::write_file(&dst, rendered.as_bytes())?;
        files_written += 1;
    }
    for entry in template::load_templates_kind(kind) {
        let dst = target.join(&entry.dst_rel);
        let rendered = ctx.render(std::str::from_utf8(entry.src_bytes)?)?;
        template::write_file(&dst, rendered.as_bytes())?;
        files_written += 1;
    }
    if with_view {
        for entry in template::load_templates_view_addon() {
            let dst = target.join(&entry.dst_rel);
            let rendered = ctx.render(std::str::from_utf8(entry.src_bytes)?)?;
            template::write_file(&dst, rendered.as_bytes())?;
            files_written += 1;
        }
        let describe_path = target.join("describe.json");
        let current = std::fs::read_to_string(&describe_path)
            .map_err(|e| anyhow::anyhow!("read {}: {e}", describe_path.display()))?;
        let authored = view_addon::add_view_to_describe(&current, "hello")?;
        template::write_file(&describe_path, authored.as_bytes())?;
    }
    Ok(files_written)
}
```

Update the single call site to pass `args.with_view`.

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo build -p greentic-extension-sdk-cli && cargo test -p greentic-extension-sdk-cli --test cli_new`
Expected: PASS, including the three new tests.

Run: `cargo test -p greentic-extension-sdk-cli --test templates_schema_conformance`
Expected: PASS — this test validates every template describe against the v2 schema, so a malformed patch shows up here.

- [ ] **Step 8: Write the authoring guide**

Create `docs/authoring-views.md`, following the shape of the existing `docs/authoring-secrets.md`:

```markdown
# Authoring views

A view is a UI page your extension contributes to the Greentic Designer or the
Greentic Admin console. You write the HTML, JS and CSS; they ship inside your
`.gtxpack`; the host serves them and renders your entry in a sandboxed iframe.

Scaffold one:

    gtdx new my-ext --kind design --with-view

## What ships

    assets/views/<view-id>/index.html
    assets/views/<view-id>/app.js
    assets/views/<view-id>/style.css

The packer copies `assets/` verbatim, and `manifest.json` records a sha256 for
every file, so your page is tamper-evident without you doing anything.

Everything your page loads must ship in that directory. `gtdx lint` rejects a
remote `<script>` or `<link>` with `E_VIEW_REMOTE_ASSET`: the manifest hash
would otherwise cover a file that pulls unverified code at runtime.

## Declaring it

    "contributions": {
      "views": [{
        "id": "usage-dashboard",
        "surface": "admin",
        "title_key": "view.usage.label",
        "title_fallback": "Usage",
        "entry": "index.html",
        "placement": { "slot": "admin.tenantDetail", "path": ["access"], "order": 20 },
        "min_visibility": "tenant_admin",
        "tools": ["fetch_usage"]
      }]
    }

`placement` is a suggestion. Platform admins decide which tenants get your
extension at all, and tenant admins decide where your view actually lands and
which of their teams can see it. `min_visibility` is a floor, not a guarantee.

Known slots: `designer.sidebar`, `admin.sidebar`, `admin.tenantDetail`. An
unknown slot is a lint warning rather than an error, because this list is a
snapshot in your `gtdx` build and hosts add slots between releases. A host that
cannot resolve your placement mounts the view under an "Extensions" section and
records a diagnostic — it will not disappear on you.

## What your page can reach

Your page runs with an opaque origin. It has no host cookies, no
`localStorage`, no access to the parent DOM, and its own `fetch()` would send
`Origin: null`. Everything goes through the bridge, and the host holds the
credentials:

    await greentic.ready                                  // locale, theme, surface, context
    await greentic.invokeTool("fetch_usage", { days: 30 }) // your own tool
    await greentic.callApi("GET", "/api/flows")            // platform REST
    await greentic.fetch("https://api.example.com/x")      // proxied server-side

The last three are gated by `runtime.permissions.ui`:

    "permissions": {
      "ui": {
        "fetchHosts": ["https://api.example.com/*"],
        "platformApi": [{ "method": "GET", "path_pattern": "/api/flows" }]
      }
    }

The host intersects that allowlist with the permissions of whoever is looking
at the page. Declaring `/api/admin/tenants/*` does not let an ordinary tenant
user read other tenants — the bridge can only ever narrow what that person
could already do by hand.

Never expect a secret to arrive in the browser. Ask the bridge for a result;
the credential stays on the server.

## Lint codes

| Code | Meaning |
|---|---|
| `E_VIEW_ENTRY_MISSING` | `entry` names a file that is not in your project |
| `E_VIEW_ENTRY_PATH` | `entry` escapes `assets/views/<id>/` |
| `E_VIEW_REMOTE_ASSET` | the entry HTML pulls a remote script or stylesheet |
| `W_VIEW_SLOT_UNKNOWN` | `placement.slot` is not in this `gtdx` build's snapshot |

Duplicate view ids and a `tools[]` entry naming a tool you do not contribute
are rejected when the describe is parsed, so they fail `gtdx validate` and
installation as well as lint.
```

Add a pointer to it in `README.md` next to the existing authoring links.

- [ ] **Step 9: Run the full gate**

Run: `ci/local_check.sh`
Expected: all checks pass — fmt, clippy with `-D warnings`, the full test suite, release build, and the two publish dry-runs.

- [ ] **Step 10: Commit**

```bash
git add crates/greentic-extension-sdk-cli/templates/view-addon \
        crates/greentic-extension-sdk-cli/src/commands/new/view_addon.rs \
        crates/greentic-extension-sdk-cli/src/commands/new/mod.rs \
        crates/greentic-extension-sdk-cli/src/scaffold/template.rs \
        crates/greentic-extension-sdk-cli/tests/cli_new/views.rs \
        crates/greentic-extension-sdk-cli/tests/cli_new/main.rs \
        docs/authoring-views.md README.md
git commit -m "feat(cli): scaffold an example contributed view with gtdx new --with-view"
```

---

### Task 7: Pin the packer's asset round-trip

**Files:**
- Modify: `crates/greentic-extension-sdk-cli/src/dev/packer/tests.rs` (append one test)

**Interfaces:**
- Consumes: `build_pack` and the module-local `make_project(root: &Path) -> PathBuf` helper already in that file, both in scope via its `use super::*`.
- Produces: nothing. No source change.

**No dependencies.** It can be executed first, and arguably should be: every
other task in this plan rests on the claim that the packer already ships
`assets/` and already hashes it, and that claim is currently pinned by nothing.

This is a unit test inside the packer module, not an integration test under
`tests/`, because `greentic-extension-sdk-cli` is a **binary-only crate** — it
declares `[[bin]] gtdx` and has no `lib.rs`, so nothing under `tests/` can call
`build_pack` at all. Every existing integration test there shells out to the
built binary instead. Appending here also reuses `make_project`, which already
writes a valid `describe.json` and a stub wasm.

The 500-line budget in this plan's Global Constraints applies to non-test
sources. `packer/tests.rs` is already 823 lines and `lint/tests.rs` 637, so
appending to either is consistent with how this repo already treats test
files — do not split them on account of the limit.

- [ ] **Step 1: Write the test**

Append to `crates/greentic-extension-sdk-cli/src/dev/packer/tests.rs`:

```rust
/// The packer already walks `assets/` and `manifest.json` already records a
/// sha256 for every entry, which is why contributed views need no packer
/// change at all. Nothing pinned that until now. If someone narrows the asset
/// walk later, this fails here instead of every published view silently losing
/// its HTML somewhere downstream.
#[test]
fn view_assets_round_trip_into_the_pack_and_the_manifest() {
    use std::io::Read as _;

    let tmp = tempfile::tempdir().unwrap();
    let wasm = make_project(tmp.path());

    let view_dir = tmp.path().join("assets/views/hello");
    std::fs::create_dir_all(&view_dir).unwrap();
    let html: &[u8] = b"<!doctype html><h1>hi</h1>\n";
    std::fs::write(view_dir.join("index.html"), html).unwrap();
    std::fs::write(view_dir.join("app.js"), b"console.log(1);\n").unwrap();

    let out = tmp.path().join("dist/demo-0.1.0.gtxpack");
    build_pack(tmp.path(), &wasm, &out).unwrap();

    let zip_bytes = std::fs::read(&out).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes)).unwrap();

    let names: Vec<String> = (0..zip.len())
        .map(|i| zip.by_index(i).unwrap().name().to_string())
        .collect();
    for expected in ["assets/views/hello/index.html", "assets/views/hello/app.js"] {
        assert!(
            names.iter().any(|n| n == expected),
            "{expected} must be in the pack, got {names:?}"
        );
    }

    let mut packed = Vec::new();
    zip.by_name("assets/views/hello/index.html")
        .unwrap()
        .read_to_end(&mut packed)
        .unwrap();
    assert_eq!(
        packed, html,
        "asset bytes must round-trip unmodified — the host verifies them \
         against the manifest sha256 before serving"
    );

    let mut manifest_bytes = Vec::new();
    zip.by_name("manifest.json")
        .unwrap()
        .read_to_end(&mut manifest_bytes)
        .unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();
    let entries = manifest["entries"].as_array().unwrap();
    for expected in ["assets/views/hello/index.html", "assets/views/hello/app.js"] {
        let row = entries
            .iter()
            .find(|e| e["path"] == expected)
            .unwrap_or_else(|| panic!("{expected} missing from manifest.json"));
        let sha = row["sha256"].as_str().unwrap();
        assert_eq!(sha.len(), 64, "sha256 must be 64 hex chars, got {sha:?}");
    }
}
```

- [ ] **Step 2: Run the test**

Run: `cargo test -p greentic-extension-sdk-cli --bin gtdx view_assets_round_trip`
Expected: PASS on the first run. This pins existing behaviour rather than
driving new behaviour, so a failure means the assumption the whole plan rests
on is wrong. If it fails, stop and read
`crates/greentic-extension-sdk-cli/src/dev/packer/mod.rs:301-320` — the
`for asset_dir in ["i18n", "schemas", "prompts", "assets"]` loop — before
changing anything else in this plan.

- [ ] **Step 3: Commit**

```bash
git add crates/greentic-extension-sdk-cli/src/dev/packer/tests.rs
git commit -m "test(cli): pin that assets/views round-trips into the pack and manifest"
```

---

## What Phase 1 deliberately leaves out

Named here so a reviewer does not read them as gaps:

- **No packer change.** `assets/` already round-trips into the pack and
  `manifest.json` already hashes it — Task 7 pins that so it stays true.
- **No WIT change.** The bridge terminates at the host, which invokes existing
  tool exports. A view does not add a guest interface.
- **No new `ExtensionKind`.** Deliberate, and argued in the spec: a kind would
  have to be added to five hand-maintained lists across four repositories.
- **No host rendering.** Phases 2–4 (`greentic-ext-runtime`, Admin, Designer)
  each get their own plan. After Phase 1 an author can declare, scaffold, lint,
  validate, pack and publish a view; nothing displays it yet.
- **No `fetchHosts` SSRF validation in lint.** The address rules live where
  `permissions.network` already enforces them; extending that to `ui.fetchHosts`
  belongs with the host proxy that will honour them, in Phase 3.

## Follow-on plans (not this document)

- Phase 2 — `greentic-ext-runtime`: surface `views[]` through the loader.
- Phase 3 — Admin: sync-time asset materialisation, asset route, bridge,
  `tenant_view_placements` / `team_view_overrides`, `/api/admin/views`, nav
  merge, `ExtensionViewHost`, tenant-admin placement UI. Store-server publish
  support folds in here.
- Phase 4 — Designer: asset route over the unpacked directory, bridge,
  `/api/views` reading configuration from Admin with cache and fallback, nav
  merge, `ExtensionViewHost`.
