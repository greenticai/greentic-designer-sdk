# Kind Registry Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every kind-dependent code path in the SDK derive from `ExtensionKind::ALL` instead of a hand-written list, and turn two silent fallbacks into hard errors — fixing five live bugs today and making a sixth kind cheap to add later.

**Architecture:** Add two methods to the contract crate (`wire_name`, `from_wire`) so every consumer has a single derivation point for kind↔string mapping. Then convert each hand-written list to derive from `ExtensionKind::ALL`. Finally, replace the two `_ =>` fallbacks that swallow unknown inputs with errors, and add a guard test that fails when the JSON Schema's `kind` enum drifts from the Rust enum.

**Tech Stack:** Rust 1.95.0, `cargo component`, `serde`/`serde_json`, `anyhow`, `clap` (`ValueEnum`), `jsonschema` (already a contract dev-dep for schema tests).

**Spec:** `docs/superpowers/specs/2026-08-26-environment-addon-deployment-design.md` — this plan implements §9.1 only. §5 (the addon contract), §6 (renderers) and the `ExtensionKind::Addon` variant are **out of scope** and belong to a second plan, gated on the §9.2 release gates.

## Global Constraints

- Rust toolchain pinned to `1.95.0` (`rust-toolchain.toml`).
- No `unwrap()` or `panic!()` in non-test code. Tests may use `expect` with a message.
- Every SDK crate root carries `#![forbid(unsafe_code)]` — do not add `unsafe`.
- `ci/local_check.sh` is the gate: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`, `cargo test --workspace --all-features --locked`, `cargo build --workspace --locked --release`, then two `cargo publish --dry-run` steps. Clippy warnings are errors.
- Conventional commits (`feat:`, `fix:`, `refactor:`, `test:`, `docs:`).
- `ExtensionKind::ALL` is the single source of truth for "every kind". Never re-list variants by hand.
- **Do not add `ExtensionKind::Addon` in this plan.** Every task here must leave the enum at five variants. The whole point is that the sixth becomes cheap; adding it now would couple this work to a blocked contract release.

---

## Why this order

Task 1 creates the derivation point. Tasks 2–3 add guards that will *fail loudly* when a sixth kind lands. Tasks 4–7 fix the five live bugs. Tasks 8–9 close the two silent fallbacks. Each task ships independently and is worth reviewing on its own.

---

### Task 1: `wire_name` and `from_wire` on `ExtensionKind`

The five stale call sites each re-derive the kind↔string mapping by hand, in two different directions, which is why they drifted apart. This task gives them one place to derive from.

**Files:**
- Modify: `crates/greentic-extension-sdk-contract/src/kind.rs`
- Test: `crates/greentic-extension-sdk-contract/tests/kind.rs`

**Interfaces:**
- Consumes: `ExtensionKind::ALL`, `ExtensionKind::dir_name` (existing).
- Produces:
  - `ExtensionKind::wire_name(self) -> &'static str` — the `serde` rename value (`"DesignExtension"`, `"wasix:mcp/router"`, …).
  - `ExtensionKind::from_wire(s: &str) -> Option<Self>` — inverse of `wire_name`.
  - `ExtensionKind::from_dir_name(s: &str) -> Option<Self>` — inverse of `dir_name`, needed by Task 5.

- [ ] **Step 1: Write the failing test**

Append to `crates/greentic-extension-sdk-contract/tests/kind.rs`:

```rust
use greentic_extension_sdk_contract::ExtensionKind;

/// `wire_name` must agree with what serde actually emits. If someone adds a
/// variant and sets `#[serde(rename)]` without updating `wire_name`, this
/// catches it — the two are separate declarations and will otherwise drift.
#[test]
fn wire_name_matches_serde() {
    for kind in ExtensionKind::ALL {
        let json = serde_json::to_string(&kind).expect("kind serializes");
        let expected = format!("\"{}\"", kind.wire_name());
        assert_eq!(json, expected, "wire_name disagrees with serde for {kind:?}");
    }
}

#[test]
fn from_wire_round_trips_every_variant() {
    for kind in ExtensionKind::ALL {
        assert_eq!(
            ExtensionKind::from_wire(kind.wire_name()),
            Some(kind),
            "from_wire failed to round-trip {kind:?}"
        );
    }
}

#[test]
fn from_dir_name_round_trips_every_variant() {
    for kind in ExtensionKind::ALL {
        assert_eq!(
            ExtensionKind::from_dir_name(kind.dir_name()),
            Some(kind),
            "from_dir_name failed to round-trip {kind:?}"
        );
    }
}

#[test]
fn unknown_strings_are_rejected() {
    assert_eq!(ExtensionKind::from_wire("AddonExtension"), None);
    assert_eq!(ExtensionKind::from_wire(""), None);
    assert_eq!(ExtensionKind::from_dir_name("addon"), None);
    assert_eq!(ExtensionKind::from_dir_name(""), None);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-contract --test kind`
Expected: FAIL — `no function or associated item named 'wire_name' found`.

- [ ] **Step 3: Write minimal implementation**

In `crates/greentic-extension-sdk-contract/src/kind.rs`, inside `impl ExtensionKind`, after `dir_name`:

```rust
    /// The `serde` wire value for this kind, as it appears in
    /// `describe.json`'s `kind` field.
    ///
    /// Declared separately from the `#[serde(rename = "…")]` attributes
    /// because attributes are not readable at runtime. `wire_name_matches_serde`
    /// in `tests/kind.rs` asserts the two agree, so the duplication cannot
    /// drift silently.
    #[must_use]
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Design => "DesignExtension",
            Self::Bundle => "BundleExtension",
            Self::Deploy => "DeployExtension",
            Self::Provider => "ProviderExtension",
            Self::WasixMcpRouter => "wasix:mcp/router",
        }
    }

    /// Inverse of [`Self::wire_name`]. `None` for anything this contract
    /// version does not know — callers decide whether that is an error or a
    /// skip.
    #[must_use]
    pub fn from_wire(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.wire_name() == s)
    }

    /// Inverse of [`Self::dir_name`].
    #[must_use]
    pub fn from_dir_name(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.dir_name() == s)
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p greentic-extension-sdk-contract --test kind`
Expected: PASS, all four new tests plus the existing ones.

- [ ] **Step 5: Commit**

```bash
git add crates/greentic-extension-sdk-contract/src/kind.rs \
        crates/greentic-extension-sdk-contract/tests/kind.rs
git commit -m "feat(contract): add wire_name, from_wire and from_dir_name to ExtensionKind

Five call sites across the CLI re-derive the kind-to-string mapping by hand,
in both directions, and have drifted apart. Give them one derivation point."
```

---

### Task 2: Guard the JSON Schema `kind` enum against the Rust enum

`schemas/describe-v2.json` hand-maintains its `kind` enum. A new variant compiles in Rust and then fails every `gtdx validate` and `gtdx publish` at schema time, with an error that points at the descriptor rather than at the schema.

**Files:**
- Test: `crates/greentic-extension-sdk-contract/tests/schema_kind_enum.rs` (create)

**Interfaces:**
- Consumes: `ExtensionKind::ALL`, `ExtensionKind::wire_name` (Task 1).
- Produces: nothing consumed by later tasks. This is a guard.

- [ ] **Step 1: Write the failing test**

Create `crates/greentic-extension-sdk-contract/tests/schema_kind_enum.rs`:

```rust
//! `describe-v2.json` hand-maintains the `kind` enum. Nothing generates it
//! from `ExtensionKind`, so a new variant compiles in Rust and then fails
//! every `gtdx validate` and `gtdx publish` — blaming the descriptor, not the
//! schema. This test makes the schema fail instead, at build time.

use greentic_extension_sdk_contract::ExtensionKind;

/// `wasix:mcp/router` is deliberately absent from describe-v2: those
/// artifacts validate against `describe-mcp-v1.json` instead. Every other
/// kind must be present.
fn kinds_expected_in_v2() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = ExtensionKind::ALL
        .into_iter()
        .filter(|k| *k != ExtensionKind::WasixMcpRouter)
        .map(ExtensionKind::wire_name)
        .collect();
    v.sort_unstable();
    v
}

#[test]
fn describe_v2_kind_enum_matches_extension_kind() {
    let raw = include_str!("../schemas/describe-v2.json");
    let schema: serde_json::Value = serde_json::from_str(raw).expect("schema is valid JSON");

    let enum_values = schema["properties"]["kind"]["enum"]
        .as_array()
        .expect("describe-v2.json properties.kind.enum is an array");

    let mut actual: Vec<&str> = enum_values
        .iter()
        .map(|v| v.as_str().expect("kind enum entries are strings"))
        .collect();
    actual.sort_unstable();

    assert_eq!(
        actual,
        kinds_expected_in_v2(),
        "describe-v2.json's kind enum has drifted from ExtensionKind. \
         Add the new variant to the schema (and decide whether it needs its \
         own schema file, as wasix:mcp/router does)."
    );
}

#[test]
fn describe_mcp_v1_pins_the_router_kind() {
    let raw = include_str!("../schemas/describe-mcp-v1.json");
    let schema: serde_json::Value = serde_json::from_str(raw).expect("schema is valid JSON");

    assert_eq!(
        schema["properties"]["kind"]["const"].as_str(),
        Some(ExtensionKind::WasixMcpRouter.wire_name()),
        "describe-mcp-v1.json's kind const must match ExtensionKind::WasixMcpRouter"
    );
}
```

- [ ] **Step 2: Run test to verify it passes today**

Run: `cargo test -p greentic-extension-sdk-contract --test schema_kind_enum`
Expected: PASS. Both schemas are correct *today* — this test exists to fail the day they stop being.

If it fails now, the schema and enum have already drifted; fix the schema before continuing, and note what drifted in the commit message.

- [ ] **Step 3: Prove the guard actually guards**

Temporarily add a sixth variant to confirm the test fires. In `crates/greentic-extension-sdk-contract/src/kind.rs` add `Probe` with `#[serde(rename = "ProbeExtension")]`, add it to `ALL` (bump the array length to 6), `dir_name` (`"probe"`), and `wire_name` (`"ProbeExtension"`).

Run: `cargo test -p greentic-extension-sdk-contract --test schema_kind_enum`
Expected: FAIL on `describe_v2_kind_enum_matches_extension_kind`, reporting `ProbeExtension` as unexpected.

**Then revert the probe completely** — `git checkout crates/greentic-extension-sdk-contract/src/kind.rs` — and re-run to confirm PASS. Per the global constraints, the enum must stay at five variants.

- [ ] **Step 4: Commit**

```bash
git add crates/greentic-extension-sdk-contract/tests/schema_kind_enum.rs
git commit -m "test(contract): fail when describe schemas drift from ExtensionKind

The kind enum in describe-v2.json is hand-maintained. A new variant compiled
in Rust and then failed gtdx validate and gtdx publish at schema time, with
an error blaming the descriptor rather than the schema."
```

---

### Task 3: Close the `permissions` schema hole

`permissions` has no `additionalProperties: false`, which is the only reason `oauthProviders` validates while being absent from the schema entirely. The same hole means a typo'd permission key passes validation silently today.

**Files:**
- Modify: `crates/greentic-extension-sdk-contract/schemas/describe-v2.json` (`$defs.runtime.properties.permissions`)
- Modify: `crates/greentic-extension-sdk-contract/schemas/describe-mcp-v1.json` (same block)
- Test: `crates/greentic-extension-sdk-contract/tests/schema_permissions.rs` (create)

**Interfaces:**
- Consumes: `validate_describe_json` from `greentic_extension_sdk_contract::schema`.
- Produces: nothing consumed by later tasks.

- [ ] **Step 1: Write the failing test**

Create `crates/greentic-extension-sdk-contract/tests/schema_permissions.rs`:

```rust
//! `permissions` accepted unknown keys, which is why `oauthProviders`
//! validated while being absent from the schema — and why a typo'd
//! permission key passed validation while granting nothing.

use greentic_extension_sdk_contract::schema::validate_describe_json;

/// `validate_describe_json` takes a parsed `&serde_json::Value`, not a string.
fn validate(describe_json: &str) -> Result<(), greentic_extension_sdk_contract::ContractError> {
    let value: serde_json::Value =
        serde_json::from_str(describe_json).expect("fixture is valid JSON");
    validate_describe_json(&value)
}

/// A minimal but complete v2 describe, with `permissions` filled in by the
/// caller. Kept inline rather than loaded from a fixture so a schema change
/// that breaks the shape shows up here as a compile-visible diff.
fn describe_with_permissions(permissions: &str) -> String {
    format!(
        r#"{{
          "$schema": "https://store.greentic.cloud/schemas/describe-v2.json",
          "apiVersion": "greentic.ai/v2",
          "kind": "DesignExtension",
          "compat": {{
            "minDesigner": "1.2.0",
            "minRunner": "1.2.0",
            "contract": "1.2.8"
          }},
          "metadata": {{
            "id": "greentic.perm-test",
            "version": "0.1.0",
            "displayName": "Permission Test",
            "description": "Fixture for permission schema validation.",
            "author": {{ "name": "Greentic" }}
          }},
          "engine": {{ "greenticDesigner": ">=1.2.0", "extRuntime": ">=1.2.0" }},
          "capabilities": {{ "offered": [], "required": [] }},
          "runtime": {{
            "memoryLimitMB": 64,
            "permissions": {permissions},
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
          "contributions": {{}}
        }}"#
    )
}

#[test]
fn known_permission_keys_validate() {
    let d = describe_with_permissions(
        r#"{ "network": ["https://api.example.com/*"],
              "secrets": ["greentic://tenant/*"],
              "callExtensionKinds": ["DesignExtension"],
              "llmRoles": ["some_role"],
              "oauthProviders": ["hubspot"] }"#,
    );
    let result = validate(&d);
    assert!(result.is_ok(), "known permission keys must validate: {result:?}");
}

#[test]
fn a_typod_permission_key_is_rejected() {
    // `netwrok`, not `network`. Before additionalProperties:false this
    // validated cleanly and granted nothing — the extension then failed at
    // runtime with a permission error that pointed nowhere useful.
    let d = describe_with_permissions(r#"{ "netwrok": ["https://api.example.com/*"] }"#);
    assert!(
        validate(&d).is_err(),
        "an unknown permission key must be rejected, not silently ignored"
    );
}

#[test]
fn empty_permissions_validate() {
    let d = describe_with_permissions("{}");
    let result = validate(&d);
    assert!(result.is_ok(), "empty permissions must validate: {result:?}");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-contract --test schema_permissions`
Expected: `a_typod_permission_key_is_rejected` FAILS (the typo currently validates). `known_permission_keys_validate` may also fail, because `oauthProviders` is absent from the schema and will be rejected once `additionalProperties: false` is added — which is exactly why both fixes belong in one task.

- [ ] **Step 3: Fix both schemas**

In `crates/greentic-extension-sdk-contract/schemas/describe-v2.json`, inside `$defs.runtime.properties.permissions`, add the missing property and close the object. The block becomes:

```json
"permissions": {
  "description": "Host permissions the extension requests. `gtdx install` prints the network, secrets and cross-extension requests and asks for confirmation before installing, unless the install was pre-approved (`--yes` / CI).",
  "type": "object",
  "additionalProperties": false,
  "properties": {
    "network": {
      "description": "URL patterns the extension may reach, e.g. `https://api.example.com/*`. `gtdx publish` requires `https://`, with one exception: plain `http://` is accepted for loopback hosts (`127.0.0.1`, `localhost`, `[::1]`) only. This mirrors the extension runtime, which honours plain http for loopback hosts and drops non-loopback http patterns.",
      "type": "array",
      "items": { "type": "string" }
    },
    "secrets": {
      "description": "Secret keys the extension declares it needs to read. Listed in the `gtdx install` consent prompt.",
      "type": "array",
      "items": { "type": "string" }
    },
    "callExtensionKinds": {
      "description": "Extension kinds this extension may call into. Surfaced in the `gtdx install` consent prompt as a cross-extension request.",
      "type": "array",
      "items": { "type": "string" }
    },
    "llmRoles": {
      "description": "LLM roles (wire names, e.g. sorla_composer) this extension may request from the host greentic:extension-host/llm import.",
      "type": "array",
      "items": { "type": "string" }
    },
    "oauthProviders": {
      "description": "OAuth provider ids (e.g. `hubspot`) this extension may request tokens for via the host `greentic:oauth-broker/broker-v1` import. The host rejects `get-token` for any provider not listed here.",
      "type": "array",
      "items": { "type": "string" }
    }
  }
}
```

Apply the same two changes — add `oauthProviders`, add `"additionalProperties": false` — to the `permissions` block in `crates/greentic-extension-sdk-contract/schemas/describe-mcp-v1.json`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p greentic-extension-sdk-contract`
Expected: PASS. Run the whole crate, not just the new test — `deny_unknown_fields` on the Rust `Permissions` struct means existing round-trip tests exercise this too.

- [ ] **Step 5: Check the templates still validate**

Run: `cargo test -p greentic-extension-sdk-cli --test templates_schema_conformance`
Expected: PASS. Every `templates/*/describe.json.tmpl` is validated against the schema; if a template carries a stray permission key, this is where it surfaces.

- [ ] **Step 6: Commit**

```bash
git add crates/greentic-extension-sdk-contract/schemas/describe-v2.json \
        crates/greentic-extension-sdk-contract/schemas/describe-mcp-v1.json \
        crates/greentic-extension-sdk-contract/tests/schema_permissions.rs
git commit -m "fix(contract): reject unknown permission keys, document oauthProviders

permissions had no additionalProperties:false, so a typo'd key validated
cleanly and granted nothing. It is also why oauthProviders passed validation
while being absent from the schema entirely."
```

---

### Task 4: `install.rs` — search every kind, not four of them

`warn_if_designer_cannot_load` hand-lists four kinds and omits `Provider`, so installing a provider extension never runs the designer-compatibility check.

**Files:**
- Modify: `crates/greentic-extension-sdk-cli/src/commands/install.rs:150-165`
- Test: `crates/greentic-extension-sdk-cli/src/commands/install.rs` (new `#[cfg(test)] mod tests` at end of file)

**Interfaces:**
- Consumes: `ExtensionKind::ALL`, `Storage::kind_dir`.
- Produces: `fn find_installed_describe_bytes(storage: &Storage, name: &str, version: &str) -> Option<Vec<u8>>` — extracted so the kind sweep is testable without a designer present.

- [ ] **Step 1: Write the failing test**

Append to `crates/greentic-extension-sdk-cli/src/commands/install.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::find_installed_describe_bytes;
    use greentic_extension_sdk_contract::ExtensionKind;
    use greentic_extension_sdk_registry::storage::Storage;

    /// The sweep must cover every kind. It hand-listed four and omitted
    /// `Provider`, so a provider install silently skipped the
    /// designer-compatibility warning.
    #[test]
    fn finds_a_describe_installed_under_any_kind() {
        for kind in ExtensionKind::ALL {
            let home = tempfile::tempdir().expect("tempdir");
            let storage = Storage::new(home.path());
            let dir = storage.kind_dir(kind).join("greentic.demo-0.1.0");
            std::fs::create_dir_all(&dir).expect("create install dir");
            std::fs::write(dir.join("describe.json"), br#"{"marker":"found"}"#)
                .expect("write describe");

            let found = find_installed_describe_bytes(&storage, "greentic.demo", "0.1.0");
            assert_eq!(
                found.as_deref(),
                Some(&br#"{"marker":"found"}"#[..]),
                "describe under kind {kind:?} was not found"
            );
        }
    }

    #[test]
    fn absent_describe_is_none() {
        let home = tempfile::tempdir().expect("tempdir");
        let storage = Storage::new(home.path());
        assert!(find_installed_describe_bytes(&storage, "greentic.absent", "0.1.0").is_none());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-cli install::tests`
Expected: FAIL — `cannot find function 'find_installed_describe_bytes'`.

- [ ] **Step 3: Extract the helper and derive from `ALL`**

Replace the body of `warn_if_designer_cannot_load` in `crates/greentic-extension-sdk-cli/src/commands/install.rs`:

```rust
/// Read an installed extension's `describe.json`, whichever kind directory it
/// landed in.
///
/// Sweeps `ExtensionKind::ALL` rather than a hand-written list: this call site
/// listed four kinds and omitted `Provider`, so provider installs skipped the
/// designer-compatibility check entirely while still reporting success.
fn find_installed_describe_bytes(
    storage: &Storage,
    name: &str,
    version: &str,
) -> Option<Vec<u8>> {
    let dir_name = format!("{name}-{version}");
    ExtensionKind::ALL.into_iter().find_map(|kind| {
        std::fs::read(storage.kind_dir(kind).join(&dir_name).join("describe.json")).ok()
    })
}

fn warn_if_designer_cannot_load(storage: &Storage, name: &str, version: &str) {
    let Some(bytes) = find_installed_describe_bytes(storage, name, version) else {
        return;
    };
    crate::dev::installer::warn_if_designer_cannot_load(&bytes, name);
}
```

Move the `use greentic_extension_sdk_contract::ExtensionKind;` that was inside the old function body up to the module's imports at the top of the file.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p greentic-extension-sdk-cli install::tests`
Expected: PASS, both tests.

- [ ] **Step 5: Commit**

```bash
git add crates/greentic-extension-sdk-cli/src/commands/install.rs
git commit -m "fix(cli): run the designer-compatibility check for every kind

warn_if_designer_cannot_load hand-listed four kinds and omitted Provider, so
installing a provider extension skipped the check and reported success."
```

---

### Task 5: `search.rs` — accept every kind

`gtdx search --kind provider` answers `unknown kind: provider`. The match hand-lists four dir names and omits `provider`.

**Files:**
- Modify: `crates/greentic-extension-sdk-cli/src/commands/search.rs:27-34`
- Test: `crates/greentic-extension-sdk-cli/src/commands/search.rs` (new `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `ExtensionKind::from_dir_name` (Task 1), `ExtensionKind::ALL`.
- Produces: `fn parse_kind_arg(s: Option<&str>) -> anyhow::Result<Option<ExtensionKind>>`.

- [ ] **Step 1: Write the failing test**

Append to `crates/greentic-extension-sdk-cli/src/commands/search.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::parse_kind_arg;
    use greentic_extension_sdk_contract::ExtensionKind;

    /// `--kind provider` answered "unknown kind: provider" because the match
    /// hand-listed four dir names. Every kind must parse.
    #[test]
    fn every_kind_dir_name_parses() {
        for kind in ExtensionKind::ALL {
            let parsed = parse_kind_arg(Some(kind.dir_name()))
                .unwrap_or_else(|e| panic!("{} should parse: {e}", kind.dir_name()));
            assert_eq!(parsed, Some(kind));
        }
    }

    #[test]
    fn no_kind_means_no_filter() {
        assert_eq!(parse_kind_arg(None).expect("None is valid"), None);
    }

    #[test]
    fn an_unknown_kind_is_an_error() {
        let err = parse_kind_arg(Some("nonsense")).expect_err("unknown kind must error");
        assert!(
            err.to_string().contains("nonsense"),
            "the error should name the offending input, got: {err}"
        );
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-cli search::tests`
Expected: FAIL — `cannot find function 'parse_kind_arg'`.

- [ ] **Step 3: Write the implementation**

In `crates/greentic-extension-sdk-cli/src/commands/search.rs`, add above `pub async fn run`:

```rust
/// Resolve the `--kind` argument to a filter.
///
/// Derives from `ExtensionKind::ALL` rather than matching literals: the
/// hand-written match omitted `provider`, so `--kind provider` answered
/// "unknown kind: provider" for a kind that has existed since 1.2.0.
fn parse_kind_arg(
    kind: Option<&str>,
) -> anyhow::Result<Option<greentic_extension_sdk_contract::ExtensionKind>> {
    use greentic_extension_sdk_contract::ExtensionKind;

    match kind {
        None => Ok(None),
        Some(s) => ExtensionKind::from_dir_name(s).map(Some).ok_or_else(|| {
            let known: Vec<&str> = ExtensionKind::ALL.iter().map(|k| k.dir_name()).collect();
            anyhow::anyhow!("unknown kind: {s} (known kinds: {})", known.join(", "))
        }),
    }
}
```

Then replace the `let kind = match args.kind.as_deref() { … };` block in `run` with:

```rust
    let kind = parse_kind_arg(args.kind.as_deref())?;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p greentic-extension-sdk-cli search::tests`
Expected: PASS, all three tests.

- [ ] **Step 5: Commit**

```bash
git add crates/greentic-extension-sdk-cli/src/commands/search.rs
git commit -m "fix(cli): gtdx search --kind provider is no longer an unknown kind

The match hand-listed four dir names and omitted provider. Derive the set
from ExtensionKind::ALL and name the known kinds in the error."
```

---

### Task 6: `info.rs` and `list.rs` — derive both kind sweeps

Both hand-list all five kinds. They are correct today and will be wrong the moment a sixth lands. `list.rs` additionally has a `KindArg` clap enum that must stay hand-written (clap needs the literal variants) — so it gets a coverage test instead.

**Files:**
- Modify: `crates/greentic-extension-sdk-cli/src/commands/info.rs:41-47`
- Modify: `crates/greentic-extension-sdk-cli/src/commands/list.rs:48-58`
- Test: `crates/greentic-extension-sdk-cli/src/commands/list.rs` (new `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `ExtensionKind::ALL`, `KindArg::to_extension_kind` (existing).
- Produces: nothing consumed by later tasks.

- [ ] **Step 1: Write the failing test**

Append to `crates/greentic-extension-sdk-cli/src/commands/list.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::KindArg;
    use clap::ValueEnum;
    use greentic_extension_sdk_contract::ExtensionKind;

    /// `KindArg` must stay hand-written because clap needs literal variants,
    /// so it cannot derive from `ExtensionKind::ALL`. This test is the
    /// substitute: every kind must be reachable from the CLI, or a kind
    /// exists that `gtdx list --kind` cannot name.
    #[test]
    fn kind_arg_covers_every_extension_kind() {
        let reachable: Vec<ExtensionKind> = KindArg::value_variants()
            .iter()
            .filter_map(|k| k.to_extension_kind())
            .collect();

        for kind in ExtensionKind::ALL {
            assert!(
                reachable.contains(&kind),
                "no KindArg variant maps to {kind:?} — add one, with \
                 #[value(name = \"{}\")]",
                kind.dir_name()
            );
        }
    }

    /// The `--kind all` branch must sweep every kind, not a frozen list.
    #[test]
    fn all_expands_to_every_kind() {
        assert_eq!(super::kinds_for(KindArg::All), ExtensionKind::ALL.to_vec());
    }

    #[test]
    fn a_specific_kind_expands_to_just_that_kind() {
        assert_eq!(super::kinds_for(KindArg::Provider), vec![ExtensionKind::Provider]);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-cli list::tests`
Expected: FAIL — `cannot find function 'kinds_for'`.

- [ ] **Step 3: Extract `kinds_for` in `list.rs` and derive from `ALL`**

In `crates/greentic-extension-sdk-cli/src/commands/list.rs`, add after the `impl KindArg` block:

```rust
/// Expand the `--kind` argument into the set of kinds to sweep.
///
/// `All` derives from `ExtensionKind::ALL`; it used to be a hand-written vec,
/// which is the same pattern that left `gtdx search` unable to see providers.
fn kinds_for(arg: KindArg) -> Vec<ExtensionKind> {
    arg.to_extension_kind()
        .map_or_else(|| ExtensionKind::ALL.to_vec(), |kind| vec![kind])
}
```

Then in `run`, replace the `let kinds: Vec<ExtensionKind> = if let Some(kind) = … { … } else { vec![ … ] };` block with:

```rust
    let kinds: Vec<ExtensionKind> = kinds_for(args.kind);
```

- [ ] **Step 4: Derive the sweep in `info.rs`**

In `crates/greentic-extension-sdk-cli/src/commands/info.rs`, replace the `let all_kinds = [ … ];` array in `find_installed` with:

```rust
    // Derived, not hand-listed: a kind missing from this sweep makes
    // `gtdx info` report an installed extension as absent.
    let all_kinds = ExtensionKind::ALL;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p greentic-extension-sdk-cli list::tests && cargo test -p greentic-extension-sdk-cli info`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/greentic-extension-sdk-cli/src/commands/list.rs \
        crates/greentic-extension-sdk-cli/src/commands/info.rs
git commit -m "refactor(cli): derive the list and info kind sweeps from ExtensionKind::ALL

Both were correct but hand-written. KindArg must stay literal for clap, so it
gets a coverage test asserting every kind is reachable from the CLI."
```

---

### Task 7: `lint/rules.rs` — stop skipping MCP routers

`kind_dir_name` re-implements `dir_name()` from the wire string and omits `wasix:mcp/router`, returning `None`. `W_DESCRIBE_DIFF_BREAKING` then silently skips every MCP router — a lint that reports nothing looks identical to a lint that found nothing.

**Files:**
- Modify: `crates/greentic-extension-sdk-cli/src/commands/lint/rules.rs:94-104`
- Test: `crates/greentic-extension-sdk-cli/src/commands/lint/tests.rs`

**Interfaces:**
- Consumes: `ExtensionKind::from_wire`, `ExtensionKind::dir_name` (Task 1).
- Produces: nothing consumed by later tasks.

- [ ] **Step 1: Write the failing test**

Append to `crates/greentic-extension-sdk-cli/src/commands/lint/tests.rs`:

```rust
/// `kind_dir_name` re-implemented `dir_name()` from the wire string and
/// omitted `wasix:mcp/router`, so `W_DESCRIBE_DIFF_BREAKING` silently skipped
/// every MCP router. A lint that reports nothing is indistinguishable from a
/// lint that found nothing.
#[test]
fn kind_dir_name_resolves_every_kind() {
    use greentic_extension_sdk_contract::ExtensionKind;

    for kind in ExtensionKind::ALL {
        assert_eq!(
            super::rules::kind_dir_name(kind.wire_name()),
            Some(kind.dir_name()),
            "kind_dir_name failed for wire name {}",
            kind.wire_name()
        );
    }
}

#[test]
fn kind_dir_name_rejects_an_unknown_wire_name() {
    assert_eq!(super::rules::kind_dir_name("AddonExtension"), None);
}
```

If `kind_dir_name` is private to `rules`, mark it `pub(super)` so the sibling test module can reach it — do not widen it further.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-cli lint::tests::kind_dir_name`
Expected: FAIL on `wasix:mcp/router` — got `None`, expected `Some("mcp")`.

- [ ] **Step 3: Derive from the contract**

Replace `kind_dir_name` in `crates/greentic-extension-sdk-cli/src/commands/lint/rules.rs`:

```rust
/// Map a v1/v2 `kind` string (`"DesignExtension"` etc.) to the on-disk
/// directory name (`"design"` etc.) the installer writes into.
///
/// Derived from the contract rather than re-listed here: the hand-written
/// version omitted `wasix:mcp/router` and returned `None`, which made
/// `W_DESCRIBE_DIFF_BREAKING` skip every MCP router without saying so.
pub(super) fn kind_dir_name(kind: &str) -> Option<&'static str> {
    greentic_extension_sdk_contract::ExtensionKind::from_wire(kind)
        .map(greentic_extension_sdk_contract::ExtensionKind::dir_name)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p greentic-extension-sdk-cli lint`
Expected: PASS. Run the whole `lint` module — `check_describe_diff_breaking` now runs against MCP routers for the first time, and any fixture that relied on the rule being skipped will surface here.

- [ ] **Step 5: Commit**

```bash
git add crates/greentic-extension-sdk-cli/src/commands/lint/rules.rs \
        crates/greentic-extension-sdk-cli/src/commands/lint/tests.rs
git commit -m "fix(cli): W_DESCRIBE_DIFF_BREAKING no longer skips MCP routers

kind_dir_name re-implemented dir_name() from the wire string and omitted
wasix:mcp/router, returning None. The rule then skipped those extensions
silently, which reads exactly like finding no breakage."
```

---

### Task 8: `load_templates_kind` — a missing template arm must fail loudly

`_ => Vec::new()` means a kind with no template arm scaffolds **zero kind files** and reports success. The user gets a directory with only the common overlay and no `src/lib.rs`, no `describe.json`, no `Cargo.toml`.

**Files:**
- Modify: `crates/greentic-extension-sdk-cli/src/scaffold/template.rs:84-106`
- Modify: `crates/greentic-extension-sdk-cli/src/commands/new/mod.rs:375,413`
- Modify: `crates/greentic-extension-sdk-cli/src/commands/openapi/mod.rs:142`
- Test: `crates/greentic-extension-sdk-cli/src/scaffold/template.rs` (existing `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: nothing new.
- Produces: `load_templates_kind(kind: &str) -> anyhow::Result<Vec<TemplateEntry>>` — signature change; every call site must handle the `Result`.

- [ ] **Step 1: Write the failing test**

Append to the existing `mod tests` in `crates/greentic-extension-sdk-cli/src/scaffold/template.rs`:

```rust
/// A kind with no match arm used to scaffold zero kind files and report
/// success — the user got the common overlay and nothing else: no
/// src/lib.rs, no describe.json, no Cargo.toml.
#[test]
fn an_unknown_kind_is_an_error() {
    let err = load_templates_kind("addon").expect_err("unknown kind must error");
    assert!(
        err.to_string().contains("addon"),
        "the error should name the offending kind, got: {err}"
    );
}

/// Every kind the scaffold CLI can produce must resolve to a non-empty
/// template set. `openapi-connector` is included because `gtdx openapi`
/// loads it directly, without going through `Kind`.
#[test]
fn every_scaffoldable_kind_resolves_to_files() {
    for kind in [
        "design",
        "bundle",
        "deploy",
        "provider",
        "wasm-component",
        "llm",
        "mcp",
        "openapi-connector",
    ] {
        let entries = load_templates_kind(kind)
            .unwrap_or_else(|e| panic!("{kind} must resolve: {e}"));
        assert!(!entries.is_empty(), "{kind} resolved to zero files");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-cli scaffold::template::tests`
Expected: FAIL — `expect_err` panics because the current signature returns `Vec`, not `Result`; the file will not compile.

- [ ] **Step 3: Change the signature**

In `crates/greentic-extension-sdk-cli/src/scaffold/template.rs`, change the function and its fallback arm:

```rust
/// Load the template tree for a scaffold kind.
///
/// Errors on an unknown kind rather than returning an empty set: the previous
/// `_ => Vec::new()` meant a kind whose match arm was forgotten scaffolded the
/// common overlay and nothing else, then reported success.
pub fn load_templates_kind(kind: &str) -> anyhow::Result<Vec<TemplateEntry>> {
    let entries = match kind {
        "design" => collect(&TEMPLATES_DESIGN),
        "bundle" => collect(&TEMPLATES_BUNDLE),
        "deploy" => collect(&TEMPLATES_DEPLOY),
        "provider" => collect(&TEMPLATES_PROVIDER),
        // `wasm-component` is a `design` extension whose describe additionally
        // declares the OCI component that executes its palette node, so it
        // reuses the design crate wholesale and overrides only the files that
        // genuinely differ. It used to carry its own two-crate workspace
        // (`extension/` + `runtime/`), which duplicated the crate, drifted
        // against the contract, and put the vendored WIT deps outside the
        // crate's target path so nothing it generated ever built.
        "wasm-component" => overlay(
            collect(&TEMPLATES_DESIGN),
            collect(&TEMPLATES_WASM_COMPONENT),
        ),
        "llm" => collect(&TEMPLATES_LLM),
        "mcp" => collect(&TEMPLATES_MCP),
        "openapi-connector" => collect(&TEMPLATES_OPENAPI_CONNECTOR),
        other => anyhow::bail!("no scaffold templates for kind `{other}`"),
    };
    Ok(entries)
}
```

- [ ] **Step 4: Update every call site**

There are eleven, three of them non-test:

- `crates/greentic-extension-sdk-cli/src/commands/new/mod.rs:375` — `template::load_templates_kind("mcp")` → append `?` before the existing `.` chain, e.g. `template::load_templates_kind("mcp")?`.
- `crates/greentic-extension-sdk-cli/src/commands/new/mod.rs:413` — `for entry in template::load_templates_kind(kind) {` → `for entry in template::load_templates_kind(kind)? {`. `render_templates` already returns `anyhow::Result<usize>`.
- `crates/greentic-extension-sdk-cli/src/commands/openapi/mod.rs:142` — `for entry in template::load_templates_kind("openapi-connector") {` → add `?`. Confirm the enclosing function returns `anyhow::Result`; if not, propagate it.

In `template.rs`'s own tests (lines ~289, 310, 334, 352, 375, 408, 437, 467), append `.expect("<kind> templates resolve")` to each call.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p greentic-extension-sdk-cli` then `cargo clippy -p greentic-extension-sdk-cli --all-targets -- -D warnings`
Expected: PASS on both. Clippy is included because a newly-introduced `?` in a function returning `()` is a compile error and a missed `.expect` is a `unused_must_use` warning, which is fatal under `-D warnings`.

- [ ] **Step 6: Commit**

```bash
git add crates/greentic-extension-sdk-cli/src/scaffold/template.rs \
        crates/greentic-extension-sdk-cli/src/commands/new/mod.rs \
        crates/greentic-extension-sdk-cli/src/commands/openapi/mod.rs
git commit -m "fix(cli): a kind with no template arm is an error, not an empty scaffold

load_templates_kind fell through to Vec::new(), so a forgotten match arm
produced a project with the common overlay and no crate at all, and said it
succeeded."
```

---

### Task 9: `wit_package_subdir_for` — an unmapped WIT file must fail loudly

`_ => "extension-misc"` puts a new WIT package in a directory that no `Cargo.toml.tmpl` references. The scaffold builds nothing, and the error surfaces as a missing WIT package rather than a missing mapping.

**Files:**
- Modify: `crates/greentic-extension-sdk-cli/src/commands/new/mod.rs:578-589` and its call site at `:430`
- Test: `crates/greentic-extension-sdk-cli/src/commands/new/mod.rs` (new `#[cfg(test)] mod wit_subdir_tests`)

**Interfaces:**
- Consumes: `crate::scaffold::embedded::wit_files()`.
- Produces: `wit_package_subdir_for(filename: &str) -> anyhow::Result<&'static str>` — signature change.

- [ ] **Step 1: Write the failing test**

Append to `crates/greentic-extension-sdk-cli/src/commands/new/mod.rs`:

```rust
#[cfg(test)]
mod wit_subdir_tests {
    use super::wit_package_subdir_for;

    /// Every embedded WIT file must have an explicit dependency directory.
    /// The old `_ => "extension-misc"` fallback put unmapped packages in a
    /// directory no Cargo.toml.tmpl references, so the scaffold built nothing
    /// and blamed a missing WIT package rather than a missing mapping.
    #[test]
    fn every_embedded_wit_file_is_mapped() {
        for file in crate::scaffold::embedded::wit_files() {
            let subdir = wit_package_subdir_for(file.name)
                .unwrap_or_else(|e| panic!("{} is unmapped: {e}", file.name));
            assert_ne!(
                subdir, "extension-misc",
                "{} still resolves to the old catch-all",
                file.name
            );
        }
    }

    #[test]
    fn an_unmapped_wit_file_is_an_error() {
        let err = wit_package_subdir_for("extension-addon.wit")
            .expect_err("an unmapped wit file must error");
        assert!(
            err.to_string().contains("extension-addon.wit"),
            "the error should name the file, got: {err}"
        );
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-cli wit_subdir_tests`
Expected: FAIL — the file will not compile, because `wit_package_subdir_for` returns `&'static str` and `expect_err` is not a method on it.

- [ ] **Step 3: Change the signature**

In `crates/greentic-extension-sdk-cli/src/commands/new/mod.rs`:

```rust
/// Map an embedded WIT filename to the vendored dependency directory it is
/// written into (`wit/deps/greentic/<subdir>/world.wit`).
///
/// Errors on an unmapped file rather than falling back to `extension-misc`:
/// that catch-all directory is referenced by no `Cargo.toml.tmpl`, so the
/// package was vendored somewhere nothing could find it and the scaffold
/// failed later, blaming a missing WIT package.
fn wit_package_subdir_for(filename: &str) -> anyhow::Result<&'static str> {
    let subdir = match filename {
        "extension-base.wit" => "extension-base",
        "extension-host.wit" => "extension-host",
        "extension-design.wit" => "extension-design",
        "extension-bundle.wit" => "extension-bundle",
        "extension-deploy.wit" => "extension-deploy",
        "extension-provider.wit" => "extension-provider",
        "runtime-side.wit" => "runtime-side",
        other => anyhow::bail!(
            "no vendored dependency directory mapped for WIT file `{other}` — \
             add an arm to wit_package_subdir_for and reference the directory \
             from the kind's Cargo.toml.tmpl"
        ),
    };
    Ok(subdir)
}
```

- [ ] **Step 4: Update the call site**

At `crates/greentic-extension-sdk-cli/src/commands/new/mod.rs:430`, inside `write_wit_and_lock`:

```rust
        let pkg_dir = wit_package_subdir_for(file.name)?;
```

`write_wit_and_lock` already returns `anyhow::Result`; confirm that before adding `?`.

- [ ] **Step 5: Map `extension-dw-composer.wit`, which the fallback was hiding**

`wit/` contains eight files; the match above has seven arms.
`extension-dw-composer.wit` has none, so it has been silently vendored into
`extension-misc` all along — a live instance of the bug this task fixes, found
by writing the test.

Add the arm, matching its declared package `greentic:dw-composer@0.2.0`:

```rust
        "extension-dw-composer.wit" => "dw-composer",
```

Run: `cargo test -p greentic-extension-sdk-cli wit_subdir_tests::every_embedded_wit_file_is_mapped`
Expected: PASS. Before adding the arm it fails naming `extension-dw-composer.wit`; run it once without the arm first to see that, then add it.

- [ ] **Step 6: Run the full crate**

Run: `cargo test -p greentic-extension-sdk-cli && cargo clippy -p greentic-extension-sdk-cli --all-targets -- -D warnings`
Expected: PASS on both.

- [ ] **Step 7: Commit**

```bash
git add crates/greentic-extension-sdk-cli/src/commands/new/mod.rs
git commit -m "fix(cli): an unmapped WIT file is an error, not extension-misc

The catch-all directory is referenced by no Cargo.toml.tmpl, so a new WIT
package was vendored where nothing could find it and the scaffold failed
later, blaming a missing package rather than a missing mapping."
```

---

### Task 10: Full gate and spec cross-reference

**Files:**
- Modify: `docs/superpowers/specs/2026-08-26-environment-addon-deployment-design.md` (§9.1 status)

- [ ] **Step 1: Run the full local gate**

Run: `./ci/local_check.sh`
Expected: all six steps PASS. This is the same script CI runs (`.github/workflows/ci.yml` mirrors it exactly), so a pass here means a green CI.

If `cargo publish --dry-run` fails for `greentic-extension-sdk-contract`, check whether Task 1's new methods need a version bump in `Cargo.toml` — they are additive, so a patch bump is correct, but the workspace pins `=1.2.8` between crates and both sides must move together.

- [ ] **Step 2: Confirm the guard actually guards, end to end**

Re-apply the Task 2 probe: add a sixth `ExtensionKind` variant, `ALL` length 6, `dir_name`, `wire_name`.

Run: `cargo test --workspace 2>&1 | grep -E "^(test .* FAILED|failures:)" | head -20`
Expected: failures in `schema_kind_enum` (Task 2) **and** `list::tests::kind_arg_covers_every_extension_kind` (Task 6). Those two are the tripwires that make a sixth kind cheap — if either passes with a sixth variant present, it is not doing its job.

**Revert the probe** — `git checkout crates/greentic-extension-sdk-contract/src/kind.rs` — and confirm `cargo test --workspace` is green.

- [ ] **Step 3: Mark §9.1 done in the spec**

In `docs/superpowers/specs/2026-08-26-environment-addon-deployment-design.md`, change the §9.1 heading line to:

```markdown
### 9.1 Independently shippable, should land first — DONE 2026-08-26
```

and add immediately below it:

```markdown
> Delivered by `docs/superpowers/plans/2026-08-26-kind-registry-hardening.md`.
> All five stale lists now derive from `ExtensionKind::ALL`, both silent
> fallbacks are hard errors, and two tripwire tests (`schema_kind_enum`,
> `kind_arg_covers_every_extension_kind`) fail when a sixth kind is added
> without updating the schema or the CLI.
```

- [ ] **Step 4: Commit**

```bash
git add docs/superpowers/specs/2026-08-26-environment-addon-deployment-design.md
git commit -m "docs(spec): mark §9.1 prerequisites delivered"
```

---

## Out of scope, and why

**`ExtensionKind::Addon` and the `extension-addon` WIT contract.** Blocked on `greentic:extension-base@0.3.0` (adding a WIT enum variant is breaking, and the runtime must serve `manifest@0.2.0` and `@0.3.0` concurrently during migration) — a cross-repo contract release, not a feature branch. Spec §9.2.

**Renderers, the environment model, and the reconcile loop.** These live in the commercial platform, not this SDK. Spec §4, §6, §7.

**The addon conformance suite in `sdk-testing`.** It asserts properties of an interface that does not exist yet. It belongs in the same plan as the contract.

**Unresolved spec questions that gate the next plan** (spec §11): whether an `AddonExtension` may carry `contributions`; whether secrets are excluded from desired state or marked write-only; and which of Terraform's ten lifecycle RPCs are needed at v0.1.0 versus retrofitted. None block this plan; all block the next one.
