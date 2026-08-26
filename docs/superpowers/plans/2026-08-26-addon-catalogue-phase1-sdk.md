# Addon Catalogue (Phase 1, SDK) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let an extension declare the addons it offers — Qdrant, Redis, Postgres — as a typed `contributions.addons` block, so the Designer can list them and render their configuration forms without any extension code running.

**Architecture:** A new contribution type, modelled exactly on the `views` contribution that landed this week. A typed `Addon` struct with a JSON Schema entry, deserialize-time invariants for what the type system cannot express, dedicated lint rules, and an authoring doc. Nothing executes; this is catalogue metadata.

**Tech Stack:** Rust 1.95.0, `serde`, `serde_json`, `jsonschema` (contract dev-dep), `anyhow`, `clap`.

**Spec:** `docs/superpowers/specs/2026-08-26-environment-addon-deployment-design.md` — this plan implements **Phase 1 of §9.3** only.

## Global Constraints

- Rust toolchain pinned to `1.95.0` (`rust-toolchain.toml`).
- No `unwrap()` or `panic!()` in non-test code. Tests may use `expect` with a message.
- Every SDK crate root carries `#![forbid(unsafe_code)]` — do not add `unsafe`.
- `ci/local_check.sh` is the gate: fmt, clippy `-D warnings`, `cargo test --workspace --all-features --locked`, release build, two `cargo publish --dry-run`. Clippy warnings are errors.
- **Run `cargo fmt --all` BEFORE committing, never after.**
- Conventional commits, one per task.
- **Do NOT add an `ExtensionKind` variant.** The enum stays at five. `AddonExtension` is Phase 2 and is gated on the `extension-base@0.3.0` contract release (spec §9.2).
- **Contribution fields are `snake_case` on the wire.** `NodeType` uses `type_id`, `config_schema`, `output_ports`; `View` uses `title_key`, `min_visibility`. Follow that — no `camelCase` renames except where a Rust keyword forces one.
- **D16 is binding: secrets never appear in `desired_state_schema`.** They arrive through the runtime binding instead. Task 4 enforces this with a lint rule rather than leaving it to review.

---

## The pattern this follows

The `views` contribution landed in `main` days ago and is the reference implementation. Read it when a step is ambiguous:

| Layer | Views | This plan |
|---|---|---|
| Typed struct | `contract/src/describe/contributions/view.rs` | `…/contributions/addon.rs` |
| Wiring | `contract/src/describe/contributions.rs` | same file |
| Deserialize invariants | `contract/src/describe/mod.rs:159-190` | same file |
| JSON Schema | `contract/schemas/describe-v2.json` | same file only — `describe-mcp-v1.json` types `contributions` as a bare object |
| Lint | `cli/src/commands/lint/rules_views.rs` | `…/lint/rules_addons.rs` |
| Tests | `contract/tests/{contributions_view,describe_views_invariants,schema_v2_views}.rs` | same three names, `addon`/`addons` |
| Doc | `docs/authoring-views.md` | `docs/authoring-addons.md` |

---

### Task 1: The `Addon` and `OutputSpec` types

**Files:**
- Create: `crates/greentic-extension-sdk-contract/src/describe/contributions/addon.rs`
- Modify: `crates/greentic-extension-sdk-contract/src/describe/contributions.rs`
- Test: `crates/greentic-extension-sdk-contract/tests/contributions_addon.rs` (create)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces:
  - `contributions::addon::OutputType` — enum `Text | Number | Boolean`, `snake_case` on the wire.
  - `contributions::addon::OutputSpec { name: String, output_type: OutputType, sensitive: bool, description: Option<String> }` — `output_type` serialises as `"type"`.
  - `contributions::addon::Addon { id, family, display_name, description, icon: Option<String>, config_schema: String, desired_state_schema: String, outputs: Vec<OutputSpec>, supports_backup: bool, schema_version: u32 }`
  - `Contributions.addons: Vec<Addon>` — the wire key is `addons`.
  - Re-exported from `contributions` as `pub use addon::{Addon, OutputSpec, OutputType};`

- [ ] **Step 1: Write the failing test**

Create `crates/greentic-extension-sdk-contract/tests/contributions_addon.rs`:

```rust
//! `Addon` is catalogue metadata: it tells the Designer an addon exists and
//! what its configuration form looks like. Nothing here executes.

use greentic_extension_sdk_contract::describe::contributions::{Addon, OutputType};

fn qdrant_json() -> serde_json::Value {
    serde_json::json!({
        "id": "qdrant",
        "family": "vector-db",
        "display_name": "Qdrant",
        "description": "Vector database for similarity search.",
        "config_schema": "{\"type\":\"object\",\"properties\":{\"replicas\":{\"type\":\"integer\"}}}",
        "desired_state_schema": "{\"type\":\"object\",\"properties\":{\"collections\":{\"type\":\"array\"}}}",
        "outputs": [
            { "name": "url", "type": "text" },
            { "name": "api_key", "type": "text", "sensitive": true, "description": "Bearer token." }
        ],
        "supports_backup": true,
        "schema_version": 1
    })
}

#[test]
fn a_full_addon_round_trips() {
    let addon: Addon = serde_json::from_value(qdrant_json()).expect("addon deserializes");

    assert_eq!(addon.id, "qdrant");
    assert_eq!(addon.family, "vector-db");
    assert_eq!(addon.outputs.len(), 2);
    assert_eq!(addon.outputs[1].name, "api_key");
    assert!(addon.outputs[1].sensitive, "api_key must be marked sensitive");
    assert!(!addon.outputs[0].sensitive, "sensitive defaults to false");
    assert!(addon.supports_backup);
    assert_eq!(addon.schema_version, 1);

    let back = serde_json::to_value(&addon).expect("addon serializes");
    let round: Addon = serde_json::from_value(back).expect("round-trips");
    assert_eq!(round, addon);
}

/// The wire key is `type`, not `output_type` — `output_type` only exists
/// because `type` is a Rust keyword.
#[test]
fn output_type_serialises_as_type() {
    let addon: Addon = serde_json::from_value(qdrant_json()).expect("deserializes");
    let v = serde_json::to_value(&addon).expect("serializes");
    let first = &v["outputs"][0];
    assert_eq!(first["type"], "text", "got: {first}");
    assert!(first.get("output_type").is_none(), "output_type must not reach the wire");
}

#[test]
fn optional_fields_may_be_omitted() {
    let minimal = serde_json::json!({
        "id": "redis",
        "family": "cache",
        "display_name": "Redis",
        "description": "In-memory key-value store.",
        "config_schema": "{\"type\":\"object\"}",
        "desired_state_schema": "{\"type\":\"object\"}"
    });
    let addon: Addon = serde_json::from_value(minimal).expect("minimal addon deserializes");

    assert!(addon.icon.is_none());
    assert!(addon.outputs.is_empty());
    assert!(!addon.supports_backup, "supports_backup defaults to false");
    assert_eq!(addon.schema_version, 1, "schema_version defaults to 1");
}

/// `deny_unknown_fields` catches a typo'd key at parse time rather than
/// silently dropping the value the author meant to set.
#[test]
fn an_unknown_field_is_rejected() {
    let mut v = qdrant_json();
    v["supports_backups"] = serde_json::json!(true); // note the trailing s
    let r: Result<Addon, _> = serde_json::from_value(v);
    assert!(r.is_err(), "an unknown field must be rejected");
}

#[test]
fn every_output_type_parses() {
    for (wire, expected) in [
        ("text", OutputType::Text),
        ("number", OutputType::Number),
        ("boolean", OutputType::Boolean),
    ] {
        let v = serde_json::json!({ "name": "x", "type": wire });
        let spec: greentic_extension_sdk_contract::describe::contributions::OutputSpec =
            serde_json::from_value(v).unwrap_or_else(|e| panic!("{wire} should parse: {e}"));
        assert_eq!(spec.output_type, expected);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-contract --test contributions_addon`
Expected: FAIL — `unresolved import ... Addon`.

- [ ] **Step 3: Write the types**

Create `crates/greentic-extension-sdk-contract/src/describe/contributions/addon.rs`:

```rust
//! `Addon` — a managed service an extension offers to an environment, e.g.
//! Qdrant or Redis.
//!
//! This is catalogue metadata only. It tells the Designer that an addon
//! exists, what its configuration form looks like, and what values it hands
//! back to the services that bind to it. Nothing here provisions anything:
//! the platform owns provisioning, and the addon declares only what it needs
//! (spec D6). That split is what lets one declaration serve both hosted and
//! bring-your-own-cloud placement.

use serde::{Deserialize, Serialize};

/// Scalar type of a value an addon hands back once it is running.
///
/// Deliberately three scalars and no object or array: an output is consumed
/// by string interpolation into another resource's configuration
/// (`${resources.qdrant.outputs.url}`), and a structured value has no
/// meaningful rendering there. An addon that wants to expose structure
/// exposes several outputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputType {
    Text,
    Number,
    Boolean,
}

/// One value an addon publishes once provisioned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputSpec {
    /// Referenced as `${resources.<resource_id>.outputs.<name>}`. Constrained
    /// by `gtdx lint` to characters that survive being turned into an
    /// environment variable, because that is what the platform does with it.
    pub name: String,

    /// `output_type` in Rust because `type` is a keyword; `type` on the wire.
    #[serde(rename = "type")]
    pub output_type: OutputType,

    /// A sensitive output never becomes a literal value. The platform
    /// resolves it to a secret reference — `valueFrom.secretKeyRef` on
    /// Kubernetes, a `sensitive` variable in generated IaC — so it never
    /// passes through a plan document, a plan UI, or a support bundle
    /// (spec §4.3). Getting this flag wrong is how a Redis password ends up
    /// in a log.
    #[serde(default)]
    pub sensitive: bool,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// One addon an extension offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Addon {
    /// Unique within the extension. The platform namespaces it as
    /// `<extension_id>/<id>`.
    pub id: String,

    /// What kind of thing this is — `vector-db`, `cache`, `sql`. A flow that
    /// needs a vector database asks for the family, not the vendor, so a
    /// deployment can substitute one implementation for another.
    ///
    /// An open string rather than a closed enum, for the same reason `View`
    /// keeps `slot` open: `describe.json` is signed and immutable once
    /// published, so a closed enum in it rots the way this project's
    /// hard-coded kind lists did. `gtdx lint` warns on an unfamiliar family
    /// instead.
    pub family: String,

    pub display_name: String,
    pub description: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,

    /// JSON Schema (Draft 2020-12) for the knobs a user sets per environment
    /// — size, replicas, version. The Designer renders this as a form.
    /// Stringly-encoded for the same reason `NodeType.config_schema` is:
    /// it is a payload passed through to a renderer, not host control data.
    pub config_schema: String,

    /// JSON Schema for the day-2 state the addon reconciles — Qdrant
    /// collections, Redis ACL users.
    ///
    /// **Secrets do not belong here** (spec D16). A password inside desired
    /// state can never be read back by `observe`, so it diffs forever and no
    /// plan is ever clean. Credentials reach the addon through its runtime
    /// binding instead. `gtdx lint` reports a secret-looking property here as
    /// an error.
    pub desired_state_schema: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<OutputSpec>,

    /// Whether the addon can snapshot before a destructive change. The
    /// platform offers to back up on the strength of this flag, so declare
    /// `true` only when a snapshot genuinely happens.
    #[serde(default)]
    pub supports_backup: bool,

    /// Version of THIS addon's `desired_state_schema`, not of the addon
    /// itself. It lets one extension migrate instances from a v1 shape to a
    /// v2 shape rather than breaking them (spec D17). Defaults to 1 so
    /// existing declarations stay valid.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
}

const fn default_schema_version() -> u32 {
    1
}
```

- [ ] **Step 4: Wire it into `Contributions`**

In `crates/greentic-extension-sdk-contract/src/describe/contributions.rs`:

Add `pub mod addon;` to the module list (alphabetically first, before `connection_test`).

Add `pub use addon::{Addon, OutputSpec, OutputType};` to the re-exports (alphabetically first).

Add the field to `struct Contributions`, after `views`:

```rust
    /// Managed services this extension offers to an environment — Qdrant,
    /// Redis, Postgres. Catalogue metadata only; the platform provisions.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub addons: Vec<Addon>,
```

Update the module doc's opening line — it currently says "Nine children"; it becomes ten.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p greentic-extension-sdk-contract --test contributions_addon`
Expected: PASS, all five tests.

Then run the whole crate — `Contributions` gained a field and derives `PartialEq`, so round-trip tests elsewhere exercise it:

Run: `cargo test -p greentic-extension-sdk-contract`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all
git add crates/greentic-extension-sdk-contract/src/describe/contributions/addon.rs \
        crates/greentic-extension-sdk-contract/src/describe/contributions.rs \
        crates/greentic-extension-sdk-contract/tests/contributions_addon.rs
git commit -m "feat(contract): add contributions.addons catalogue entries

An extension can now declare the managed services it offers, with the
config and desired-state schemas the Designer renders as forms. Catalogue
metadata only — the platform owns provisioning."
```

---

### Task 2: Deserialize-time invariants

Two things the type system cannot express, and one that would otherwise surface as a broken form in the Designer rather than a parse error here.

**Files:**
- Modify: `crates/greentic-extension-sdk-contract/src/describe/mod.rs` (the custom `Deserialize` for `DescribeJson`, alongside the existing view checks around lines 159-190)
- Test: `crates/greentic-extension-sdk-contract/tests/describe_addons_invariants.rs` (create)

**Interfaces:**
- Consumes: `Addon`, `OutputSpec` from Task 1.
- Produces: no new public API. Three rejection paths, each with a message naming the offending id.

- [ ] **Step 1: Write the failing test**

Create `crates/greentic-extension-sdk-contract/tests/describe_addons_invariants.rs`:

```rust
//! Invariants the `Addon` type cannot state, enforced at deserialize time so
//! a bad descriptor fails on load rather than as a broken form in the
//! Designer.

use greentic_extension_sdk_contract::describe::DescribeJson;

fn describe_with(addons: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "$schema": "https://store.greentic.cloud/schemas/describe-v2.json",
        "apiVersion": "greentic.ai/v2",
        "kind": "DesignExtension",
        "compat": { "min_designer": "1.2.0", "min_runner": "1.2.0", "contract": "1.2.9" },
        "metadata": {
            "id": "greentic.addon-test",
            "version": "0.1.0",
            "display_name": "Addon Test",
            "description": "Fixture.",
            "author": { "name": "Greentic" }
        },
        "capabilities": { "offered": [], "required": [] },
        "runtime": {
            "memoryLimitMB": 64,
            "permissions": {},
            "components": {
                "main": {
                    "sha256": "0000000000000000000000000000000000000000000000000000000000000001",
                    "world": "greentic:extension-design/design-extension",
                    "gtpack": {
                        "file": "extension.wasm",
                        "sha256": "0000000000000000000000000000000000000000000000000000000000000001",
                        "pack_id": "main"
                    }
                }
            }
        },
        "contributions": { "addons": addons }
    })
}

fn addon(id: &str, outputs: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "family": "vector-db",
        "display_name": "Test",
        "description": "Fixture addon.",
        "config_schema": "{\"type\":\"object\"}",
        "desired_state_schema": "{\"type\":\"object\"}",
        "outputs": outputs
    })
}

fn parse(v: serde_json::Value) -> Result<DescribeJson, serde_json::Error> {
    serde_json::from_value(v)
}

#[test]
fn valid_addons_accepted() {
    let d = describe_with(serde_json::json!([
        addon("qdrant", serde_json::json!([{ "name": "url", "type": "text" }])),
        addon("redis", serde_json::json!([{ "name": "url", "type": "text" }])),
    ]));
    assert!(parse(d).is_ok(), "two distinct addons must parse");
}

/// Ids namespace to `<extension_id>/<addon_id>` on the platform, so a
/// duplicate would make one of the two unaddressable.
#[test]
fn duplicate_addon_id_rejected() {
    let d = describe_with(serde_json::json!([
        addon("qdrant", serde_json::json!([])),
        addon("qdrant", serde_json::json!([])),
    ]));
    let err = parse(d).expect_err("duplicate id must be rejected");
    assert!(err.to_string().contains("qdrant"), "error should name the id: {err}");
}

/// Outputs are addressed by name; two with the same name means a binding
/// resolves to whichever the platform happened to see last.
#[test]
fn duplicate_output_name_rejected() {
    let d = describe_with(serde_json::json!([addon(
        "qdrant",
        serde_json::json!([
            { "name": "url", "type": "text" },
            { "name": "url", "type": "text" }
        ])
    )]));
    let err = parse(d).expect_err("duplicate output name must be rejected");
    assert!(err.to_string().contains("url"), "error should name the output: {err}");
}

/// The same name in two different addons is fine — they are separate
/// namespaces.
#[test]
fn the_same_output_name_in_two_addons_is_fine() {
    let d = describe_with(serde_json::json!([
        addon("qdrant", serde_json::json!([{ "name": "url", "type": "text" }])),
        addon("redis", serde_json::json!([{ "name": "url", "type": "text" }])),
    ]));
    assert!(parse(d).is_ok(), "output names are scoped per addon");
}

/// A schema string that is not JSON renders as an empty form with no error,
/// which is the worst way to discover it.
#[test]
fn a_config_schema_that_is_not_json_is_rejected() {
    let mut a = addon("qdrant", serde_json::json!([]));
    a["config_schema"] = serde_json::json!("not json at all");
    let d = describe_with(serde_json::json!([a]));
    let err = parse(d).expect_err("unparseable config_schema must be rejected");
    assert!(err.to_string().contains("config_schema"), "error should name the field: {err}");
}

#[test]
fn a_desired_state_schema_that_is_not_json_is_rejected() {
    let mut a = addon("qdrant", serde_json::json!([]));
    a["desired_state_schema"] = serde_json::json!("{ unclosed");
    let d = describe_with(serde_json::json!([a]));
    let err = parse(d).expect_err("unparseable desired_state_schema must be rejected");
    assert!(
        err.to_string().contains("desired_state_schema"),
        "error should name the field: {err}"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-contract --test describe_addons_invariants`
Expected: the four rejection tests FAIL (they parse successfully today); the two acceptance tests PASS.

If `valid_addons_accepted` also fails, the fixture is wrong rather than the code. Fix the fixture against the shape used in `tests/describe_views_invariants.rs`, which is known-good, before touching `describe/mod.rs`.

- [ ] **Step 3: Add the checks**

In `crates/greentic-extension-sdk-contract/src/describe/mod.rs`, in the custom `Deserialize` for `DescribeJson`, immediately after the existing view checks and before the final `Ok(DescribeJson { … })`:

```rust
        // Addon ids namespace to `<extension_id>/<addon_id>` on the platform,
        // so a duplicate makes one of the two unaddressable.
        let mut seen_addons = std::collections::BTreeSet::new();
        for addon in &raw.contributions.addons {
            if !seen_addons.insert(addon.id.as_str()) {
                return Err(format!(
                    "contributions.addons[] declares duplicate id {:?}",
                    addon.id
                ));
            }

            // Outputs are addressed by name from other resources' bindings. A
            // duplicate means `${resources.x.outputs.url}` resolves to
            // whichever entry the platform saw last.
            let mut seen_outputs = std::collections::BTreeSet::new();
            for out in &addon.outputs {
                if !seen_outputs.insert(out.name.as_str()) {
                    return Err(format!(
                        "addon {:?} declares duplicate output name {:?}",
                        addon.id, out.name
                    ));
                }
            }

            // A schema that is not JSON renders as an empty form with no
            // error — the worst place to discover the typo.
            for (field, text) in [
                ("config_schema", &addon.config_schema),
                ("desired_state_schema", &addon.desired_state_schema),
            ] {
                if serde_json::from_str::<serde_json::Value>(text).is_err() {
                    return Err(format!(
                        "addon {:?} has a {field} that is not valid JSON",
                        addon.id
                    ));
                }
            }
        }
```

Note the error-return style: the surrounding function returns `Result<_, String>` and the caller maps it into a serde error. Match whatever the adjacent view checks do — copy their `return Err(format!(...))` shape exactly.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p greentic-extension-sdk-contract`
Expected: PASS. Run the whole crate, not just the new file — the deserializer is shared.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all
git add crates/greentic-extension-sdk-contract/src/describe/mod.rs \
        crates/greentic-extension-sdk-contract/tests/describe_addons_invariants.rs
git commit -m "feat(contract): reject duplicate addon ids, duplicate outputs, unparseable schemas

Three things the Addon type cannot state. The schema check matters most: a
config_schema that is not JSON renders as an empty form with no error, which
is the worst way to find a typo."
```

---

### Task 3: JSON Schema entries

**Files:**
- Modify: `crates/greentic-extension-sdk-contract/schemas/describe-v2.json` (`$defs.contributions.properties`)
- Test: `crates/greentic-extension-sdk-contract/tests/schema_v2_addons.rs` (create)

**Interfaces:**
- Consumes: the wire shape from Task 1.
- Produces: no Rust API. `contributions.addons` validates.

- [ ] **Step 1: Write the failing test**

Create `crates/greentic-extension-sdk-contract/tests/schema_v2_addons.rs`:

```rust
//! The schema and the Rust struct must agree. The struct is
//! `deny_unknown_fields`, so a schema that is more permissive lets a
//! descriptor pass validation and then fail on load, blaming the wrong layer.

use greentic_extension_sdk_contract::schema::validate_describe_json;

fn validate(describe_json: &str) -> Result<(), greentic_extension_sdk_contract::ContractError> {
    let value: serde_json::Value =
        serde_json::from_str(describe_json).expect("fixture is valid JSON");
    validate_describe_json(&value)
}

/// Built from the known-good fixture in `schema_v2_validate.rs`; only
/// `contributions` varies.
fn describe_with_addons(addons: &str) -> String {
    format!(
        r#"{{
          "$schema": "https://store.greentic.cloud/schemas/describe-v2.json",
          "apiVersion": "greentic.ai/v2",
          "kind": "DesignExtension",
          "compat": {{ "min_designer": "1.2.0", "min_runner": "1.2.0", "contract": "1.2.9" }},
          "metadata": {{
            "id": "greentic.addon-test",
            "version": "0.1.0",
            "display_name": "Addon Test",
            "description": "Fixture.",
            "author": {{ "name": "Greentic" }}
          }},
          "capabilities": {{ "offered": [], "required": [] }},
          "runtime": {{
            "memoryLimitMB": 64,
            "permissions": {{}},
            "components": {{
              "main": {{
                "sha256": "0000000000000000000000000000000000000000000000000000000000000001",
                "world": "greentic:extension-design/design-extension",
                "gtpack": {{
                  "file": "extension.wasm",
                  "sha256": "0000000000000000000000000000000000000000000000000000000000000001",
                  "pack_id": "main"
                }}
              }}
            }}
          }},
          "contributions": {{ "addons": {addons} }}
        }}"#
    )
}

#[test]
fn a_full_addon_validates() {
    // Historical note (not a correction of this plan): `icon` is shown here
    // as a path, `icons/qdrant.svg`. What shipped instead is a
    // host-resolved icon name, matching `View.icon` - the host looks the
    // name up in its own icon set, and no directory is reserved for an
    // addon icon the way `assets/views/<id>/` is for a view. See
    // `docs/authoring-addons.md`'s `icon` field entry.
    let d = describe_with_addons(
        r#"[{
            "id": "qdrant",
            "family": "vector-db",
            "display_name": "Qdrant",
            "description": "Vector database.",
            "icon": "icons/qdrant.svg",
            "config_schema": "{\"type\":\"object\"}",
            "desired_state_schema": "{\"type\":\"object\"}",
            "outputs": [{ "name": "url", "type": "text", "sensitive": false }],
            "supports_backup": true,
            "schema_version": 2
        }]"#,
    );
    let r = validate(&d);
    assert!(r.is_ok(), "a full addon must validate: {r:?}");
}

#[test]
fn a_minimal_addon_validates() {
    let d = describe_with_addons(
        r#"[{
            "id": "redis",
            "family": "cache",
            "display_name": "Redis",
            "description": "Key-value store.",
            "config_schema": "{\"type\":\"object\"}",
            "desired_state_schema": "{\"type\":\"object\"}"
        }]"#,
    );
    let r = validate(&d);
    assert!(r.is_ok(), "optional fields may be omitted: {r:?}");
}

#[test]
fn an_addon_missing_a_required_field_is_rejected() {
    // No `family`.
    let d = describe_with_addons(
        r#"[{
            "id": "redis",
            "display_name": "Redis",
            "description": "Key-value store.",
            "config_schema": "{\"type\":\"object\"}",
            "desired_state_schema": "{\"type\":\"object\"}"
        }]"#,
    );
    assert!(validate(&d).is_err(), "family is required");
}

/// The struct is `deny_unknown_fields`; the schema must be too, or a typo
/// passes validation and fails on load.
#[test]
fn an_unknown_addon_field_is_rejected() {
    let d = describe_with_addons(
        r#"[{
            "id": "redis",
            "family": "cache",
            "display_name": "Redis",
            "description": "Key-value store.",
            "config_schema": "{\"type\":\"object\"}",
            "desired_state_schema": "{\"type\":\"object\"}",
            "supports_backups": true
        }]"#,
    );
    assert!(validate(&d).is_err(), "an unknown addon field must be rejected");
}

#[test]
fn an_unknown_output_type_is_rejected() {
    let d = describe_with_addons(
        r#"[{
            "id": "redis",
            "family": "cache",
            "display_name": "Redis",
            "description": "Key-value store.",
            "config_schema": "{\"type\":\"object\"}",
            "desired_state_schema": "{\"type\":\"object\"}",
            "outputs": [{ "name": "url", "type": "object" }]
        }]"#,
    );
    assert!(validate(&d).is_err(), "output type must be text|number|boolean");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-contract --test schema_v2_addons`
Expected: FAIL — `contributions` has `additionalProperties: false` or no `addons` key, so even the valid cases are rejected. Check which: if the valid cases pass and only the rejection cases fail, `contributions` is permissive and you are adding constraint rather than permission.

- [ ] **Step 3: Add the schema entry**

In `crates/greentic-extension-sdk-contract/schemas/describe-v2.json`, inside `$defs.contributions.properties` (alongside `views`), add:

```json
"addons": {
  "description": "Managed services this extension offers to an environment - Qdrant, Redis, Postgres. Catalogue metadata only: the platform provisions the workload, and the addon declares what it needs. Secrets never appear in `desired_state_schema`; they reach the addon through its runtime binding, because a secret in desired state can never be read back and so diffs forever.",
  "type": "array",
  "items": {
    "type": "object",
    "additionalProperties": false,
    "required": ["id", "family", "display_name", "description", "config_schema", "desired_state_schema"],
    "properties": {
      "id": {
        "description": "Unique within the extension. The platform namespaces it as `<extension_id>/<id>`.",
        "type": "string"
      },
      "family": {
        "description": "What kind of thing this is - `vector-db`, `cache`, `sql`. A flow asks for the family, not the vendor, so a deployment can substitute one implementation for another. An open string on purpose: describe.json is signed and immutable once published, so a closed enum in it rots. `gtdx lint` warns on an unfamiliar family instead.",
        "type": "string"
      },
      "display_name": { "type": "string" },
      "description": { "type": "string" },
      "icon": { "type": "string" },
      "config_schema": {
        "description": "JSON Schema (Draft 2020-12) for the knobs a user sets per environment - size, replicas, version. Rendered as a form by the Designer. Stringly-encoded because it is a payload passed to a renderer, not host control data. Must parse as JSON; the Rust deserializer rejects it otherwise.",
        "type": "string"
      },
      "desired_state_schema": {
        "description": "JSON Schema for the day-2 state the addon reconciles - Qdrant collections, Redis ACL users. Secrets do not belong here: a password in desired state can never be read back by `observe`, so it diffs forever and no plan is ever clean. `gtdx lint` reports a secret-looking property as an error.",
        "type": "string"
      },
      "outputs": {
        "description": "Values the addon publishes once provisioned, referenced from another resource as `${resources.<id>.outputs.<name>}`.",
        "type": "array",
        "items": {
          "type": "object",
          "additionalProperties": false,
          "required": ["name", "type"],
          "properties": {
            "name": {
              "description": "Referenced as `${resources.<id>.outputs.<name>}`. `gtdx lint` constrains this to characters that survive becoming an environment variable, because that is what the platform does with it.",
              "type": "string"
            },
            "type": {
              "description": "Scalar only. An output is interpolated into another resource's configuration, where a structured value has no meaningful rendering; an addon wanting structure exposes several outputs.",
              "enum": ["text", "number", "boolean"]
            },
            "sensitive": {
              "description": "A sensitive output never becomes a literal value - the platform resolves it to a secret reference, so it never passes through a plan document, a plan UI, or a support bundle. Getting this wrong is how a password ends up in a log.",
              "type": "boolean"
            },
            "description": { "type": "string" }
          }
        }
      },
      "supports_backup": {
        "description": "Whether the addon can snapshot before a destructive change. The platform offers to back up on the strength of this flag, so declare true only when a snapshot genuinely happens.",
        "type": "boolean"
      },
      "schema_version": {
        "description": "Version of this addon's `desired_state_schema`, not of the addon. It lets one extension migrate instances from a v1 shape to a v2 shape rather than breaking them. Defaults to 1.",
        "type": "integer",
        "minimum": 1
      }
    }
  }
}
```

**Do not touch `describe-mcp-v1.json`.** Its `contributions` is declared as a bare `{"type": "object"}` with no typed children, so there is nothing for an `addons` entry to sit beside; adding one would be the only typed child in an otherwise untyped block. Verified before this plan was written.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p greentic-extension-sdk-contract`
Expected: PASS.

Then the template conformance suite, since every `templates/*/describe.json.tmpl` validates against this schema:

Run: `cargo test -p greentic-extension-sdk-cli --test templates_schema_conformance`
Expected: PASS. No template declares addons yet, so this should be unaffected; if it fails, report rather than adjusting a template.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all
git add crates/greentic-extension-sdk-contract/schemas/describe-v2.json \
        crates/greentic-extension-sdk-contract/tests/schema_v2_addons.rs
git commit -m "feat(contract): validate contributions.addons in describe-v2 schema

additionalProperties:false on both the addon and its outputs, matching the
struct's deny_unknown_fields. A permissive schema would let a typo pass
validation and fail on load, blaming the wrong layer."
```

---

### Task 4: Lint rules

Four rules. The secret rule is the one that matters: it enforces spec D16 mechanically rather than leaving it to review.

**Files:**
- Create: `crates/greentic-extension-sdk-cli/src/commands/lint/rules_addons.rs`
- Modify: `crates/greentic-extension-sdk-cli/src/commands/lint/mod.rs` (module decl, import, `collect_violations`)
- Test: `crates/greentic-extension-sdk-cli/src/commands/lint/tests.rs` (append)

**Interfaces:**
- Consumes: `Violation` and the `collect_violations` shape from `lint/mod.rs`; mirror `rules_views.rs::check_views` exactly.
- Produces: `pub(super) fn check_addons(describe: &serde_json::Value) -> Vec<Violation>`. Note it takes no `dir` — unlike views, addons ship no assets.

Codes:

| Code | Severity | Means |
|---|---|---|
| `E_ADDON_ID_PATTERN` | Error | id is not `^[a-z0-9][a-z0-9-]*$` |
| `E_ADDON_OUTPUT_NAME` | Error | output name is not `^[A-Za-z_][A-Za-z0-9_]*$` |
| `E_ADDON_SECRET_IN_DESIRED_STATE` | Error | a top-level property of `desired_state_schema` looks like a credential |
| `W_ADDON_FAMILY_UNKNOWN` | Warning | family is not one this SDK version knows |

- [ ] **Step 1: Write the failing test**

Append to `crates/greentic-extension-sdk-cli/src/commands/lint/tests.rs`:

```rust
// --- contributions.addons ---

use rules_addons::check_addons;

fn describe_with_addon(addon: serde_json::Value) -> serde_json::Value {
    json!({ "contributions": { "addons": [addon] } })
}

fn base_addon() -> serde_json::Value {
    json!({
        "id": "qdrant",
        "family": "vector-db",
        "display_name": "Qdrant",
        "description": "Vector database.",
        "config_schema": "{\"type\":\"object\"}",
        "desired_state_schema": "{\"type\":\"object\",\"properties\":{\"collections\":{\"type\":\"array\"}}}",
        "outputs": [{ "name": "QDRANT_URL", "type": "text" }]
    })
}

#[test]
fn a_well_formed_addon_produces_no_violations() {
    let v = check_addons(&describe_with_addon(base_addon()));
    assert!(v.is_empty(), "expected no violations, got: {v:?}");
}

#[test]
fn an_id_with_uppercase_or_underscores_is_an_error() {
    for bad in ["Qdrant", "qdrant_db", "-qdrant", ""] {
        let mut a = base_addon();
        a["id"] = json!(bad);
        let v = check_addons(&describe_with_addon(a));
        assert!(
            v.iter().any(|x| x.code == "E_ADDON_ID_PATTERN"),
            "id {bad:?} should be rejected, got: {v:?}"
        );
    }
}

/// Output names become environment variables on the consuming service, so a
/// name that is not a valid identifier breaks at injection time.
#[test]
fn an_output_name_that_is_not_env_var_safe_is_an_error() {
    for bad in ["qdrant-url", "1url", "url!", ""] {
        let mut a = base_addon();
        a["outputs"] = json!([{ "name": bad, "type": "text" }]);
        let v = check_addons(&describe_with_addon(a));
        assert!(
            v.iter().any(|x| x.code == "E_ADDON_OUTPUT_NAME"),
            "output name {bad:?} should be rejected, got: {v:?}"
        );
    }
}

/// Spec D16. A credential in desired state can never be read back by
/// `observe`, so it diffs forever and no plan is ever clean. Catching it here
/// is cheaper than discovering it when the first reconcile never converges.
#[test]
fn a_secret_looking_property_in_desired_state_is_an_error() {
    for bad in ["password", "apiKey", "api_key", "auth_token", "clientSecret", "credentials"] {
        let mut a = base_addon();
        a["desired_state_schema"] =
            json!(format!(r#"{{"type":"object","properties":{{"{bad}":{{"type":"string"}}}}}}"#));
        let v = check_addons(&describe_with_addon(a));
        assert!(
            v.iter().any(|x| x.code == "E_ADDON_SECRET_IN_DESIRED_STATE"),
            "desired-state property {bad:?} should be rejected, got: {v:?}"
        );
    }
}

/// The same word in `config_schema` is fine — config is not reconciled
/// against observed state, so it does not diff forever.
#[test]
fn a_secret_looking_property_in_config_schema_is_not_flagged() {
    let mut a = base_addon();
    a["config_schema"] =
        json!(r#"{"type":"object","properties":{"password":{"type":"string"}}}"#);
    let v = check_addons(&describe_with_addon(a));
    assert!(
        !v.iter().any(|x| x.code == "E_ADDON_SECRET_IN_DESIRED_STATE"),
        "config_schema must not be flagged, got: {v:?}"
    );
}

#[test]
fn an_unfamiliar_family_is_a_warning_not_an_error() {
    let mut a = base_addon();
    a["family"] = json!("quantum-db");
    let v = check_addons(&describe_with_addon(a));
    let hit = v
        .iter()
        .find(|x| x.code == "W_ADDON_FAMILY_UNKNOWN")
        .unwrap_or_else(|| panic!("expected W_ADDON_FAMILY_UNKNOWN, got: {v:?}"));
    assert!(
        matches!(hit.severity, Severity::Warning),
        "an unfamiliar family must warn, not fail the run: {hit:?}"
    );
}

#[test]
fn a_describe_with_no_addons_produces_no_violations() {
    let v = check_addons(&json!({ "contributions": {} }));
    assert!(v.is_empty(), "expected no violations, got: {v:?}");
}
```

`tests.rs` opens with `use super::*;`, so `Severity` is already in scope — do not add an import for it. Existing tests at `tests.rs:260` and `:494` already assert on `v[0].severity` the same way.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-cli lint::tests`
Expected: FAIL to compile — `unresolved import rules_addons`.

- [ ] **Step 3: Write the rules**

Create `crates/greentic-extension-sdk-cli/src/commands/lint/rules_addons.rs`:

```rust
//! `contributions.addons` lint rules.
//!
//! The secret rule is the one that earns its keep. Spec D16 says credentials
//! never appear in `desired_state_schema`, because a value `observe` cannot
//! read back diffs forever and no plan is ever clean. That is a design
//! decision a reviewer would have to remember; here it is a rule.

use super::Violation;

/// Families this SDK version knows. Unknown ones warn rather than fail: the
/// list lives in a released binary while `describe.json` is signed and
/// immutable, so a hard error here would reject an addon that a newer
/// platform understands perfectly well.
const KNOWN_FAMILIES: [&str; 6] = [
    "vector-db",
    "cache",
    "sql",
    "queue",
    "object-store",
    "search",
];

/// Property names that name a credential. Matched case-insensitively against
/// the name with `-` and `_` stripped, so `api_key`, `apiKey` and `api-key`
/// all hit the same entry.
const SECRET_MARKERS: [&str; 6] = ["password", "secret", "token", "apikey", "credential", "passwd"];

fn is_valid_addon_id(id: &str) -> bool {
    !id.is_empty()
        && id.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Output names become environment variables on the consuming service, so
/// they must survive that translation unchanged.
fn is_env_var_safe(name: &str) -> bool {
    !name.is_empty()
        && name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn looks_like_a_secret(property: &str) -> bool {
    let flat: String = property
        .chars()
        .filter(|c| *c != '-' && *c != '_')
        .flat_map(char::to_lowercase)
        .collect();
    SECRET_MARKERS.iter().any(|m| flat.contains(m))
}

pub(super) fn check_addons(describe: &serde_json::Value) -> Vec<Violation> {
    let mut out = Vec::new();
    let Some(addons) = describe
        .get("contributions")
        .and_then(|c| c.get("addons"))
        .and_then(|a| a.as_array())
    else {
        return out;
    };

    for addon in addons {
        let id = addon.get("id").and_then(|v| v.as_str()).unwrap_or_default();

        if !is_valid_addon_id(id) {
            out.push(Violation::error(
                "E_ADDON_ID_PATTERN",
                format!(
                    "addon id {id:?} must match ^[a-z0-9][a-z0-9-]*$ - it becomes part of \
                     `<extension_id>/<id>` on the platform"
                ),
            ));
        }

        let family = addon.get("family").and_then(|v| v.as_str()).unwrap_or_default();
        if !family.is_empty() && !KNOWN_FAMILIES.contains(&family) {
            out.push(Violation::warning(
                "W_ADDON_FAMILY_UNKNOWN",
                format!(
                    "addon {id:?} declares family {family:?}, which this SDK does not know \
                     (known: {}). A flow asking for a family will not match it unless the \
                     platform knows it too.",
                    KNOWN_FAMILIES.join(", ")
                ),
            ));
        }

        if let Some(outputs) = addon.get("outputs").and_then(|v| v.as_array()) {
            for out_spec in outputs {
                let name = out_spec.get("name").and_then(|v| v.as_str()).unwrap_or_default();
                if !is_env_var_safe(name) {
                    out.push(Violation::error(
                        "E_ADDON_OUTPUT_NAME",
                        format!(
                            "addon {id:?} output {name:?} must match ^[A-Za-z_][A-Za-z0-9_]*$ - \
                             outputs are injected as environment variables"
                        ),
                    ));
                }
            }
        }

        // D16: credentials reach the addon through its binding, never through
        // desired state. `config_schema` is deliberately not checked - config
        // is not reconciled against observed state, so it does not diff.
        let desired = addon
            .get("desired_state_schema")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(desired) {
            if let Some(props) = parsed.get("properties").and_then(|p| p.as_object()) {
                for property in props.keys() {
                    if looks_like_a_secret(property) {
                        out.push(Violation::error(
                            "E_ADDON_SECRET_IN_DESIRED_STATE",
                            format!(
                                "addon {id:?} declares {property:?} in desired_state_schema. \
                                 A credential there can never be read back by `observe`, so it \
                                 diffs forever and no plan is ever clean. Credentials reach the \
                                 addon through its runtime binding instead."
                            ),
                        ));
                    }
                }
            }
        }
    }

    out
}
```

`Violation::error(code, message)` and `Violation::warning(code, message)` are the constructors defined in `lint/mod.rs:95-110`; `Severity` lives there too, not in the contract crate. `rules_views.rs` imports only `use super::Violation;` — do the same.

- [ ] **Step 4: Wire it into `collect_violations`**

In `crates/greentic-extension-sdk-cli/src/commands/lint/mod.rs`:

- add `mod rules_addons;` beside `mod rules_views;`
- add `use rules_addons::check_addons;` beside the `check_views` import
- add `out.extend(check_addons(describe));` in `collect_violations`, after the `check_views` line

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p greentic-extension-sdk-cli lint`
Expected: PASS — the whole `lint` module, not just the new tests.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all
git add crates/greentic-extension-sdk-cli/src/commands/lint/rules_addons.rs \
        crates/greentic-extension-sdk-cli/src/commands/lint/mod.rs \
        crates/greentic-extension-sdk-cli/src/commands/lint/tests.rs
git commit -m "feat(lint): four contributions.addons rules

E_ADDON_SECRET_IN_DESIRED_STATE is the one that earns its keep: spec D16 says
credentials never appear in desired state, because a value observe cannot read
back diffs forever. That was a decision a reviewer had to remember; now it is
a rule."
```

---

### Task 5: Authoring doc and full gate

**Files:**
- Create: `docs/authoring-addons.md`
- Modify: `docs/superpowers/specs/2026-08-26-environment-addon-deployment-design.md` (§9.3 phase 1 status)

- [ ] **Step 1: Write the doc**

Create `docs/authoring-addons.md`, modelled on `docs/authoring-views.md`. It must contain, with a worked Qdrant example:

- What an addon is and what it is not — catalogue metadata; the platform provisions, the addon declares (spec D6)
- Every field of `contributions.addons[]`, with the `outputs` `sensitive` flag explained in terms of what it prevents
- **Why secrets do not go in `desired_state_schema`**, with the `observe` round-trip reason, and the `E_ADDON_SECRET_IN_DESIRED_STATE` code
- The four lint codes and how to fix each
- `schema_version` and what it is for
- An explicit note that Phase 1 ships the catalogue only: nothing reconciles yet, and `AddonExtension` is a later contract release

- [ ] **Step 2: Run the full gate**

Run: `./ci/local_check.sh`
Expected: all six steps pass. If a `cargo publish --dry-run` step fails for a network or registry reason rather than a code reason, say so explicitly — do not report a network error as a passing gate or as a code defect.

- [ ] **Step 3: Mark the phase in the spec**

In `docs/superpowers/specs/2026-08-26-environment-addon-deployment-design.md` §9.3, change the phase 1 row's "Gated on" cell from `**Nothing.** Startable now.` to:

```
**Done in the SDK** (catalogue). Reconcilers remain platform-side.
```

- [ ] **Step 4: Commit**

```bash
git add docs/authoring-addons.md \
        docs/superpowers/specs/2026-08-26-environment-addon-deployment-design.md
git commit -m "docs(addons): authoring guide, and mark phase 1's SDK half done

Says plainly what this phase does not do: nothing reconciles yet. An author
reading only the schema would reasonably assume declaring an addon deploys
one."
```

---

## Out of scope, and why

**Reconcilers (`observe` / `plan` / `apply`).** Spec §5.1 defines them; they are implemented platform-side in phase 1 (spec D14) and exposed over WIT in phase 2. Nothing in this SDK executes them.

**Renderers, the environment model, bindings resolution.** Spec §4, §6, §7 — commercial platform repo.

**`ExtensionKind::Addon` and the `extension-addon` WIT world.** Phase 2, gated on the `extension-base@0.3.0` contract release (spec §9.2). Adding a WIT enum variant is breaking and forces the runtime to serve two `manifest` versions concurrently across repos.

**Node types bound to an addon (spec D15).** D15 governs what an `AddonExtension` may contribute, and that kind does not exist until phase 2. Phase 1's addons are declared by existing kinds, which already contribute node types by the normal route.

**The conformance suite (spec §8.1).** It asserts properties of `plan` and `apply`, which have no implementation here yet.
