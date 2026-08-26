# Addon WIT Contract (unblocked slice of Phase 2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the `greentic:extension-addon@0.1.0` WIT contract and ship D11's plan/apply consistency rule as a callable assertion, so the contract can be reviewed and an addon author can check conformance before any of it can execute.

**Architecture:** One new `.wit` file carrying four interfaces and two worlds, registered in the three places that enumerate WIT files. Then two additions to `greentic-extension-sdk-testing`, both pure JSON and free of any WIT-generated type: the consistency assertion the platform will call in production, and closure-taking property helpers for plan idempotency and stability.

**Tech Stack:** WIT (WebAssembly Component Model), Rust 1.95.0, `serde_json`, `wasm-tools` / `wit-parser` for syntax validation.

**Spec:** `docs/superpowers/specs/2026-08-26-environment-addon-deployment-design.md` — §5.1 as amended, plus D10, D11, D13, D16, D18–D21.

## Global Constraints

- Rust toolchain pinned to `1.95.0` (`rust-toolchain.toml`).
- No `unwrap()` or `panic!()` in non-test code. Tests may use `expect` with a message.
- Every SDK crate root carries `#![forbid(unsafe_code)]` — do not add `unsafe`.
- `ci/local_check.sh` is the gate: fmt, clippy `-D warnings`, `cargo test --workspace --all-features --locked`, release build, two `cargo publish --dry-run`. Clippy warnings are errors.
- **Run `cargo fmt --all` BEFORE committing, never after.**
- Conventional commits, one per task.
- **Do NOT add an `ExtensionKind` variant.** The enum stays at five. `AddonExtension` needs `extension-base@0.3.0`, which is a cross-repo contract release (spec §9.2).
- **Do NOT change the workspace version.** `1.2.11` was just released; the next bump is a separate release decision.
- **Do NOT add the `E_ADDON_IMAGE_NOT_PINNED` or `E_ADDON_BACKUP_MISMATCH` lint rules.** Both inspect a world no artifact can declare yet. A rule nothing can trigger is exactly the defect this project spent a fix wave removing — see the `W_DESCRIBE_DIFF_BREAKING` history in `docs/superpowers/plans/2026-08-26-kind-registry-hardening.md`.

---

## What is deliberately not here

Phase 2 as a whole is blocked. This plan is the slice that is not.

| Blocked | Why |
|---|---|
| `ExtensionKind::Addon` | Adding `addon` to `wit/extension-base.wit`'s `enum kind` is a breaking WIT change, forcing `extension-base@0.3.0` and a runtime that serves `manifest@0.2.0` and `@0.3.0` concurrently. |
| A `--kind addon` scaffold | Would generate a component that cannot declare its own kind. |
| The two addon lint rules | Nothing can declare the world they inspect. |
| A third-party addon marketplace | Production trust root (spec §9.2). |

The `.wit` file itself is not blocked: its interfaces need only
`extension-base/types@0.2.0`, which exists. Its worlds can be *declared*; only
a component *implementing* one needs the base bump. That is stated in the file
so a reader does not mistake it for an oversight.

---

### Task 1: `wit/extension-addon.wit` and its three registrations

**Files:**
- Create: `wit/extension-addon.wit`
- Modify: `crates/greentic-extension-sdk-cli/tests/contract_version_consistency.rs` (the `expected` map)
- Modify: `crates/greentic-extension-sdk-cli/src/scaffold/embedded.rs` (`wit_files_returns_all_embedded_packages`)
- Modify: `crates/greentic-extension-sdk-cli/src/commands/new/mod.rs` (`wit_package_subdir_for`)
- Test: `crates/greentic-extension-sdk-cli/tests/wit_addon_parses.rs` (create)

**Interfaces:**
- Consumes: `greentic:extension-base/types@0.2.0.{diagnostic, extension-error}` — already on disk.
- Produces: the `greentic:extension-addon@0.1.0` package. Nothing in Rust consumes it yet.

**Why three registrations.** The repo has three independent guards that enumerate `wit/`, and all three fail closed. `contract_version_consistency.rs` asserts the set of files on disk equals the set in its expected-version map. `embedded.rs`'s unit test asserts an exact count. `wit_package_subdir_for` errors on an unmapped filename — a guard this project added itself, precisely so a new WIT file cannot land half-registered.

- [ ] **Step 1: Write the failing parse test**

Create `crates/greentic-extension-sdk-cli/tests/wit_addon_parses.rs`:

```rust
//! A `.wit` file that does not parse is worse than no file: it reads as a
//! contract, reviews as a contract, and fails the first time anyone points a
//! toolchain at it. Nothing else in this repo parses `wit/` — the scaffold
//! copies the bytes and `cargo component` parses them later, in another
//! crate, at another time.

use std::path::PathBuf;

fn wit_root() -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("wit");
    root.is_dir().then(|| root)
}

#[test]
fn the_addon_contract_parses() {
    let Some(root) = wit_root() else {
        eprintln!("workspace wit/ not present (likely packaged tarball) — skipping");
        return;
    };

    let mut resolve = wit_parser::Resolve::default();
    // Parsing the whole directory resolves `extension-addon`'s import of
    // `extension-base/types` for real, rather than checking its syntax in
    // isolation and discovering the dangling reference downstream.
    resolve
        .push_dir(&root)
        .unwrap_or_else(|e| panic!("wit/ must parse as a resolvable set: {e:?}"));

    let found = resolve
        .packages
        .iter()
        .any(|(_, p)| p.name.namespace == "greentic" && p.name.name == "extension-addon");
    assert!(found, "greentic:extension-addon must be among the parsed packages");
}

/// The four interfaces are the contract's shape; a rename or an accidental
/// deletion should fail here rather than in a downstream repo.
#[test]
fn the_addon_contract_declares_its_four_interfaces() {
    let Some(root) = wit_root() else {
        eprintln!("workspace wit/ not present — skipping");
        return;
    };
    let mut resolve = wit_parser::Resolve::default();
    resolve.push_dir(&root).expect("wit/ parses");

    let pkg = resolve
        .packages
        .iter()
        .find(|(_, p)| p.name.namespace == "greentic" && p.name.name == "extension-addon")
        .map(|(_, p)| p)
        .expect("extension-addon package present");

    for want in ["validation", "workload", "reconciler", "backup"] {
        assert!(
            pkg.interfaces.contains_key(want),
            "extension-addon must declare interface {want:?}; has {:?}",
            pkg.interfaces.keys().collect::<Vec<_>>()
        );
    }
}

/// `backup` is optional by design (spec D19): a world exports it only when the
/// addon can genuinely snapshot. Two worlds is how WIT expresses that, since
/// it has no optional export.
#[test]
fn backup_is_optional_via_two_worlds() {
    let Some(root) = wit_root() else {
        eprintln!("workspace wit/ not present — skipping");
        return;
    };
    let mut resolve = wit_parser::Resolve::default();
    resolve.push_dir(&root).expect("wit/ parses");

    let pkg = resolve
        .packages
        .iter()
        .find(|(_, p)| p.name.namespace == "greentic" && p.name.name == "extension-addon")
        .map(|(_, p)| p)
        .expect("extension-addon package present");

    assert!(pkg.worlds.contains_key("addon-extension"));
    assert!(pkg.worlds.contains_key("addon-extension-with-backup"));
}
```

Add `wit-parser` under `[dev-dependencies]` in `crates/greentic-extension-sdk-cli/Cargo.toml`, at the version `cargo add --dev wit-parser --dry-run` resolves to. If the workspace already pins a `wit-parser` or `wasm-tools` version, use `{ workspace = true }` instead.

**The `wit-parser` API moves between versions.** `Resolve::push_dir`'s return type and the shape of `Package::name` have both changed across releases. If the code above does not compile against the version you resolve, adapt it — the assertions are what matter, not the exact calls. Do not weaken an assertion to make it compile: if you cannot reach `interfaces` or `worlds` on the parsed package, say so in your report rather than dropping those two tests.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-cli --test wit_addon_parses`
Expected: FAIL — `extension-addon package present` panics; the file does not exist yet.

- [ ] **Step 3: Write the WIT file**

Create `wit/extension-addon.wit`:

```wit
package greentic:extension-addon@0.1.0;

/// Behavioural validation the JSON Schema cannot express.
///
/// `contributions.addons[].config_schema` already checked shape. This checks
/// meaning: that a collection's vector size matches the embedding model the
/// config selects, that a name is free in this instance. Shape is a schema's
/// job; meaning needs code.
interface validation {
  use greentic:extension-base/types@0.2.0.{diagnostic};

  /// Returns `[]` when the config is usable. Diagnostic-only rather than
  /// `result<>`: there is no failure mode distinct from "here is what is
  /// wrong". Mirrors `bundling.validate-config`.
  validate-config: func(resource-id: string, config-json: string) -> list<diagnostic>;

  /// Takes `config-json` as well as the desired state. The most common thing
  /// wrong with desired state is disagreement with config — a collection's
  /// vector size against the model config selects — and without config the
  /// addon cannot check it, so the error surfaces at apply instead.
  validate-desired-state: func(
    resource-id: string,
    config-json: string,
    desired-json: string,
  ) -> list<diagnostic>;
}

/// What the addon needs run on its behalf. The addon never provisions; the
/// platform does. That split is what lets one declaration serve both hosted
/// and bring-your-own-cloud placement.
interface workload {
  use greentic:extension-base/types@0.2.0.{extension-error};

  record port {
    name: string,
    number: u16,
  }

  record volume {
    name: string,
    size-gb: u32,
    mount-path: string,
  }

  record probe {
    /// Readiness path on the primary container's first port. `none` means the
    /// addon is ready as soon as the container runs.
    http-path: option<string>,
    initial-delay-seconds: u32,
    period-seconds: u32,
  }

  record resource-request {
    /// What the addon needs to function at all. The platform refuses at plan
    /// time when the environment's cap is below this, naming both numbers —
    /// rather than scheduling a container that OOMs, which reads as an
    /// infrastructure fault and points at everything except the cause.
    min-memory-mb: u32,
    min-cpu-millis: u32,
    /// What the addon would prefer. Advisory; the platform may grant less.
    recommended-memory-mb: option<u32>,
    recommended-cpu-millis: option<u32>,
  }

  record container {
    /// OCI reference, digest-pinned. A mutable tag means the workload the
    /// plan approved is not necessarily the workload that runs, which drains
    /// the plan/apply consistency guarantee through the one field that
    /// guarantee does not cover.
    image: string,
    args: list<string>,
    env: list<tuple<string, string>>,
    ports: list<port>,
  }

  record container-workload {
    primary: container,
    sidecars: list<container>,
    volumes: list<volume>,
    resources: resource-request,
    readiness: probe,
  }

  record managed-workload {
    /// Vendor-neutral class the BYOC renderer maps to a cloud resource —
    /// `postgres`, `redis`. The hosted renderer refuses it at plan time with
    /// an explicit message rather than silently substituting a container: an
    /// addon asking for a managed Postgres wants managed backups and
    /// failover, which a container does not provide.
    service-class: string,
    params-json: string,
  }

  variant workload-spec {
    container(container-workload),
    managed(managed-workload),
  }

  /// PURE. No network, no side effects. Called at plan time and cacheable.
  render-workload: func(resource-id: string, config-json: string)
    -> result<workload-spec, extension-error>;
}

/// Day-2 state inside a running instance: Qdrant collections, Redis ACL
/// users. Level-triggered — the platform re-derives everything from observed
/// state rather than remembering which step it was on.
interface reconciler {
  use greentic:extension-base/types@0.2.0.{extension-error};

  /// How to reach the live instance. Every value the addon needs in order to
  /// authenticate arrives here and nowhere else. Credentials never appear in
  /// desired state, because a value `observe` cannot read back diffs forever
  /// and no plan is ever clean.
  record binding {
    outputs: list<tuple<string, string>>,
  }

  /// Why no plan could be produced yet. Present from v0.1.0 rather than
  /// retrofitted: day-2 config genuinely cannot be planned before the
  /// instance is reachable, and "I cannot plan this yet" must be a
  /// first-class answer rather than an error or a lie.
  enum deferred-reason {
    /// The instance is not up. Ask again after readiness.
    absent-prereq,
    /// A referenced output is not resolved yet.
    config-unknown,
  }

  record planned-change {
    /// The addon's own desired-state JSON, amended with any defaults the
    /// addon knows. The platform diffs `current-json` against this to render
    /// the plan — the addon never names an action. An opaque action payload
    /// would mean only the addon could say what an apply will do, and the
    /// platform would have to take its word.
    planned-json: string,
    /// JSON Pointer paths whose change cannot be applied in place and require
    /// destroy-and-recreate. This is what surfaces as destructive in the plan
    /// UI and what gates approval — a list the platform reads, not a boolean
    /// the addon asserts.
    requires-replace: list<string>,
  }

  variant plan-outcome {
    planned(planned-change),
    deferred(deferred-reason),
  }

  enum outcome {
    applied,
    /// A connection reset. The platform may retry.
    failed-retryable,
    /// A schema violation. Retrying changes nothing.
    failed-terminal,
  }

  record apply-report {
    /// State actually reached. The host asserts every leaf of `planned-json`
    /// is present and equal here and fails the apply if not: without that
    /// assertion a plan is a dry run — a suggestion, not a contract.
    observed-json: string,
    outcome: outcome,
    message: string,
  }

  /// Deliberately does NOT receive desired state. An observer that can see
  /// intent will reconcile toward it, and drift detection becomes
  /// unfalsifiable. Terraform withholds config from `ReadResource` for the
  /// same reason.
  observe: func(resource-id: string, binding: binding)
    -> result<string, extension-error>;

  /// PURE. No network, no side effects.
  plan: func(
    resource-id: string,
    current-json: string,
    desired-json: string,
  ) -> result<plan-outcome, extension-error>;

  apply: func(
    resource-id: string,
    binding: binding,
    current-json: string,
    planned-json: string,
  ) -> result<apply-report, extension-error>;
}

/// OPTIONAL. Exported only by `addon-extension-with-backup`.
///
/// A retention schedule is NOT here — that is observable, diffable and
/// convergent, so it is ordinary desired state and belongs in
/// `desired_state_schema`. What is here has no desired state at all: no
/// `observe` returns "a snapshot was taken before the thing that has not
/// happened yet". Modelling it as desired state would make it diff forever.
interface backup {
  use greentic:extension-base/types@0.2.0.{extension-error};
  use reconciler.{binding};

  record backup-handle {
    /// Opaque to the platform, meaningful to the addon. The platform stores
    /// it against the revision that triggered the backup and hands it back to
    /// `restore` unchanged.
    id: string,
    /// Shown next to the destructive change it guarded.
    summary: string,
    size-bytes: option<u64>,
  }

  /// Snapshot before a destructive change. The PLATFORM calls this; an author
  /// never schedules it.
  backup: func(resource-id: string, binding: binding)
    -> result<backup-handle, extension-error>;

  /// Destructive by definition. The platform gates it the way it gates a
  /// `requires-replace` path.
  restore: func(resource-id: string, binding: binding, handle: backup-handle)
    -> result<_, extension-error>;
}

/// NOTE FOR IMPLEMENTERS: these worlds can be declared today but not yet
/// correctly implemented. `manifest.get-identity()` returns an
/// `extension-identity` whose `kind` field is an enum that has no `addon`
/// variant, and adding one is a breaking change to `greentic:extension-base`.
/// The contract is here to be reviewed and versioned; a component implementing
/// it waits on `extension-base@0.3.0`. This is deliberate, not an oversight.
world addon-extension {
  import greentic:extension-base/types@0.2.0;
  import greentic:extension-host/logging@0.1.0;
  import greentic:extension-host/i18n@0.1.0;
  import greentic:extension-host/secrets@0.1.0;
  import greentic:extension-host/http@0.1.0;

  export greentic:extension-base/manifest@0.2.0;
  export greentic:extension-base/lifecycle@0.2.0;
  export validation;
  export workload;
  export reconciler;
}

/// The same, for an addon that can genuinely snapshot. WIT has no optional
/// export, so the capability is expressed as a second world — the shape
/// `extension-provider.wit` already uses for its own combinations. A platform
/// therefore reads the capability off the declared world rather than trusting
/// a boolean.
world addon-extension-with-backup {
  import greentic:extension-base/types@0.2.0;
  import greentic:extension-host/logging@0.1.0;
  import greentic:extension-host/i18n@0.1.0;
  import greentic:extension-host/secrets@0.1.0;
  import greentic:extension-host/http@0.1.0;

  export greentic:extension-base/manifest@0.2.0;
  export greentic:extension-base/lifecycle@0.2.0;
  export validation;
  export workload;
  export reconciler;
  export backup;
}
```

- [ ] **Step 4: Register in the three guards**

In `crates/greentic-extension-sdk-cli/tests/contract_version_consistency.rs`, add to the `expected` slice, keeping the file's existing ordering style:

```rust
        ("extension-addon.wit", "0.1.0"),
```

In `crates/greentic-extension-sdk-cli/src/scaffold/embedded.rs`, in `wit_files_returns_all_embedded_packages`, add the assertion and bump the count:

```rust
        assert!(files.iter().any(|f| f.name == "extension-addon.wit"));
        assert_eq!(files.len(), 9);
```

In `crates/greentic-extension-sdk-cli/src/commands/new/mod.rs`, add an arm to `wit_package_subdir_for` beside the others:

```rust
        "extension-addon.wit" => "extension-addon",
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p greentic-extension-sdk-cli --test wit_addon_parses`
Expected: PASS, all three tests.

Run: `cargo test -p greentic-extension-sdk-cli`
Expected: PASS. Run the whole crate — `build.rs` re-embeds `wit/` into `embedded-wit/1.2.11/` on the next build, so the count assertion and the version map both see the new file.

If `wit_files_returns_all_embedded_packages` reports 8 rather than 9, the embed did not refresh: `build.rs` skips when the destination exists. Touch `wit/` or run `cargo clean -p greentic-extension-sdk-cli` and rebuild.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all
git add wit/extension-addon.wit \
        crates/greentic-extension-sdk-cli/tests/wit_addon_parses.rs \
        crates/greentic-extension-sdk-cli/tests/contract_version_consistency.rs \
        crates/greentic-extension-sdk-cli/src/scaffold/embedded.rs \
        crates/greentic-extension-sdk-cli/src/commands/new/mod.rs \
        crates/greentic-extension-sdk-cli/Cargo.toml \
        crates/greentic-extension-sdk-cli/embedded-wit
git commit -m "feat(wit): add the greentic:extension-addon@0.1.0 contract

Four interfaces: validation and workload are pure, reconciler holds the
observe/plan/apply cycle, and backup is optional — expressed as a second
world, because WIT has no optional export and a platform should read the
capability off the world rather than trust a boolean.

The worlds can be declared but not yet implemented: manifest.get-identity()
returns a kind enum with no addon variant, and adding one is a breaking change
to extension-base. The file says so, so a reader does not take it for an
oversight.

Parsed by a test rather than trusted: nothing else in this repo parses wit/,
so a syntax error would have surfaced in a downstream repo at build time."
```

---

### Task 2: `assert_apply_consistent` — D11 as callable code

**Files:**
- Create: `crates/greentic-extension-sdk-testing/src/conformance.rs`
- Modify: `crates/greentic-extension-sdk-testing/src/lib.rs`
- Test: `crates/greentic-extension-sdk-testing/tests/apply_consistency.rs` (create)

**Interfaces:**
- Consumes: `serde_json` (already a dependency).
- Produces:
  - `conformance::Inconsistency { path: String, planned: serde_json::Value, observed: Option<serde_json::Value> }`
  - `conformance::assert_apply_consistent(planned_json: &str, observed_json: &str) -> Result<(), Vec<Inconsistency>>`
  - Re-exported from the crate root as `pub use self::conformance::{assert_apply_consistent, Inconsistency};`

**Why this is the valuable half of the plan.** Spec D11 says the host fails an apply whose result disagrees with the plan it approved. Today that is host behaviour an addon author cannot check until their addon is deployed. Shipping it as a function means the author runs the *same rule* the platform runs — not a test that resembles it.

The rule: every leaf in `planned` must be present at the same JSON Pointer in `observed`, with an equal value. Extra keys in `observed` are fine — an addon may report more than it planned, and often will.

- [ ] **Step 1: Write the failing test**

Create `crates/greentic-extension-sdk-testing/tests/apply_consistency.rs`:

```rust
//! Spec D11: a plan the apply does not honour is a dry run, not a contract.
//! The platform enforces this; shipping it as a function lets an addon author
//! run the same rule before deploying.

use greentic_extension_sdk_testing::assert_apply_consistent;

#[test]
fn an_apply_that_reached_the_planned_state_is_consistent() {
    let planned = r#"{"collections":[{"name":"docs","size":768}]}"#;
    let observed = r#"{"collections":[{"name":"docs","size":768}]}"#;
    assert!(assert_apply_consistent(planned, observed).is_ok());
}

/// The addon may report more than it planned — a server-assigned id, a
/// timestamp. Extra keys are not a violation.
#[test]
fn extra_keys_in_the_observed_state_are_allowed() {
    let planned = r#"{"collections":[{"name":"docs"}]}"#;
    let observed = r#"{"collections":[{"name":"docs","uuid":"abc","created_at":"now"}]}"#;
    assert!(assert_apply_consistent(planned, observed).is_ok());
}

#[test]
fn a_changed_leaf_is_reported_with_its_path() {
    let planned = r#"{"collections":[{"name":"docs","size":768}]}"#;
    let observed = r#"{"collections":[{"name":"docs","size":1536}]}"#;
    let errs = assert_apply_consistent(planned, observed)
        .expect_err("a changed leaf must be inconsistent");
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].path, "/collections/0/size");
    assert_eq!(errs[0].planned, serde_json::json!(768));
    assert_eq!(errs[0].observed, Some(serde_json::json!(1536)));
}

#[test]
fn a_missing_key_is_reported_with_observed_none() {
    let planned = r#"{"collections":[{"name":"docs","size":768}]}"#;
    let observed = r#"{"collections":[{"name":"docs"}]}"#;
    let errs = assert_apply_consistent(planned, observed).expect_err("a dropped key is a defect");
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].path, "/collections/0/size");
    assert_eq!(errs[0].observed, None);
}

/// A shorter array means the apply did not create everything it planned.
#[test]
fn a_missing_array_element_is_reported() {
    let planned = r#"{"collections":[{"name":"a"},{"name":"b"}]}"#;
    let observed = r#"{"collections":[{"name":"a"}]}"#;
    let errs = assert_apply_consistent(planned, observed).expect_err("a dropped element is a defect");
    assert_eq!(errs[0].path, "/collections/1/name");
}

/// Every violation is reported, not just the first: an author fixing one at a
/// time learns the shape of the problem slowly and expensively.
#[test]
fn every_violation_is_reported_not_just_the_first() {
    let planned = r#"{"a":1,"b":2,"c":3}"#;
    let observed = r#"{"a":9,"b":8,"c":3}"#;
    let errs = assert_apply_consistent(planned, observed).expect_err("two leaves differ");
    assert_eq!(errs.len(), 2);
    let paths: Vec<&str> = errs.iter().map(|e| e.path.as_str()).collect();
    assert!(paths.contains(&"/a") && paths.contains(&"/b"), "got: {paths:?}");
}

#[test]
fn unparseable_input_is_an_error_not_a_pass() {
    let errs = assert_apply_consistent("{not json", r#"{}"#)
        .expect_err("unparseable planned state must not silently pass");
    assert_eq!(errs.len(), 1);
    assert!(errs[0].path.is_empty(), "a parse failure has no path: {:?}", errs[0]);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-testing --test apply_consistency`
Expected: FAIL — `unresolved import ... assert_apply_consistent`.

- [ ] **Step 3: Write the implementation**

Create `crates/greentic-extension-sdk-testing/src/conformance.rs`:

```rust
//! Conformance checks an addon can run before deploying.
//!
//! The rule in [`assert_apply_consistent`] is the one the platform enforces in
//! production. It lives here so an author runs the same rule rather than a
//! test that resembles it.

use serde_json::Value;

/// One leaf where the applied state disagrees with the plan that was approved.
#[derive(Debug, Clone, PartialEq)]
pub struct Inconsistency {
    /// JSON Pointer to the offending leaf. Empty when the input did not parse.
    pub path: String,
    /// What the plan said.
    pub planned: Value,
    /// What the apply produced. `None` when the key is absent entirely.
    pub observed: Option<Value>,
}

/// Assert that an apply honoured the plan it was given.
///
/// Every leaf in `planned_json` must appear at the same JSON Pointer in
/// `observed_json` with an equal value. Extra keys in the observed state are
/// allowed: an addon may report a server-assigned id or a timestamp it could
/// not have planned.
///
/// Returns every violation rather than the first, because an author fixing
/// them one at a time learns the shape of the problem slowly.
///
/// # Errors
///
/// Returns the list of disagreeing leaves. A single entry with an empty `path`
/// means one of the two inputs did not parse as JSON — reported as a failure
/// rather than a pass, since silently accepting unparseable state is how a
/// check stops checking.
pub fn assert_apply_consistent(
    planned_json: &str,
    observed_json: &str,
) -> Result<(), Vec<Inconsistency>> {
    let planned: Value = serde_json::from_str(planned_json).map_err(|e| {
        vec![Inconsistency {
            path: String::new(),
            planned: Value::String(format!("planned state is not valid JSON: {e}")),
            observed: None,
        }]
    })?;
    let observed: Value = serde_json::from_str(observed_json).map_err(|e| {
        vec![Inconsistency {
            path: String::new(),
            planned: Value::String(format!("observed state is not valid JSON: {e}")),
            observed: None,
        }]
    })?;

    let mut out = Vec::new();
    walk(&planned, Some(&observed), String::new(), &mut out);
    if out.is_empty() { Ok(()) } else { Err(out) }
}

/// Descend `planned`, comparing each leaf against the same position in
/// `observed`. Containers are walked; only leaves are compared, so an object
/// gaining a key is not itself a violation.
fn walk(planned: &Value, observed: Option<&Value>, path: String, out: &mut Vec<Inconsistency>) {
    match planned {
        Value::Object(map) => {
            for (key, child) in map {
                let child_path = format!("{path}/{key}");
                let child_observed = observed.and_then(|o| o.get(key));
                walk(child, child_observed, child_path, out);
            }
        }
        Value::Array(items) => {
            for (i, child) in items.iter().enumerate() {
                let child_path = format!("{path}/{i}");
                let child_observed = observed.and_then(|o| o.get(i));
                walk(child, child_observed, child_path, out);
            }
        }
        leaf => {
            if observed != Some(leaf) {
                out.push(Inconsistency {
                    path,
                    planned: leaf.clone(),
                    observed: observed.cloned(),
                });
            }
        }
    }
}
```

In `crates/greentic-extension-sdk-testing/src/lib.rs`, add the module and the re-export, keeping the existing alphabetical grouping:

```rust
mod conformance;
```

```rust
pub use self::conformance::{Inconsistency, assert_apply_consistent};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p greentic-extension-sdk-testing`
Expected: PASS, all seven tests plus the crate's existing ones.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all
git add crates/greentic-extension-sdk-testing/src/conformance.rs \
        crates/greentic-extension-sdk-testing/src/lib.rs \
        crates/greentic-extension-sdk-testing/tests/apply_consistency.rs
git commit -m "feat(testing): ship the plan/apply consistency rule as a function

Spec D11 says the host fails an apply whose result disagrees with the plan it
approved. That was host behaviour an addon author could not check until their
addon was deployed. It is now a function the author calls — the same rule, not
a test that resembles it.

Extra keys in the observed state are allowed: an addon may report a
server-assigned id it could not have planned. Every violation is returned
rather than the first, and unparseable input is a failure rather than a pass."
```

---

### Task 3: Plan idempotency and stability, as closure-taking helpers

**Files:**
- Modify: `crates/greentic-extension-sdk-testing/src/conformance.rs`
- Modify: `crates/greentic-extension-sdk-testing/src/lib.rs`
- Test: `crates/greentic-extension-sdk-testing/tests/plan_properties.rs` (create)

**Interfaces:**
- Consumes: `Inconsistency` and `assert_apply_consistent` from Task 2.
- Produces:
  - `conformance::PlanResult { planned_json: String, requires_replace: Vec<String> }`
  - `conformance::assert_plan_idempotent(current: &str, plan: impl Fn(&str, &str) -> Option<PlanResult>) -> Result<(), String>`
  - `conformance::assert_plan_stable(current: &str, desired: &str, plan: impl Fn(&str, &str) -> Option<PlanResult>) -> Result<(), String>`
  - Re-exported from the crate root alongside Task 2's items.

**Why closures rather than a trait.** The WIT-generated `plan-outcome` type lives in the addon's own crate, produced by `cargo component`. This crate cannot name it and must not try. A closure returning a plain `PlanResult` — `None` standing for `deferred` — lets an addon adapt its own bindgen types in one line and keeps this crate free of any generated type.

- [ ] **Step 1: Write the failing test**

Create `crates/greentic-extension-sdk-testing/tests/plan_properties.rs`:

```rust
//! `plan(x, x)` returning anything but `x` unchanged means the addon is not
//! idempotent, which means it never converges. One property, checked
//! mechanically, with no infrastructure.

use greentic_extension_sdk_testing::{PlanResult, assert_plan_idempotent, assert_plan_stable};

/// A well-behaved addon: planning current against itself is a no-op.
fn good_plan(_current: &str, desired: &str) -> Option<PlanResult> {
    Some(PlanResult {
        planned_json: desired.to_string(),
        requires_replace: Vec::new(),
    })
}

#[test]
fn an_idempotent_plan_passes() {
    let current = r#"{"collections":[{"name":"docs"}]}"#;
    assert!(assert_plan_idempotent(current, good_plan).is_ok());
}

/// The failure this property exists to catch: an addon that always proposes a
/// change, so every reconcile has work to do and the resource never settles.
#[test]
fn a_plan_that_always_proposes_a_change_fails() {
    let churning = |_c: &str, _d: &str| {
        Some(PlanResult {
            planned_json: r#"{"collections":[{"name":"docs","touched":true}]}"#.to_string(),
            requires_replace: Vec::new(),
        })
    };
    let err = assert_plan_idempotent(r#"{"collections":[{"name":"docs"}]}"#, churning)
        .expect_err("a churning plan must fail");
    assert!(err.contains("touched"), "the message should show the diff: {err}");
}

/// `requires-replace` on a no-op plan means the addon would destroy and
/// recreate a resource that already matches.
#[test]
fn a_no_op_plan_that_requires_replace_fails() {
    let destructive = |_c: &str, d: &str| {
        Some(PlanResult {
            planned_json: d.to_string(),
            requires_replace: vec!["/collections/0".to_string()],
        })
    };
    let err = assert_plan_idempotent(r#"{"collections":[{"name":"docs"}]}"#, destructive)
        .expect_err("a no-op plan must not require replacement");
    assert!(err.contains("requires-replace"), "got: {err}");
}

/// `deferred` is a legitimate answer, but not to `plan(x, x)`: nothing is
/// missing when current and desired already agree.
#[test]
fn deferring_an_identity_plan_fails() {
    let deferring = |_c: &str, _d: &str| None;
    let err = assert_plan_idempotent(r#"{"a":1}"#, deferring)
        .expect_err("deferring an identity plan must fail");
    assert!(err.contains("deferred"), "got: {err}");
}

#[test]
fn a_stable_plan_passes() {
    assert!(assert_plan_stable(r#"{"a":1}"#, r#"{"a":2}"#, good_plan).is_ok());
}

/// A plan that varies between identical calls cannot be approved: what the
/// user saw is not what the apply will do.
#[test]
fn a_plan_that_varies_between_calls_fails() {
    let counter = std::cell::Cell::new(0);
    let unstable = |_c: &str, _d: &str| {
        let n = counter.get();
        counter.set(n + 1);
        Some(PlanResult {
            planned_json: format!(r#"{{"call":{n}}}"#),
            requires_replace: Vec::new(),
        })
    };
    let err = assert_plan_stable(r#"{"a":1}"#, r#"{"a":2}"#, unstable)
        .expect_err("an unstable plan must fail");
    assert!(err.contains("differed"), "got: {err}");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-testing --test plan_properties`
Expected: FAIL — `unresolved import ... PlanResult`.

- [ ] **Step 3: Write the implementation**

Append to `crates/greentic-extension-sdk-testing/src/conformance.rs`:

```rust
/// What an addon's `plan` produced, flattened for testing.
///
/// The WIT `plan-outcome` variant lives in the addon's own crate, generated by
/// `cargo component`; this crate cannot name it. An addon adapts its bindgen
/// type into this in one line, and `None` from the closure stands for
/// `deferred`.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanResult {
    pub planned_json: String,
    pub requires_replace: Vec<String>,
}

/// Assert that planning a state against itself is a no-op.
///
/// This is the single most valuable property an addon can be held to. An
/// addon that fails it never converges: every reconcile finds work, the
/// resource never settles, and the symptom — a plan that is never clean —
/// looks like a platform fault rather than an addon defect.
///
/// # Errors
///
/// Returns a human-readable description of the first property violated:
/// a deferred outcome, a non-empty `requires_replace`, or a planned state
/// that differs from the input.
pub fn assert_plan_idempotent(
    current: &str,
    plan: impl Fn(&str, &str) -> Option<PlanResult>,
) -> Result<(), String> {
    let Some(result) = plan(current, current) else {
        return Err(
            "plan(x, x) returned deferred; nothing is missing when current and desired agree"
                .to_string(),
        );
    };

    if !result.requires_replace.is_empty() {
        return Err(format!(
            "plan(x, x) returned requires-replace {:?}; a state that already matches must not be \
             destroyed and recreated",
            result.requires_replace
        ));
    }

    // Both directions, deliberately. `assert_apply_consistent` checks a
    // SUBSET — every leaf of its first argument present in its second — which
    // is right for apply (an addon may report more than it planned) and wrong
    // here. Idempotency needs equality: running it only as
    // `(current, planned)` would pass an addon that ADDS a field on every
    // plan, which is the churning case this property exists to catch.
    let removed = assert_apply_consistent(current, &result.planned_json).err();
    let added = assert_apply_consistent(&result.planned_json, current).err();

    let mut shown: Vec<String> = Vec::new();
    for d in removed.into_iter().flatten() {
        shown.push(format!("{} dropped (planned {})", d.path, d.planned));
    }
    for d in added.into_iter().flatten() {
        shown.push(format!("{} added (plan says {})", d.path, d.planned));
    }

    if shown.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "plan(x, x) did not return x unchanged, so this addon never converges: {}",
            shown.join("; ")
        ))
    }
}

/// Assert that planning the same inputs twice produces the same output.
///
/// A plan that varies cannot be approved: what the user saw in the plan is not
/// what the apply will do.
///
/// # Errors
///
/// Returns a description of how the two calls differed.
pub fn assert_plan_stable(
    current: &str,
    desired: &str,
    plan: impl Fn(&str, &str) -> Option<PlanResult>,
) -> Result<(), String> {
    let first = plan(current, desired);
    let second = plan(current, desired);
    if first == second {
        Ok(())
    } else {
        Err(format!(
            "two identical plan calls differed:\n  first:  {first:?}\n  second: {second:?}"
        ))
    }
}
```

Extend the re-export in `crates/greentic-extension-sdk-testing/src/lib.rs`:

```rust
pub use self::conformance::{
    Inconsistency, PlanResult, assert_apply_consistent, assert_plan_idempotent, assert_plan_stable,
};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p greentic-extension-sdk-testing`
Expected: PASS, all six new tests plus everything from Task 2 and the crate's existing suite.

- [ ] **Step 5: Run the full gate**

Run: `./ci/local_check.sh`
Expected: all six steps pass. If a `cargo publish --dry-run` step fails for a network or registry reason rather than a code reason, say so explicitly — do not report a network error as a passing gate, and do not report it as a code defect.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all
git add crates/greentic-extension-sdk-testing/src/conformance.rs \
        crates/greentic-extension-sdk-testing/src/lib.rs \
        crates/greentic-extension-sdk-testing/tests/plan_properties.rs
git commit -m "feat(testing): plan idempotency and stability as closure-taking properties

plan(x, x) returning anything but x unchanged means the addon never
converges — every reconcile finds work and the resource never settles, and
the symptom looks like a platform fault rather than an addon defect.

Closures rather than a trait: the WIT plan-outcome type is generated in the
addon's own crate and this crate cannot name it. An addon adapts its bindgen
type in one line, and None stands for deferred."
```

---

## Out of scope, and why

**`ExtensionKind::Addon`, the scaffold template, the install/lint plumbing.** Blocked on `extension-base@0.3.0`: adding `addon` to the WIT `enum kind` is breaking, and the runtime must serve `manifest@0.2.0` and `@0.3.0` concurrently during migration. Cross-repo coordination, planned as a contract release (spec §9.2).

**`E_ADDON_IMAGE_NOT_PINNED` and `E_ADDON_BACKUP_MISMATCH`.** Both inspect a world no artifact can declare yet. A rule nothing can trigger passes its own tests and does nothing in production — the defect the previous plan spent a fix wave removing from `W_DESCRIBE_DIFF_BREAKING`. They belong with the kind.

**The `managed` workload path, renderers, the environment model.** Commercial platform repo (spec §4, §6, §7).

**A worked first-party addon.** Spec §9.3 names Redis as the first to build and says why — it is the only candidate that both has real prior art and stresses the socket-import decision. It is also the test of §5.5's claim that D6 keeps the simple case cheap. That is a separate piece of work, and it needs the kind.
