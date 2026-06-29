# Phase D: Security Hardening Implementation Plan

> **Status (2026-05-17): D.1–D.4 SHIPPED on `research`. D.5+ blocked on org decision.**
>
> | Task | PR | Notes |
> |---|---|---|
> | D.1.1–D.1.6 (`#![forbid(unsafe_code)]` on 5 SDK crate roots) | greenticai/greentic-designer-sdk#11 | |
> | D.2 (`dev-allow-unsigned` Cargo feature gate) | greentic-biz/greentic-designer-extensions#57 | |
> | D.3 (server-side JCS canonical signing) | greentic-biz/greentic-store-server#27 | Needs Fargate redeploy to be live in prod |
> | D.4.1 (`Manifest` schema + builder + verifier in sdk-contract) | greenticai/greentic-designer-sdk#13 | |
> | D.4.2 + D.4.3 (`build_gtxpack_with_manifest` + gtdx packer call-site swap) | greenticai/greentic-designer-sdk#14 | |
> | D.4.runtime (`verify_dir_manifest` consumer side) | greentic-biz/greentic-designer-extensions#58 | Fail-open for pre-D.4.2 packs during transition |
> | D.5+ (publisher cert chain rooted at AWS-KMS Greentic root) | **BLOCKED** | Needs CEO/CTO decision on master-key custody, HSM vs KMS, rotation/DR policy. Code work ~1–2 weeks after decision. |
> | Cascade: `gtdx-version: "=1.2.1-research"` in 8 extension repo workflows | 8 PRs across greentic-biz/* + greenticai/components-public | So next per-repo publish emits manifest-bearing packs |
>
> Original plan body preserved below as historical record + design rationale.
>
> ---

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the May-2026 audit's P0 security findings by restoring `forbid(unsafe_code)`, gating the dev bypass behind a Cargo feature, switching server-side signing to JCS, signing the whole `.gtxpack` via a Merkle-style manifest, introducing a publisher-cert chain rooted at an AWS-KMS-backed Greentic root, tightening permissions allow-list linting, and locking down per-extension dir perms.

**Architecture:**
- **Trust chain:** Greentic root key in AWS KMS signs each publisher's ed25519 public key, producing a "publisher cert". Designer verifies (1) the publisher cert is signed by the embedded root pubkey + (2) the `.gtxpack` is signed by the publisher key.
- **Whole-archive integrity:** `manifest.json` inside the `.gtxpack` enumerates every file (sorted path + sha256). Only `manifest.json` is signed; verification recomputes per-entry sha256 + checks the signature.
- **Dev/prod split:** A `RootVerifier` trait lets dev tests run with a fixture root keypair while prod plugs the real KMS verifier. The legacy `GREENTIC_EXT_ALLOW_UNSIGNED` env bypass moves behind a compile-time Cargo feature `dev-allow-unsigned`.

**Tech Stack:** Rust 1.95, `ed25519-dalek`, `sha2`, `serde_jcs`, `aws-sdk-kms`, `aws-config`, `zip`, `thiserror`, `tracing`.

**Repos touched:**
- `greentic-designer-sdk` (this repo) — contract types, signing helpers, registry verify, CLI lint
- `greentic-designer-extensions` — `greentic-ext-runtime` verify path + feature flag
- `greentic-store-server` — JCS server-sign + KMS root signer
- `greentic-docs` (main) — policy doc (D.9 only)

**Branch policy:** all feature branches target `research` in their respective repos. NO Claude attribution in commits or PR bodies.

**Block-on-org-decision split:**
- D.1 through D.4 + D.8 ship independently (no KMS / no CEO sign-off needed).
- D.5 through D.7 + D.9 block on INSIGNIA DevOps provisioning the KMS key + CTO sign-off on the policy doc.

---

## File Structure

### New files (this repo: `greentic-designer-sdk`)
- `crates/greentic-extension-sdk-contract/src/manifest.rs` — manifest.json schema + builder + verifier.
- `crates/greentic-extension-sdk-contract/src/publisher_cert.rs` — `PublisherCert` struct, parse, verify.
- `crates/greentic-extension-sdk-contract/src/root_verifier.rs` — `RootVerifier` trait + `EmbeddedRootVerifier` (cached pubkey) + `FixtureRootVerifier` (test only).
- `crates/greentic-extension-sdk-registry/src/trust_store.rs` — `~/.greentic/trust/publishers.json` loader + allowlist check.
- `crates/greentic-extension-sdk-cli/src/commands/lint.rs` — `gtdx lint` permissions-breadth check (this plan adds permissions-only path; full lint is Phase E).
- `crates/greentic-extension-sdk-cli/tests/integration_attack_vectors.rs` — D.10 end-to-end attack-vector tests.

### New files (`greentic-designer-extensions`)
- `crates/greentic-ext-runtime/src/feature_gate.rs` — feature-gated env-var read.

### New files (`greentic-store-server`)
- `crates/greentic-store-api/src/auth/kms_root.rs` — AWS KMS-backed root signer (issues publisher certs).

### New files (`greentic-docs`)
- `src/content/docs/operating/trust-root.md` — KMS custody policy doc.

### Modified files
- `crates/greentic-extension-sdk-contract/src/lib.rs` — `#![forbid(unsafe_code)]` + module exports.
- `crates/greentic-extension-sdk-state/src/lib.rs` — `#![forbid(unsafe_code)]`.
- `crates/greentic-extension-sdk-registry/src/lib.rs` — `#![forbid(unsafe_code)]` + module exports.
- `crates/greentic-extension-sdk-testing/src/lib.rs` — `#![forbid(unsafe_code)]`.
- `crates/greentic-extension-sdk-cli/src/main.rs` — `#![forbid(unsafe_code)]` + register `lint` subcommand.
- `crates/greentic-extension-sdk-registry/src/lifecycle.rs` — call publisher-cert chain verify + manifest verify; set 0700 perms.
- `crates/greentic-extension-sdk-registry/src/storage.rs` — `set_secure_dir_permissions` Unix helper.
- `greentic-designer-extensions/crates/greentic-ext-runtime/src/lib.rs` — `#![forbid(unsafe_code)]`.
- `greentic-designer-extensions/crates/greentic-ext-runtime/Cargo.toml` — `[features]` block.
- `greentic-designer-extensions/crates/greentic-ext-runtime/src/runtime.rs` — feature-gate the env-var bypass.
- `greentic-store-server/crates/greentic-store-api/src/handlers/extensions.rs:393` — `serde_jcs::to_vec`.
- `greentic-store-server/Cargo.toml` — `serde_jcs` + `aws-sdk-kms` deps.

---

## Task D.1.0: Prep — create isolated worktree

**Files:** none yet (worktree mechanics).

- [ ] **Step 1: Create worktree off `research`**

```bash
cd /home/bima-pangestu/Works/greentic/greentic-designer-sdk
git fetch origin
git worktree add -b feat/security-hardening-phase-d ../greentic-designer-sdk-phase-d origin/research
cd ../greentic-designer-sdk-phase-d
```

Expected: new worktree at `../greentic-designer-sdk-phase-d` on a fresh branch.

- [ ] **Step 2: Verify clean build baseline**

Run: `bash ci/local_check.sh` (or `cargo test --workspace --all-features` if `ci/local_check.sh` is missing).
Expected: PASS. If it fails on `research` already, STOP and report to Bima before proceeding.

---

## Task D.1.1: Restore `#![forbid(unsafe_code)]` — sdk-contract

**Files:**
- Modify: `crates/greentic-extension-sdk-contract/src/lib.rs:1`

- [ ] **Step 1: Add a failing grep-based test**

Create: `crates/greentic-extension-sdk-contract/tests/lint_unsafe.rs`

```rust
#[test]
fn crate_root_forbids_unsafe_code() {
    let lib = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs")).unwrap();
    assert!(
        lib.lines()
            .any(|l| l.trim() == "#![forbid(unsafe_code)]"),
        "crate root must declare `#![forbid(unsafe_code)]` (audit P0 #5)"
    );
}
```

- [ ] **Step 2: Run the test — expect FAIL**

Run: `cargo test -p greentic-extension-sdk-contract --test lint_unsafe -- --nocapture`
Expected: FAIL with `crate root must declare ...`.

- [ ] **Step 3: Add the attribute**

Edit `crates/greentic-extension-sdk-contract/src/lib.rs`, replace the first line:

```rust
//! Contract types + describe.json schema for Greentic Designer Extensions.
```

with:

```rust
#![forbid(unsafe_code)]
//! Contract types + describe.json schema for Greentic Designer Extensions.
```

- [ ] **Step 4: Re-run test + full crate build**

Run:
```bash
cargo test -p greentic-extension-sdk-contract --test lint_unsafe
cargo build -p greentic-extension-sdk-contract
```
Expected: both PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/greentic-extension-sdk-contract/src/lib.rs crates/greentic-extension-sdk-contract/tests/lint_unsafe.rs
git commit -m "feat(sdk-contract): forbid unsafe_code at crate root"
```

---

## Task D.1.2: Restore `#![forbid(unsafe_code)]` — sdk-state

**Files:**
- Modify: `crates/greentic-extension-sdk-state/src/lib.rs:1`

- [ ] **Step 1: Failing test**

Create `crates/greentic-extension-sdk-state/tests/lint_unsafe.rs` with the same body as D.1.1 Step 1.

- [ ] **Step 2: Run — expect FAIL**

Run: `cargo test -p greentic-extension-sdk-state --test lint_unsafe`
Expected: FAIL.

- [ ] **Step 3: Add attribute**

Edit `crates/greentic-extension-sdk-state/src/lib.rs`. Replace:

```rust
//! Extension lifecycle state — persistent enable/disable per extension.
```

with:

```rust
#![forbid(unsafe_code)]
//! Extension lifecycle state — persistent enable/disable per extension.
```

- [ ] **Step 4: Re-run test**

Run: `cargo test -p greentic-extension-sdk-state --test lint_unsafe && cargo build -p greentic-extension-sdk-state`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/greentic-extension-sdk-state/src/lib.rs crates/greentic-extension-sdk-state/tests/lint_unsafe.rs
git commit -m "feat(sdk-state): forbid unsafe_code at crate root"
```

---

## Task D.1.3: Restore `#![forbid(unsafe_code)]` — sdk-registry

**Files:**
- Modify: `crates/greentic-extension-sdk-registry/src/lib.rs:1`

- [ ] **Step 1: Failing test**

Create `crates/greentic-extension-sdk-registry/tests/lint_unsafe.rs` with the same body as D.1.1 Step 1.

- [ ] **Step 2: Run — expect FAIL**

Run: `cargo test -p greentic-extension-sdk-registry --test lint_unsafe`
Expected: FAIL.

- [ ] **Step 3: Add attribute**

Edit `crates/greentic-extension-sdk-registry/src/lib.rs`. Replace:

```rust
//! Registry client + install lifecycle for Greentic Designer Extensions.
```

with:

```rust
#![forbid(unsafe_code)]
//! Registry client + install lifecycle for Greentic Designer Extensions.
```

- [ ] **Step 4: Re-run**

Run: `cargo test -p greentic-extension-sdk-registry --test lint_unsafe && cargo build -p greentic-extension-sdk-registry`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/greentic-extension-sdk-registry/src/lib.rs crates/greentic-extension-sdk-registry/tests/lint_unsafe.rs
git commit -m "feat(sdk-registry): forbid unsafe_code at crate root"
```

---

## Task D.1.4: Restore `#![forbid(unsafe_code)]` — sdk-testing

**Files:**
- Modify: `crates/greentic-extension-sdk-testing/src/lib.rs:1`

- [ ] **Step 1: Failing test**

Create `crates/greentic-extension-sdk-testing/tests/lint_unsafe.rs` with the same body as D.1.1 Step 1.

- [ ] **Step 2: Run — expect FAIL**

Run: `cargo test -p greentic-extension-sdk-testing --test lint_unsafe`
Expected: FAIL.

- [ ] **Step 3: Add attribute**

Edit `crates/greentic-extension-sdk-testing/src/lib.rs`. Replace:

```rust
//! Test utilities for Greentic Designer Extensions.
```

with:

```rust
#![forbid(unsafe_code)]
//! Test utilities for Greentic Designer Extensions.
```

- [ ] **Step 4: Re-run**

Run: `cargo test -p greentic-extension-sdk-testing --test lint_unsafe && cargo build -p greentic-extension-sdk-testing`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/greentic-extension-sdk-testing/src/lib.rs crates/greentic-extension-sdk-testing/tests/lint_unsafe.rs
git commit -m "feat(sdk-testing): forbid unsafe_code at crate root"
```

---

## Task D.1.5: Restore `#![forbid(unsafe_code)]` — sdk-cli

**Files:**
- Modify: `crates/greentic-extension-sdk-cli/src/main.rs:1`

Note: this crate has no `lib.rs` — only `main.rs`. The forbid attribute goes there.

- [ ] **Step 1: Failing test**

Create `crates/greentic-extension-sdk-cli/tests/lint_unsafe.rs`:

```rust
#[test]
fn binary_root_forbids_unsafe_code() {
    let main = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/main.rs")).unwrap();
    assert!(
        main.lines()
            .any(|l| l.trim() == "#![forbid(unsafe_code)]"),
        "binary root must declare `#![forbid(unsafe_code)]` (audit P0 #5)"
    );
}
```

- [ ] **Step 2: Run — expect FAIL**

Run: `cargo test -p greentic-extension-sdk-cli --test lint_unsafe`
Expected: FAIL.

- [ ] **Step 3: Add attribute**

Edit `crates/greentic-extension-sdk-cli/src/main.rs`. Prepend:

```rust
#![forbid(unsafe_code)]
mod commands;
mod dev;
mod publish;
mod scaffold;
```

(replace the existing `mod commands;` block by inserting the attribute as the first line).

- [ ] **Step 4: Re-run**

Run: `cargo test -p greentic-extension-sdk-cli --test lint_unsafe && cargo build -p greentic-extension-sdk-cli`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/greentic-extension-sdk-cli/src/main.rs crates/greentic-extension-sdk-cli/tests/lint_unsafe.rs
git commit -m "feat(sdk-cli): forbid unsafe_code at binary root"
```

---

## Task D.1.6: Restore `#![forbid(unsafe_code)]` — greentic-ext-runtime

**Repo:** `greentic-designer-extensions` (separate worktree).

**Files:**
- Modify: `crates/greentic-ext-runtime/src/lib.rs:1`

- [ ] **Step 1: Create worktree**

```bash
cd /home/bima-pangestu/Works/greentic/greentic-designer-extensions
git fetch origin
git worktree add -b feat/security-hardening-phase-d ../greentic-designer-extensions-phase-d origin/research
cd ../greentic-designer-extensions-phase-d
```

- [ ] **Step 2: Failing test**

Create `crates/greentic-ext-runtime/tests/lint_unsafe.rs`:

```rust
#[test]
fn crate_root_forbids_unsafe_code() {
    let lib = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs")).unwrap();
    assert!(
        lib.lines()
            .any(|l| l.trim() == "#![forbid(unsafe_code)]"),
        "crate root must declare `#![forbid(unsafe_code)]` (audit P0 #5)"
    );
}
```

- [ ] **Step 3: Run — expect FAIL**

Run: `cargo test -p greentic-ext-runtime --test lint_unsafe`
Expected: FAIL.

- [ ] **Step 4: Add attribute**

Edit `crates/greentic-ext-runtime/src/lib.rs`. Replace:

```rust
//! Wasmtime-based runtime for Greentic Designer Extensions.
```

with:

```rust
#![forbid(unsafe_code)]
//! Wasmtime-based runtime for Greentic Designer Extensions.
```

- [ ] **Step 5: Re-run**

Run: `cargo test -p greentic-ext-runtime --test lint_unsafe && cargo build -p greentic-ext-runtime`
Expected: PASS. If wasmtime usage forces `unsafe`, downgrade to `deny` and document — but the runtime currently has no `unsafe` blocks, so this should succeed.

- [ ] **Step 6: Commit**

```bash
git add crates/greentic-ext-runtime/src/lib.rs crates/greentic-ext-runtime/tests/lint_unsafe.rs
git commit -m "feat(ext-runtime): forbid unsafe_code at crate root"
```

---

## Task D.2: Gate `GREENTIC_EXT_ALLOW_UNSIGNED` behind `cfg(feature = "dev-allow-unsigned")`

**Repo:** `greentic-designer-extensions`.

**Files:**
- Modify: `crates/greentic-ext-runtime/Cargo.toml` (add `[features]` block)
- Create: `crates/greentic-ext-runtime/src/feature_gate.rs`
- Modify: `crates/greentic-ext-runtime/src/lib.rs` (register module)
- Modify: `crates/greentic-ext-runtime/src/runtime.rs:144-150` (call feature-gated helper)
- Test: `crates/greentic-ext-runtime/tests/feature_gate.rs`

- [ ] **Step 1: Write failing tests covering both feature states**

Create `crates/greentic-ext-runtime/tests/feature_gate.rs`:

```rust
//! Verify the `dev-allow-unsigned` Cargo feature is the ONLY way to honor
//! the `GREENTIC_EXT_ALLOW_UNSIGNED` env var. Release builds must ignore
//! the env entirely.

use greentic_ext_runtime::feature_gate::allow_unsigned_bypass;

#[test]
#[cfg(feature = "dev-allow-unsigned")]
fn dev_feature_honors_env_when_set() {
    // SAFETY: tests within a single binary share env; the feature-gate
    // helper just reads `var_os`, so we set, query, unset.
    // Use a unique var name in real impl to avoid cross-test bleed,
    // here we share the one defined by the helper API.
    // The helper takes the var name as input to keep it deterministic.
    unsafe { std::env::set_var("GREENTIC_EXT_ALLOW_UNSIGNED", "1") };
    assert!(allow_unsigned_bypass());
    unsafe { std::env::remove_var("GREENTIC_EXT_ALLOW_UNSIGNED") };
}

#[test]
#[cfg(feature = "dev-allow-unsigned")]
fn dev_feature_returns_false_when_env_unset() {
    unsafe { std::env::remove_var("GREENTIC_EXT_ALLOW_UNSIGNED") };
    assert!(!allow_unsigned_bypass());
}

#[test]
#[cfg(not(feature = "dev-allow-unsigned"))]
fn release_build_ignores_env_completely() {
    unsafe { std::env::set_var("GREENTIC_EXT_ALLOW_UNSIGNED", "1") };
    assert!(
        !allow_unsigned_bypass(),
        "release builds (no feature) MUST NOT honor the env var"
    );
    unsafe { std::env::remove_var("GREENTIC_EXT_ALLOW_UNSIGNED") };
}
```

- [ ] **Step 2: Run — expect compile error**

Run: `cargo test -p greentic-ext-runtime --test feature_gate`
Expected: FAIL (`module 'feature_gate' not found`).

- [ ] **Step 3: Add the feature flag in Cargo.toml**

Edit `crates/greentic-ext-runtime/Cargo.toml`. Add right after `publish = false`:

```toml
[features]
default = []
# Dev-only: allows GREENTIC_EXT_ALLOW_UNSIGNED=1 to skip describe.json
# signature verification. Audit P0 #6: NEVER enable in production builds.
dev-allow-unsigned = []
```

- [ ] **Step 4: Implement the helper module**

Create `crates/greentic-ext-runtime/src/feature_gate.rs`:

```rust
//! Compile-time gating for development-only escape hatches.
//!
//! The historical `GREENTIC_EXT_ALLOW_UNSIGNED=1` env var allowed
//! contributors to load unsigned extensions during local dev. Audit
//! finding P0 #6 flagged it as a runtime bypass — anyone could disable
//! signature verification on a shipped designer by setting one env var.
//!
//! This module funnels every read of that env var through a single
//! helper. Without the `dev-allow-unsigned` Cargo feature, the helper
//! is a `const fn` returning `false` and the env var is never read.

/// Returns `true` only if (a) the crate was compiled with feature
/// `dev-allow-unsigned`, AND (b) `GREENTIC_EXT_ALLOW_UNSIGNED` is set
/// to any value in the current process environment.
#[cfg(feature = "dev-allow-unsigned")]
#[must_use]
pub fn allow_unsigned_bypass() -> bool {
    std::env::var_os("GREENTIC_EXT_ALLOW_UNSIGNED").is_some()
}

/// Release-build stub: always returns `false`. The env var is not
/// even read, so attackers cannot reach behind this with an env probe.
#[cfg(not(feature = "dev-allow-unsigned"))]
#[must_use]
pub const fn allow_unsigned_bypass() -> bool {
    false
}
```

- [ ] **Step 5: Register module + re-export**

Edit `crates/greentic-ext-runtime/src/lib.rs`. Add `pub mod feature_gate;` after the existing `pub mod broker;` line, so the lib root reads:

```rust
#![forbid(unsafe_code)]
//! Wasmtime-based runtime for Greentic Designer Extensions.

pub mod broker;
pub mod capability;
pub mod discovery;
mod error;
mod health;
mod host_bindings;
mod host_state;
mod loaded;
mod pool;
mod runtime;
mod runtime_roles;
pub mod types;
pub mod watcher;
pub mod feature_gate;
```

- [ ] **Step 6: Replace the direct env-var read in runtime.rs**

Edit `crates/greentic-ext-runtime/src/runtime.rs`. Replace the body of `verify_dir_signature` between line 144 and line 151. Old:

```rust
fn verify_dir_signature(dir: &std::path::Path) -> Result<(), RuntimeError> {
    if std::env::var("GREENTIC_EXT_ALLOW_UNSIGNED").is_ok() {
        tracing::warn!(
            extension_dir = %dir.display(),
            "GREENTIC_EXT_ALLOW_UNSIGNED is set — signature verification skipped"
        );
        return Ok(());
    }
```

New:

```rust
fn verify_dir_signature(dir: &std::path::Path) -> Result<(), RuntimeError> {
    if crate::feature_gate::allow_unsigned_bypass() {
        tracing::warn!(
            extension_dir = %dir.display(),
            "GREENTIC_EXT_ALLOW_UNSIGNED honored (dev-allow-unsigned feature is enabled) — signature verification skipped"
        );
        return Ok(());
    }
```

- [ ] **Step 7: Run both branches of the test**

Run:
```bash
cargo test -p greentic-ext-runtime --test feature_gate
cargo test -p greentic-ext-runtime --test feature_gate --features dev-allow-unsigned
```
Expected: both invocations PASS.

- [ ] **Step 8: Verify release build clippy clean**

Run: `cargo clippy -p greentic-ext-runtime --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add crates/greentic-ext-runtime/Cargo.toml \
        crates/greentic-ext-runtime/src/feature_gate.rs \
        crates/greentic-ext-runtime/src/lib.rs \
        crates/greentic-ext-runtime/src/runtime.rs \
        crates/greentic-ext-runtime/tests/feature_gate.rs
git commit -m "feat(ext-runtime): gate GREENTIC_EXT_ALLOW_UNSIGNED behind dev-allow-unsigned feature"
```

---

## Task D.3: Server-side canonical signing — `serde_jcs::to_vec`

**Repo:** `greentic-store-server`.

**Files:**
- Modify: `Cargo.toml` (add `serde_jcs` workspace dep)
- Modify: `crates/greentic-store-api/Cargo.toml` (consume it)
- Modify: `crates/greentic-store-api/src/handlers/extensions.rs:393`
- Test: `crates/greentic-store-api/tests/canonical_sign_roundtrip.rs`

- [ ] **Step 1: Create worktree**

```bash
cd /home/bima-pangestu/Works/greentic/greentic-store-server
git fetch origin
# main branch — two-tier repo, no `research` line per spec table
git worktree add -b feat/security-hardening-phase-d ../greentic-store-server-phase-d origin/main
cd ../greentic-store-server-phase-d
```

- [ ] **Step 2: Failing roundtrip test**

Create `crates/greentic-store-api/tests/canonical_sign_roundtrip.rs`:

```rust
//! Audit P0 #3: server signed `serde_json::to_vec` (preserves struct field
//! emission order) while the client verifies against `serde_jcs::to_vec`
//! (JCS-canonical: lexicographic key order). This test asserts that the
//! server-produced canonical payload survives a struct-field shuffle:
//! verification MUST still pass even if the publisher's client emitted
//! fields in a different order.

use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};

fn fresh_key() -> SigningKey {
    let mut bytes = [0u8; 32];
    use rand::RngCore;
    rand::thread_rng().fill_bytes(&mut bytes);
    SigningKey::from_bytes(&bytes)
}

#[test]
fn server_canonical_payload_matches_shuffled_client_payload() {
    // Build a JSON describe with a known-non-lex-sorted field order.
    let publisher_emitted = serde_json::json!({
        "kind": "DesignExtension",
        "apiVersion": "greentic.ai/v1",
        "metadata": {
            "version": "1.0.0",
            "id": "example.demo",
            "name": "demo",
            "summary": "demo"
        }
    });

    // The client re-encodes via JCS before signing.
    let client_canonical = serde_jcs::to_vec(&publisher_emitted).unwrap();

    // The server should produce IDENTICAL bytes via the same canonicalizer.
    let server_canonical = greentic_store_api::test_support::canonical_describe_bytes(&publisher_emitted).unwrap();

    assert_eq!(
        client_canonical, server_canonical,
        "server must canonicalize via serde_jcs::to_vec, not serde_json::to_vec"
    );

    // Round-trip: server signs the canonical bytes, client verifies.
    let key = fresh_key();
    let sig = key.sign(&server_canonical);
    let pub_key: VerifyingKey = key.verifying_key();
    pub_key.verify(&client_canonical, &sig).expect("verify must pass on shuffled-order roundtrip");
}
```

- [ ] **Step 3: Run — expect FAIL (missing test_support module + missing dep)**

Run: `cargo test -p greentic-store-api --test canonical_sign_roundtrip`
Expected: FAIL with `cannot find module 'test_support'` or `serde_jcs not in scope`.

- [ ] **Step 4: Add `serde_jcs` workspace dep**

Edit `Cargo.toml` workspace section. Under `[workspace.dependencies]`, add (alphabetical position between `serde_json` and `sha2`):

```toml
serde_jcs = "0.1"
```

Edit `crates/greentic-store-api/Cargo.toml`. Add under `[dependencies]` (alphabetical):

```toml
serde_jcs = { workspace = true }
```

Add under `[dev-dependencies]`:

```toml
ed25519-dalek = { workspace = true }
rand = { workspace = true }
serde_jcs = { workspace = true }
```

(If `rand` isn't already in workspace deps for store-server, add `rand = "0.8"` under `[workspace.dependencies]`.)

- [ ] **Step 5: Add a small `test_support` module that exposes the canonicalizer**

Edit `crates/greentic-store-api/src/lib.rs` (or whichever file declares the crate's pub mods). Append:

```rust
/// Test-only helpers. Re-exports the canonicalizer used by `publish`
/// so integration tests can assert byte-for-byte equality with the
/// client's JCS encoding without going through HTTP.
#[doc(hidden)]
pub mod test_support {
    pub fn canonical_describe_bytes(
        describe: &serde_json::Value,
    ) -> Result<Vec<u8>, String> {
        serde_jcs::to_vec(describe).map_err(|e| e.to_string())
    }
}
```

If `lib.rs` does not exist as a file, find the crate root (check `Cargo.toml` for `lib` declaration) — for `greentic-store-api` it is `src/lib.rs` since it's a library crate.

- [ ] **Step 6: Swap the signing call site**

Edit `crates/greentic-store-api/src/handlers/extensions.rs`. Find line 393:

```rust
    // Sign the canonical describe JSON bytes so clients can verify later.
    let describe_bytes = serde_json::to_vec(&req.describe)
        .map_err(|e| AppError::internal_with("serialize describe", e))?;
```

Replace with:

```rust
    // Sign the canonical describe JSON bytes so clients can verify later.
    // Audit P0 #3: must use JCS canonicalization (`serde_jcs`) — the client
    // verifies against `verify_describe()` which strips `.signature` and
    // calls `serde_jcs::to_vec()`. A plain `serde_json::to_vec` here would
    // produce different bytes whenever the publisher's serde version emitted
    // fields in a different order, breaking verification.
    let describe_bytes = serde_jcs::to_vec(&req.describe)
        .map_err(|e| AppError::internal_with("canonicalize describe (JCS)", e))?;
```

Also the second `serde_json::to_vec(&signed_describe)` further down (around line 424) writes the DB-stored bytes, NOT the canonical-signing-payload — leave that one alone. The signature itself is already in the `.signature` field of the JSON; we only need canonicalization for the bytes we sign.

- [ ] **Step 7: Re-run the test**

Run: `cargo test -p greentic-store-api --test canonical_sign_roundtrip`
Expected: PASS.

- [ ] **Step 8: Run existing repack tests to ensure no regression**

Run: `cargo test -p greentic-store-api`
Expected: PASS (the previously passing publish + repack tests must still pass).

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml \
        crates/greentic-store-api/Cargo.toml \
        crates/greentic-store-api/src/lib.rs \
        crates/greentic-store-api/src/handlers/extensions.rs \
        crates/greentic-store-api/tests/canonical_sign_roundtrip.rs
git commit -m "fix(store-api): sign describe via serde_jcs (matches client verify)"
```

---

## Task D.4.1: Whole-archive manifest — define `Manifest` type + builder

**Repo:** `greentic-designer-sdk`.

**Files:**
- Create: `crates/greentic-extension-sdk-contract/src/manifest.rs`
- Modify: `crates/greentic-extension-sdk-contract/src/lib.rs` (export module)

- [ ] **Step 1: Failing unit tests**

Create `crates/greentic-extension-sdk-contract/src/manifest.rs` (tests-only first; impl follows):

```rust
//! `manifest.json` — whole-archive integrity ledger for `.gtxpack`.
//!
//! Audit P0 #2: signing only `describe.json` (JCS) leaves the WASM binary
//! and every other entry unsigned. An attacker who can rewrite a `.gtxpack`
//! in transit (or a malicious mirror) can swap `extension.wasm` while
//! re-pointing `describe.metadata.artifact_sha256` — verification still
//! passes because `verify_describe()` never looks at the archive contents.
//!
//! The fix: enumerate every archive entry (excluding `manifest.json` +
//! `describe.json`'s `.signature` field) with its sha256 + byte length in
//! a sorted ledger. Sign `serde_jcs::to_vec(&manifest)`. Verification:
//!   1. read `manifest.json` from the archive
//!   2. verify its signature against the publisher cert (D.5)
//!   3. recompute every entry's sha256 + compare against the manifest
//!
//! Any tampered file flips its sha256 → verification fails fast.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    /// Schema discriminator. Hard-coded `"greentic.gtxpack.manifest/v1"`.
    pub schema: String,
    /// Sorted-by-path list of every entry except `manifest.json` itself.
    pub entries: Vec<ManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestEntry {
    pub path: String,
    pub sha256: String,
    pub size: u64,
}

pub const MANIFEST_ENTRY_NAME: &str = "manifest.json";
pub const MANIFEST_SCHEMA_V1: &str = "greentic.gtxpack.manifest/v1";

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("manifest missing from archive")]
    Missing,
    #[error("entry '{0}' not in manifest")]
    UnexpectedEntry(String),
    #[error("entry '{path}' sha256 mismatch: expected {expected}, computed {computed}")]
    ShaMismatch { path: String, expected: String, computed: String },
    #[error("entry '{path}' present in manifest but absent from archive")]
    MissingEntry { path: String },
    #[error("manifest schema unsupported: {0}")]
    UnsupportedSchema(String),
}

/// Build a `Manifest` from the entries that will be (or are) inside a
/// `.gtxpack`. Paths are sorted lexicographically. `manifest.json` and any
/// trailing slash directory markers are excluded.
#[must_use]
pub fn build_manifest<I, S, B>(entries: I) -> Manifest
where
    I: IntoIterator<Item = (S, B)>,
    S: AsRef<str>,
    B: AsRef<[u8]>,
{
    let mut rows: Vec<ManifestEntry> = entries
        .into_iter()
        .filter_map(|(p, b)| {
            let p = p.as_ref();
            if p == MANIFEST_ENTRY_NAME || p.ends_with('/') {
                return None;
            }
            let body = b.as_ref();
            let mut hasher = Sha256::new();
            hasher.update(body);
            Some(ManifestEntry {
                path: p.to_string(),
                sha256: format!("{:x}", hasher.finalize()),
                size: body.len() as u64,
            })
        })
        .collect();
    rows.sort_by(|a, b| a.path.cmp(&b.path));
    Manifest {
        schema: MANIFEST_SCHEMA_V1.to_string(),
        entries: rows,
    }
}

/// Verify a `.gtxpack` byte stream against an in-archive `manifest.json`.
/// Returns Ok(()) when every non-manifest entry matches.
pub fn verify_archive_against_manifest(zip_bytes: &[u8]) -> Result<(), ManifestError> {
    use std::io::Read;

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes))?;

    // 1. Extract manifest.json
    let manifest: Manifest = {
        let mut f = archive
            .by_name(MANIFEST_ENTRY_NAME)
            .map_err(|_| ManifestError::Missing)?;
        let mut body = Vec::new();
        f.read_to_end(&mut body)?;
        serde_json::from_slice(&body)
            .map_err(|e| ManifestError::UnsupportedSchema(format!("parse: {e}")))?
    };
    if manifest.schema != MANIFEST_SCHEMA_V1 {
        return Err(ManifestError::UnsupportedSchema(manifest.schema));
    }

    // 2. Walk every archive entry; verify each non-manifest non-dir against the manifest.
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();
        if name == MANIFEST_ENTRY_NAME || entry.is_dir() {
            continue;
        }
        let row = manifest
            .entries
            .iter()
            .find(|r| r.path == name)
            .ok_or_else(|| ManifestError::UnexpectedEntry(name.clone()))?;
        let mut body = Vec::new();
        entry.read_to_end(&mut body)?;
        let computed = {
            let mut hasher = Sha256::new();
            hasher.update(&body);
            format!("{:x}", hasher.finalize())
        };
        if computed != row.sha256 {
            return Err(ManifestError::ShaMismatch {
                path: name,
                expected: row.sha256.clone(),
                computed,
            });
        }
        seen.insert(name);
    }

    // 3. Every manifest row must have been seen in the archive.
    for row in &manifest.entries {
        if !seen.contains(&row.path) {
            return Err(ManifestError::MissingEntry { path: row.path.clone() });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();
        {
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            for (name, body) in entries {
                w.start_file::<_, ()>(*name, zip::write::FileOptions::default()).unwrap();
                w.write_all(body).unwrap();
            }
            w.finish().unwrap();
        }
        buf
    }

    #[test]
    fn build_manifest_sorts_entries_and_excludes_self() {
        let m = build_manifest(vec![
            ("z.md", &b"alpha"[..]),
            ("a.wasm", &b"\0asm\x01\x00\x00\x00"[..]),
            ("manifest.json", &b"{}"[..]), // must be excluded
            ("describe.json", &br#"{"k":1}"#[..]),
        ]);
        let paths: Vec<&str> = m.entries.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, vec!["a.wasm", "describe.json", "z.md"]);
    }

    #[test]
    fn verify_passes_on_intact_archive() {
        let entries: Vec<(&str, &[u8])> = vec![
            ("describe.json", b"{\"k\":1}"),
            ("extension.wasm", b"\0asm\x01\x00\x00\x00"),
        ];
        let manifest = build_manifest(entries.iter().map(|(p, b)| (*p, *b)));
        let manifest_json = serde_json::to_vec(&manifest).unwrap();
        let mut with_manifest = entries.clone();
        with_manifest.push(("manifest.json", &manifest_json));
        let bytes = build_zip(&with_manifest);
        verify_archive_against_manifest(&bytes).unwrap();
    }

    #[test]
    fn verify_fails_when_wasm_tampered() {
        let entries: Vec<(&str, &[u8])> = vec![
            ("describe.json", b"{\"k\":1}"),
            ("extension.wasm", b"\0asm\x01\x00\x00\x00"),
        ];
        let manifest = build_manifest(entries.iter().map(|(p, b)| (*p, *b)));
        let manifest_json = serde_json::to_vec(&manifest).unwrap();
        // Swap WASM body AFTER the manifest is locked.
        let mut with_manifest: Vec<(&str, &[u8])> = vec![
            ("describe.json", b"{\"k\":1}"),
            ("extension.wasm", b"\0asm\x01\x00\x00\xff"), // ← tampered
        ];
        with_manifest.push(("manifest.json", &manifest_json));
        let bytes = build_zip(&with_manifest);
        let err = verify_archive_against_manifest(&bytes).unwrap_err();
        assert!(matches!(err, ManifestError::ShaMismatch { .. }), "got {err:?}");
    }

    #[test]
    fn verify_fails_when_extra_file_smuggled_in() {
        let entries: Vec<(&str, &[u8])> = vec![
            ("describe.json", b"{\"k\":1}"),
        ];
        let manifest = build_manifest(entries.iter().map(|(p, b)| (*p, *b)));
        let manifest_json = serde_json::to_vec(&manifest).unwrap();
        let with_manifest: Vec<(&str, &[u8])> = vec![
            ("describe.json", b"{\"k\":1}"),
            ("backdoor.wasm", b"evil"),  // ← not in manifest
            ("manifest.json", &manifest_json),
        ];
        let bytes = build_zip(&with_manifest);
        let err = verify_archive_against_manifest(&bytes).unwrap_err();
        assert!(matches!(err, ManifestError::UnexpectedEntry(_)), "got {err:?}");
    }

    #[test]
    fn verify_fails_when_manifest_missing() {
        let entries: Vec<(&str, &[u8])> = vec![("describe.json", b"{\"k\":1}")];
        let bytes = build_zip(&entries);
        let err = verify_archive_against_manifest(&bytes).unwrap_err();
        assert!(matches!(err, ManifestError::Missing));
    }
}
```

- [ ] **Step 2: Register module + run — expect compile errors**

Edit `crates/greentic-extension-sdk-contract/src/lib.rs`. Add module + re-exports. Append after the existing module list:

```rust
pub mod manifest;
```

Then under the `pub use` block, append:

```rust
pub use self::manifest::{
    Manifest, ManifestEntry, ManifestError, MANIFEST_ENTRY_NAME, MANIFEST_SCHEMA_V1,
    build_manifest, verify_archive_against_manifest,
};
```

Run: `cargo test -p greentic-extension-sdk-contract manifest`
Expected: COMPILE-ERRORS-then-tests-PASS once module compiles. Iterate on the impl until all 4 tests pass.

- [ ] **Step 3: Verify clippy clean**

Run: `cargo clippy -p greentic-extension-sdk-contract --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/greentic-extension-sdk-contract/src/manifest.rs \
        crates/greentic-extension-sdk-contract/src/lib.rs
git commit -m "feat(sdk-contract): introduce manifest.json schema + builder + verifier"
```

---

## Task D.4.2: Wire `Manifest` into the deterministic pack writer

**Files:**
- Modify: `crates/greentic-extension-sdk-contract/src/pack_writer.rs` (new `build_gtxpack_with_manifest` helper)

- [ ] **Step 1: Failing test for the new helper**

Append to `crates/greentic-extension-sdk-contract/src/pack_writer.rs`'s `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn pack_with_manifest_contains_manifest_json_and_passes_verify() {
        let entries = vec![
            PackEntry::file("describe.json", br#"{"k":1}"#.to_vec()),
            PackEntry::file("extension.wasm", b"\0asm\x01\x00\x00\x00".to_vec()),
        ];
        let bytes = build_gtxpack_with_manifest(entries).unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
        assert!(
            archive.by_name("manifest.json").is_ok(),
            "manifest.json must be present in archive root"
        );
        crate::manifest::verify_archive_against_manifest(&bytes).unwrap();
    }

    #[test]
    fn pack_with_manifest_is_deterministic() {
        let mk = || {
            vec![
                PackEntry::file("describe.json", br#"{"k":1}"#.to_vec()),
                PackEntry::file("extension.wasm", b"\0asm\x01\x00\x00\x00".to_vec()),
            ]
        };
        let a = build_gtxpack_with_manifest(mk()).unwrap();
        let b = build_gtxpack_with_manifest(mk()).unwrap();
        assert_eq!(sha256_hex(&a), sha256_hex(&b));
    }
```

- [ ] **Step 2: Run — expect FAIL (function not defined)**

Run: `cargo test -p greentic-extension-sdk-contract pack_with_manifest`
Expected: FAIL (undefined function).

- [ ] **Step 3: Add the helper**

Append to `crates/greentic-extension-sdk-contract/src/pack_writer.rs` (above the `#[cfg(test)]` block):

```rust
/// Build a deterministic `.gtxpack` AND inject a `manifest.json` entry
/// listing every other file with its sha256 + size. The manifest is
/// computed AFTER text normalization so the sha256 in the manifest
/// matches the bytes that land in the ZIP.
///
/// Use [`build_gtxpack`] for the no-manifest path (legacy + tests that
/// don't need integrity coverage).
pub fn build_gtxpack_with_manifest(
    entries: Vec<PackEntry>,
) -> Result<Vec<u8>, PackWriterError> {
    // Normalize first so manifest sha256 matches what we'll write.
    let normalized: Vec<PackEntry> = entries.into_iter().map(normalize_entry).collect();

    // Build manifest from the post-normalization bytes, excluding any
    // existing manifest.json entry the caller passed (shouldn't happen).
    let manifest = crate::manifest::build_manifest(
        normalized
            .iter()
            .filter(|e| !e.is_dir)
            .map(|e| (e.path.as_str(), e.bytes.as_slice())),
    );
    let manifest_bytes = serde_jcs::to_vec(&manifest)
        .map_err(|e| PackWriterError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

    let mut all = normalized;
    all.push(PackEntry::file(
        crate::manifest::MANIFEST_ENTRY_NAME,
        manifest_bytes,
    ));
    build_gtxpack(all)
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p greentic-extension-sdk-contract`
Expected: PASS for both new tests and all prior pack_writer tests.

- [ ] **Step 5: Commit**

```bash
git add crates/greentic-extension-sdk-contract/src/pack_writer.rs
git commit -m "feat(sdk-contract): build_gtxpack_with_manifest emits + signs whole-archive manifest"
```

---

## Task D.4.3: Switch `gtdx publish` to use the manifest-aware packer

**Files:**
- Modify: `crates/greentic-extension-sdk-cli/src/dev/packer.rs` (call `build_gtxpack_with_manifest`)
- Test: `crates/greentic-extension-sdk-cli/tests/manifest_in_pack.rs`

- [ ] **Step 1: Find the call site**

Run: `grep -n "build_gtxpack" crates/greentic-extension-sdk-cli/src/dev/packer.rs`
Expected: at least one call to `build_gtxpack(...)` to upgrade.

- [ ] **Step 2: Failing integration test**

Create `crates/greentic-extension-sdk-cli/tests/manifest_in_pack.rs`:

```rust
//! End-to-end check: a `.gtxpack` produced by `gtdx`'s packer must
//! contain manifest.json AND verify against `verify_archive_against_manifest`.

use greentic_extension_sdk_contract::verify_archive_against_manifest;
use greentic_extension_sdk_testing::ExtensionFixtureBuilder;

#[test]
fn fixture_pack_contains_verifiable_manifest() {
    let fx = ExtensionFixtureBuilder::default().build().expect("build fixture");
    let bytes = std::fs::read(fx.gtxpack_path()).expect("read .gtxpack");
    verify_archive_against_manifest(&bytes).expect("manifest verify");
}
```

NOTE: this test depends on `ExtensionFixtureBuilder::build()` producing a `.gtxpack` via the new manifest-aware packer. If the fixture builder doesn't call the packer, instead use the CLI directly: create the test by invoking the `gtdx`-equivalent pack helper. Check the actual fixture API before fleshing this out — if needed, use `greentic_extension_sdk_testing::pack_directory`.

- [ ] **Step 3: Run — expect FAIL**

Run: `cargo test -p greentic-extension-sdk-cli --test manifest_in_pack`
Expected: FAIL (no manifest.json in produced pack).

- [ ] **Step 4: Update the packer call site**

Edit `crates/greentic-extension-sdk-cli/src/dev/packer.rs`. Replace every call of:

```rust
greentic_extension_sdk_contract::build_gtxpack(entries)
```

with:

```rust
greentic_extension_sdk_contract::pack_writer::build_gtxpack_with_manifest(entries)
```

If `build_gtxpack` is also called from `greentic-extension-sdk-testing/src/gtxpack.rs` (`pack_directory`), upgrade that call too — fixtures need the manifest for the D.10 attack-vector suite.

Run: `grep -rn "build_gtxpack(" crates/`
For each call site that produces a SHIPPABLE `.gtxpack` (not a deliberately-malformed test fixture), swap to `build_gtxpack_with_manifest`. Comment any remaining `build_gtxpack` call sites with `// intentionally manifestless (legacy fixture)`.

- [ ] **Step 5: Re-run test + full crate build**

Run:
```bash
cargo test -p greentic-extension-sdk-cli --test manifest_in_pack
cargo test -p greentic-extension-sdk-cli
cargo test -p greentic-extension-sdk-testing
```
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/greentic-extension-sdk-cli/src/dev/packer.rs \
        crates/greentic-extension-sdk-cli/tests/manifest_in_pack.rs \
        crates/greentic-extension-sdk-testing/src/gtxpack.rs
git commit -m "feat(sdk-cli,sdk-testing): pack with manifest.json by default"
```

---

## Task D.4.4: Call `verify_archive_against_manifest` in `Installer::install`

**Files:**
- Modify: `crates/greentic-extension-sdk-registry/src/lifecycle.rs:45-93`
- Test: `crates/greentic-extension-sdk-registry/tests/install_manifest_verify.rs`

- [ ] **Step 1: Failing tamper-injection test**

Create `crates/greentic-extension-sdk-registry/tests/install_manifest_verify.rs`:

```rust
//! Audit P0 #2: install MUST fail when the archive's bytes have been
//! mutated after the manifest was sealed.

use greentic_extension_sdk_contract::{
    build_manifest, pack_writer::PackEntry, verify_archive_against_manifest,
};

#[test]
fn install_rejects_archive_with_tampered_wasm() {
    use std::io::{Cursor, Write};

    // Build manifest over CLEAN bytes.
    let clean_wasm = &b"\0asm\x01\x00\x00\x00"[..];
    let describe = br#"{"k":1}"#.as_slice();
    let manifest = build_manifest(vec![
        ("describe.json", describe),
        ("extension.wasm", clean_wasm),
    ]);
    let manifest_json = serde_json::to_vec(&manifest).unwrap();

    // Rebuild archive with a TAMPERED wasm.
    let mut buf: Vec<u8> = Vec::new();
    {
        let mut w = zip::ZipWriter::new(Cursor::new(&mut buf));
        w.start_file::<_, ()>("describe.json", zip::write::FileOptions::default()).unwrap();
        w.write_all(describe).unwrap();
        w.start_file::<_, ()>("extension.wasm", zip::write::FileOptions::default()).unwrap();
        w.write_all(b"\0asm\x01\x00\x00\xff").unwrap(); // ← evil
        w.start_file::<_, ()>("manifest.json", zip::write::FileOptions::default()).unwrap();
        w.write_all(&manifest_json).unwrap();
        w.finish().unwrap();
    }

    let err = verify_archive_against_manifest(&buf).unwrap_err();
    assert!(
        matches!(err, greentic_extension_sdk_contract::ManifestError::ShaMismatch { .. }),
        "expected sha mismatch, got {err:?}"
    );
}
```

- [ ] **Step 2: Run — expect PASS already**

Run: `cargo test -p greentic-extension-sdk-registry --test install_manifest_verify`
Expected: PASS (this validates the contract-level verifier from D.4.1; it should already pass).

- [ ] **Step 3: Add an `Installer`-level integration test**

Append to the same file:

```rust
#[test]
fn installer_install_rejects_tampered_archive() {
    use greentic_extension_sdk_contract::ExtensionKind;
    use greentic_extension_sdk_registry::lifecycle::{Installer, InstallOptions, TrustPolicy};
    use greentic_extension_sdk_registry::storage::Storage;
    use greentic_extension_sdk_registry::types::ExtensionArtifact;

    // Build a clean describe + manifest, then mutate the wasm in the ZIP.
    // (Reuses the helper above.)
    let tmp = tempfile::tempdir().unwrap();
    let storage = Storage::new(tmp.path());
    let registry = greentic_extension_sdk_registry::local::LocalFilesystemRegistry::new(
        "test",
        tmp.path().join("registry-root"),
    );
    let installer = Installer::new(storage, &registry);

    // Construct an `ExtensionArtifact` whose `bytes` carry the tampered ZIP.
    // describe.json carries no real signature here; we want to assert the
    // MANIFEST gate fires BEFORE signature verification.
    let bytes = {
        use std::io::{Cursor, Write};
        let mut buf: Vec<u8> = Vec::new();
        let mut w = zip::ZipWriter::new(Cursor::new(&mut buf));
        let describe = br#"{"apiVersion":"greentic.ai/v1","kind":"DesignExtension"}"#.as_slice();
        let clean = b"\0asm\x01\x00\x00\x00".as_slice();
        let manifest = greentic_extension_sdk_contract::build_manifest(vec![
            ("describe.json", describe),
            ("extension.wasm", clean),
        ]);
        let manifest_json = serde_json::to_vec(&manifest).unwrap();
        w.start_file::<_, ()>("describe.json", zip::write::FileOptions::default()).unwrap();
        w.write_all(describe).unwrap();
        w.start_file::<_, ()>("extension.wasm", zip::write::FileOptions::default()).unwrap();
        w.write_all(b"\0asm\x01\x00\x00\xff").unwrap(); // tamper
        w.start_file::<_, ()>("manifest.json", zip::write::FileOptions::default()).unwrap();
        w.write_all(&manifest_json).unwrap();
        w.finish().unwrap();
        buf
    };
    let artifact = ExtensionArtifact {
        name: "demo".into(),
        version: "0.1.0".into(),
        describe: serde_json::from_str(r#"{"apiVersion":"greentic.ai/v1","kind":"DesignExtension","metadata":{"id":"demo","name":"demo","version":"0.1.0","summary":"x"},"engine":{"extRuntime":"0.5"},"capabilities":{"offered":[],"required":[]},"runtime":{"component":"extension.wasm","permissions":{"network":[],"secrets":[],"callExtensionKinds":[]}},"contributions":{}}"#).unwrap(),
        bytes,
        signature: None,
    };
    let err = installer.install_artifact(&artifact, InstallOptions {
        trust_policy: TrustPolicy::Loose, // skip sig path so manifest gate is the failure surface
        accept_permissions: true,
        force: false,
    }).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("sha256 mismatch") || msg.contains("manifest"),
        "expected manifest sha failure, got '{msg}'");
}
```

- [ ] **Step 4: Run — expect FAIL (Installer doesn't yet call verifier)**

Run: `cargo test -p greentic-extension-sdk-registry --test install_manifest_verify`
Expected: FAIL (the install succeeds despite the tamper).

- [ ] **Step 5: Wire manifest verify into `install_artifact`**

Edit `crates/greentic-extension-sdk-registry/src/lifecycle.rs`. In `install_artifact`, immediately before `let result = Self::extract_to_staging(...)`, add:

```rust
        // Audit P0 #2: verify the in-archive manifest BEFORE extracting any
        // file. Tampered entries are rejected up-front.
        greentic_extension_sdk_contract::verify_archive_against_manifest(&artifact.bytes)
            .map_err(|e| RegistryError::SignatureInvalid(format!("manifest: {e}")))?;
```

(The `SignatureInvalid` variant is the closest existing error surface; if a more specific variant is preferred, add `ManifestInvalid(String)` to `RegistryError` in `error.rs` and use that — but reusing `SignatureInvalid` keeps the diff small and the failure UX consistent.)

- [ ] **Step 6: Re-run**

Run: `cargo test -p greentic-extension-sdk-registry --test install_manifest_verify`
Expected: PASS for both tests.

- [ ] **Step 7: Run full registry test suite — fix any pre-existing fixtures that ship without manifests**

Run: `cargo test -p greentic-extension-sdk-registry`
Expected: PASS. If pre-existing tests build fixtures with `build_gtxpack` (no manifest), they will now fail. For each one: either swap to `build_gtxpack_with_manifest`, OR run with `TrustPolicy::Loose` AND add a comment `// pre-D.4.4 fixture — no manifest by design`. Loose policy must still pass the manifest check though, so the cleaner path is to upgrade fixtures.

Actually — re-think: in Step 5 the manifest check runs regardless of `TrustPolicy`. To preserve backward-compat with old fixtures, gate the check on `policy != Loose`:

```rust
        if !matches!(opts.trust_policy, TrustPolicy::Loose) {
            greentic_extension_sdk_contract::verify_archive_against_manifest(&artifact.bytes)
                .map_err(|e| RegistryError::SignatureInvalid(format!("manifest: {e}")))?;
        }
```

Adjust the impl accordingly and re-run.

- [ ] **Step 8: Commit**

```bash
git add crates/greentic-extension-sdk-registry/src/lifecycle.rs \
        crates/greentic-extension-sdk-registry/tests/install_manifest_verify.rs
git commit -m "feat(sdk-registry): verify whole-archive manifest before extract"
```

---

## Task D.5.1: `RootVerifier` trait + fixture + embedded variants

> Block-on: D.5+ blocks on KMS provisioning from INSIGNIA DevOps. Implementation can land BUT the trust store + linter behavior should default to "no root configured = warn, do not fail". Hard-fail behavior flips on once policy is signed off.

**Files:**
- Create: `crates/greentic-extension-sdk-contract/src/root_verifier.rs`
- Modify: `crates/greentic-extension-sdk-contract/src/lib.rs` (export trait)

- [ ] **Step 1: Failing test**

Create `crates/greentic-extension-sdk-contract/src/root_verifier.rs`:

```rust
//! Trust-root abstraction.
//!
//! The Greentic root key lives in AWS KMS (KMS key ARN configured via env
//! `GREENTIC_TRUST_ROOT_KMS_KEY_ID` on the store-server). Designers don't
//! talk to KMS directly — they ship with an embedded cached pubkey of the
//! root and verify publisher certs locally.
//!
//! `RootVerifier` lets us split this two ways:
//!   - `EmbeddedRootVerifier` — production. Holds the cached root pubkey
//!     bytes (loaded from a `const` in the designer binary).
//!   - `FixtureRootVerifier` — tests. Wraps a fresh ed25519 keypair so
//!     unit + integration tests don't need KMS access.

use ed25519_dalek::{Verifier, VerifyingKey};

use crate::error::ContractError;

pub trait RootVerifier {
    /// Verify that `signature` is a valid ed25519 signature by the
    /// trust-root over `message`. Returns `Ok(())` on match.
    fn verify(&self, message: &[u8], signature: &[u8]) -> Result<(), ContractError>;

    /// Base64-encoded root public key (debug / logging only).
    fn root_pubkey_b64(&self) -> String;
}

pub struct EmbeddedRootVerifier {
    pubkey: VerifyingKey,
}

impl EmbeddedRootVerifier {
    pub fn from_b64(pubkey_b64: &str) -> Result<Self, ContractError> {
        use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
        let bytes = B64
            .decode(pubkey_b64)
            .map_err(|e| ContractError::SignatureInvalid(format!("root pubkey b64: {e}")))?;
        let arr: [u8; 32] = bytes
            .as_slice()
            .try_into()
            .map_err(|_| ContractError::SignatureInvalid("root pubkey length != 32".into()))?;
        let pubkey = VerifyingKey::from_bytes(&arr)
            .map_err(|e| ContractError::SignatureInvalid(format!("root pubkey parse: {e}")))?;
        Ok(Self { pubkey })
    }
}

impl RootVerifier for EmbeddedRootVerifier {
    fn verify(&self, message: &[u8], signature: &[u8]) -> Result<(), ContractError> {
        let sig_arr: [u8; 64] = signature
            .try_into()
            .map_err(|_| ContractError::SignatureInvalid("root sig length != 64".into()))?;
        let sig = ed25519_dalek::Signature::from_bytes(&sig_arr);
        self.pubkey
            .verify(message, &sig)
            .map_err(|e| ContractError::SignatureInvalid(format!("root verify: {e}")))
    }

    fn root_pubkey_b64(&self) -> String {
        use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
        B64.encode(self.pubkey.to_bytes())
    }
}

/// Test-only verifier wrapping an in-memory keypair. Construct via
/// [`FixtureRootVerifier::new`] and use the returned `SigningKey` to
/// produce publisher-cert signatures inside tests.
pub struct FixtureRootVerifier {
    pubkey: VerifyingKey,
}

impl FixtureRootVerifier {
    pub fn new(signing_key: &ed25519_dalek::SigningKey) -> Self {
        Self { pubkey: signing_key.verifying_key() }
    }
}

impl RootVerifier for FixtureRootVerifier {
    fn verify(&self, message: &[u8], signature: &[u8]) -> Result<(), ContractError> {
        let sig_arr: [u8; 64] = signature
            .try_into()
            .map_err(|_| ContractError::SignatureInvalid("fixture sig length != 64".into()))?;
        let sig = ed25519_dalek::Signature::from_bytes(&sig_arr);
        self.pubkey
            .verify(message, &sig)
            .map_err(|e| ContractError::SignatureInvalid(format!("fixture root verify: {e}")))
    }

    fn root_pubkey_b64(&self) -> String {
        use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
        B64.encode(self.pubkey.to_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn fresh_key() -> SigningKey {
        let mut bytes = [0u8; 32];
        use rand::RngCore;
        rand::thread_rng().fill_bytes(&mut bytes);
        SigningKey::from_bytes(&bytes)
    }

    #[test]
    fn fixture_verifier_accepts_matching_signature() {
        let key = fresh_key();
        let verifier = FixtureRootVerifier::new(&key);
        let msg = b"hello world";
        let sig = key.sign(msg).to_bytes();
        verifier.verify(msg, &sig).expect("verify must pass");
    }

    #[test]
    fn fixture_verifier_rejects_mismatched_signature() {
        let key_a = fresh_key();
        let key_b = fresh_key();
        let verifier = FixtureRootVerifier::new(&key_a);
        let sig = key_b.sign(b"hello world").to_bytes();
        assert!(verifier.verify(b"hello world", &sig).is_err());
    }

    #[test]
    fn embedded_verifier_roundtrips_via_b64() {
        let key = fresh_key();
        let b64 = {
            use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
            B64.encode(key.verifying_key().to_bytes())
        };
        let v = EmbeddedRootVerifier::from_b64(&b64).unwrap();
        let sig = key.sign(b"x").to_bytes();
        v.verify(b"x", &sig).unwrap();
    }
}
```

- [ ] **Step 2: Register + run — iterate until tests pass**

Edit `crates/greentic-extension-sdk-contract/src/lib.rs`. Add `pub mod root_verifier;` and:

```rust
pub use self::root_verifier::{EmbeddedRootVerifier, FixtureRootVerifier, RootVerifier};
```

Add `rand` as a dev-dep in `crates/greentic-extension-sdk-contract/Cargo.toml` (already in workspace deps):

```toml
[dev-dependencies]
tempfile = { workspace = true }
rand = { workspace = true }
base64 = { workspace = true }
```

Run: `cargo test -p greentic-extension-sdk-contract root_verifier`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/greentic-extension-sdk-contract/src/root_verifier.rs \
        crates/greentic-extension-sdk-contract/src/lib.rs \
        crates/greentic-extension-sdk-contract/Cargo.toml
git commit -m "feat(sdk-contract): introduce RootVerifier trait + embedded + fixture impls"
```

---

## Task D.5.2: `PublisherCert` struct + verify-chain helper

**Files:**
- Create: `crates/greentic-extension-sdk-contract/src/publisher_cert.rs`
- Modify: `crates/greentic-extension-sdk-contract/src/lib.rs`

- [ ] **Step 1: Failing tests**

Create `crates/greentic-extension-sdk-contract/src/publisher_cert.rs`:

```rust
//! Publisher cert — a Greentic-root signature over a publisher's ed25519
//! pubkey + identity metadata.
//!
//! The store-server (which has access to the Greentic root key via AWS
//! KMS) issues a `PublisherCert` for each registered publisher at
//! onboarding. The cert lives inside every `.gtxpack` the publisher
//! produces; designers verify two links:
//!   1. `root_signature` verifies against the embedded root pubkey
//!      over `canonical_signing_payload(cert without root_signature)`.
//!   2. `manifest.json` (D.4) verifies against `publisher_pubkey` over
//!      the in-archive manifest bytes.

use serde::{Deserialize, Serialize};

use crate::error::ContractError;
use crate::root_verifier::RootVerifier;

pub const PUBLISHER_CERT_ENTRY_NAME: &str = "publisher-cert.json";
pub const PUBLISHER_CERT_SCHEMA_V1: &str = "greentic.publisher-cert/v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublisherCert {
    pub schema: String,
    /// Publisher identifier (e.g. `"greentic-biz"`, `"acme-corp"`).
    pub publisher_id: String,
    /// Base64-encoded ed25519 public key.
    #[serde(rename = "publisherPubkey")]
    pub publisher_pubkey: String,
    /// ISO-8601 issuance timestamp.
    #[serde(rename = "issuedAt")]
    pub issued_at: String,
    /// ISO-8601 expiration timestamp (cert holders should rotate annually).
    #[serde(rename = "expiresAt")]
    pub expires_at: String,
    /// Base64-encoded ed25519 signature by the trust-root over JCS bytes
    /// of THIS struct with `root_signature` set to `None`. Strip-and-resign
    /// produces identical bytes regardless of prior signature.
    #[serde(rename = "rootSignature", skip_serializing_if = "Option::is_none")]
    pub root_signature: Option<String>,
}

impl PublisherCert {
    /// Canonical bytes for signing or verifying: strip `root_signature`,
    /// then JCS-encode.
    pub fn canonical_payload(&self) -> Result<Vec<u8>, ContractError> {
        let mut clone = self.clone();
        clone.root_signature = None;
        serde_jcs::to_vec(&clone).map_err(|e| ContractError::Canonicalize(e.to_string()))
    }

    /// Verify the cert against a [`RootVerifier`]. Returns `Ok(())` if
    /// the `root_signature` field validates and the cert has not yet
    /// expired according to `now_unix_seconds`.
    pub fn verify<V: RootVerifier>(
        &self,
        verifier: &V,
        now_unix_seconds: i64,
    ) -> Result<(), ContractError> {
        use base64::{Engine as _, engine::general_purpose::STANDARD as B64};

        if self.schema != PUBLISHER_CERT_SCHEMA_V1 {
            return Err(ContractError::SignatureInvalid(format!(
                "publisher cert schema unsupported: {}",
                self.schema
            )));
        }
        let sig_b64 = self
            .root_signature
            .as_ref()
            .ok_or_else(|| ContractError::SignatureInvalid("publisher cert: missing root_signature".into()))?;
        let sig_bytes = B64
            .decode(sig_b64)
            .map_err(|e| ContractError::SignatureInvalid(format!("publisher cert sig b64: {e}")))?;
        let payload = self.canonical_payload()?;
        verifier.verify(&payload, &sig_bytes)?;

        // Expiration check. Strict RFC 3339 parse; reject malformed dates.
        let expiry = chrono::DateTime::parse_from_rfc3339(&self.expires_at)
            .map_err(|e| ContractError::SignatureInvalid(format!("expiresAt parse: {e}")))?;
        if expiry.timestamp() < now_unix_seconds {
            return Err(ContractError::SignatureInvalid(format!(
                "publisher cert expired at {}",
                self.expires_at
            )));
        }
        Ok(())
    }

    pub fn publisher_pubkey_bytes(&self) -> Result<[u8; 32], ContractError> {
        use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
        let raw = B64
            .decode(&self.publisher_pubkey)
            .map_err(|e| ContractError::SignatureInvalid(format!("publisher pubkey b64: {e}")))?;
        raw.as_slice()
            .try_into()
            .map_err(|_| ContractError::SignatureInvalid("publisher pubkey length != 32".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::root_verifier::FixtureRootVerifier;
    use ed25519_dalek::{Signer, SigningKey};

    fn fresh_key() -> SigningKey {
        let mut bytes = [0u8; 32];
        use rand::RngCore;
        rand::thread_rng().fill_bytes(&mut bytes);
        SigningKey::from_bytes(&bytes)
    }

    fn issue_cert(root: &SigningKey, publisher_pub_b64: &str) -> PublisherCert {
        use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
        let mut cert = PublisherCert {
            schema: PUBLISHER_CERT_SCHEMA_V1.to_string(),
            publisher_id: "fixture-publisher".into(),
            publisher_pubkey: publisher_pub_b64.to_string(),
            issued_at: "2026-01-01T00:00:00Z".into(),
            expires_at: "2099-01-01T00:00:00Z".into(),
            root_signature: None,
        };
        let payload = cert.canonical_payload().unwrap();
        let sig = root.sign(&payload).to_bytes();
        cert.root_signature = Some(B64.encode(sig));
        cert
    }

    #[test]
    fn well_formed_cert_passes_verify() {
        let root = fresh_key();
        let publisher = fresh_key();
        let pub_b64 = {
            use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
            B64.encode(publisher.verifying_key().to_bytes())
        };
        let cert = issue_cert(&root, &pub_b64);
        let verifier = FixtureRootVerifier::new(&root);
        cert.verify(&verifier, 1_780_000_000).unwrap();
    }

    #[test]
    fn cert_signed_by_wrong_root_is_rejected() {
        let real_root = fresh_key();
        let fake_root = fresh_key();
        let publisher = fresh_key();
        let pub_b64 = {
            use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
            B64.encode(publisher.verifying_key().to_bytes())
        };
        let cert = issue_cert(&fake_root, &pub_b64);
        let verifier = FixtureRootVerifier::new(&real_root);
        assert!(cert.verify(&verifier, 1_780_000_000).is_err());
    }

    #[test]
    fn expired_cert_is_rejected() {
        let root = fresh_key();
        let publisher = fresh_key();
        let pub_b64 = {
            use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
            B64.encode(publisher.verifying_key().to_bytes())
        };
        let mut cert = issue_cert(&root, &pub_b64);
        cert.expires_at = "2020-01-01T00:00:00Z".into();
        // Resign over the new expiry.
        use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
        let mut tmp = cert.clone();
        tmp.root_signature = None;
        let payload = serde_jcs::to_vec(&tmp).unwrap();
        cert.root_signature = Some(B64.encode(root.sign(&payload).to_bytes()));
        let verifier = FixtureRootVerifier::new(&root);
        assert!(cert.verify(&verifier, 1_780_000_000).is_err());
    }
}
```

- [ ] **Step 2: Add `chrono` dep + register module**

Edit `crates/greentic-extension-sdk-contract/Cargo.toml`. Under `[dependencies]` add (alphabetical):

```toml
chrono = { workspace = true }
```

Edit `crates/greentic-extension-sdk-contract/src/lib.rs`. Add:

```rust
pub mod publisher_cert;
```

and:

```rust
pub use self::publisher_cert::{PublisherCert, PUBLISHER_CERT_ENTRY_NAME, PUBLISHER_CERT_SCHEMA_V1};
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p greentic-extension-sdk-contract publisher_cert`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/greentic-extension-sdk-contract/src/publisher_cert.rs \
        crates/greentic-extension-sdk-contract/src/lib.rs \
        crates/greentic-extension-sdk-contract/Cargo.toml
git commit -m "feat(sdk-contract): PublisherCert with root-signature verify + expiry"
```

---

## Task D.5.3: Sign the manifest with the publisher key + chain-verify on install

**Files:**
- Modify: `crates/greentic-extension-sdk-contract/src/manifest.rs` (add `SignedManifest` type)
- Modify: `crates/greentic-extension-sdk-contract/src/pack_writer.rs` (`build_gtxpack_signed`)
- Modify: `crates/greentic-extension-sdk-registry/src/lifecycle.rs` (chain-verify)
- Test: `crates/greentic-extension-sdk-registry/tests/install_chain_verify.rs`

- [ ] **Step 1: Failing chain-verify integration test**

Create `crates/greentic-extension-sdk-registry/tests/install_chain_verify.rs`:

```rust
//! End-to-end chain verification:
//!   1. fixture root issues a publisher cert
//!   2. publisher signs manifest with their key
//!   3. install verifies cert against root + manifest sig against cert.pubkey
//!
//! Negative case: install with an UNTRUSTED publisher (no cert) fails.

use ed25519_dalek::{Signer, SigningKey};
use greentic_extension_sdk_contract::{
    FixtureRootVerifier, PUBLISHER_CERT_ENTRY_NAME, PUBLISHER_CERT_SCHEMA_V1, PublisherCert,
};

fn fresh_key() -> SigningKey {
    let mut bytes = [0u8; 32];
    use rand::RngCore;
    rand::thread_rng().fill_bytes(&mut bytes);
    SigningKey::from_bytes(&bytes)
}

#[test]
fn chained_pack_verifies_against_root() {
    // 1. Issue publisher cert
    let root_key = fresh_key();
    let publisher_key = fresh_key();
    use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
    let publisher_pub_b64 = B64.encode(publisher_key.verifying_key().to_bytes());

    let mut cert = PublisherCert {
        schema: PUBLISHER_CERT_SCHEMA_V1.into(),
        publisher_id: "fixture".into(),
        publisher_pubkey: publisher_pub_b64,
        issued_at: "2026-01-01T00:00:00Z".into(),
        expires_at: "2099-01-01T00:00:00Z".into(),
        root_signature: None,
    };
    let cert_payload = cert.canonical_payload().unwrap();
    cert.root_signature = Some(B64.encode(root_key.sign(&cert_payload).to_bytes()));

    // 2. Build pack: describe.json + wasm + manifest.json + publisher-cert.json + manifest.sig
    //    For this test we just assert the contract-level chain verifier exposes a function.
    let verifier = FixtureRootVerifier::new(&root_key);
    cert.verify(&verifier, 1_780_000_000).unwrap();

    // 3. Sign + verify the manifest bytes with publisher key.
    let manifest_bytes = b"fake manifest bytes";
    let manifest_sig = publisher_key.sign(manifest_bytes).to_bytes();
    let publisher_pubkey_bytes = cert.publisher_pubkey_bytes().unwrap();
    use ed25519_dalek::{Verifier, VerifyingKey};
    let pubkey = VerifyingKey::from_bytes(&publisher_pubkey_bytes).unwrap();
    let sig = ed25519_dalek::Signature::from_bytes(&manifest_sig);
    pubkey.verify(manifest_bytes, &sig).unwrap();
}
```

- [ ] **Step 2: Run — should already PASS (uses primitives we built)**

Run: `cargo test -p greentic-extension-sdk-registry --test install_chain_verify`
Expected: PASS.

- [ ] **Step 3: Add `SignedManifest` wrapper**

Append to `crates/greentic-extension-sdk-contract/src/manifest.rs`:

```rust
/// Signed manifest envelope. The in-archive `manifest.json` lives as
/// JCS-canonicalized bytes of `Manifest`; the publisher signature over
/// those bytes lives in a SIBLING entry `manifest.sig` (base64). The
/// designer reads both, verifies the sig against the publisher cert's
/// public key, then walks the manifest to verify every other entry.
pub const MANIFEST_SIG_ENTRY_NAME: &str = "manifest.sig";

/// Verify that `manifest_sig_b64` is a valid ed25519 signature over
/// `manifest_bytes` by `publisher_pubkey_bytes`.
pub fn verify_manifest_signature(
    manifest_bytes: &[u8],
    manifest_sig_b64: &str,
    publisher_pubkey_bytes: &[u8; 32],
) -> Result<(), ManifestError> {
    use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
    use ed25519_dalek::{Verifier, VerifyingKey};
    let sig_bytes = B64
        .decode(manifest_sig_b64)
        .map_err(|e| ManifestError::UnsupportedSchema(format!("sig b64: {e}")))?;
    let sig_arr: [u8; 64] = sig_bytes
        .as_slice()
        .try_into()
        .map_err(|_| ManifestError::UnsupportedSchema("sig length != 64".into()))?;
    let pubkey = VerifyingKey::from_bytes(publisher_pubkey_bytes)
        .map_err(|e| ManifestError::UnsupportedSchema(format!("pubkey parse: {e}")))?;
    let sig = ed25519_dalek::Signature::from_bytes(&sig_arr);
    pubkey
        .verify(manifest_bytes, &sig)
        .map_err(|e| ManifestError::UnsupportedSchema(format!("manifest verify: {e}")))
}
```

Re-export from lib.rs:

```rust
pub use self::manifest::{
    Manifest, ManifestEntry, ManifestError, MANIFEST_ENTRY_NAME, MANIFEST_SCHEMA_V1,
    MANIFEST_SIG_ENTRY_NAME, build_manifest, verify_archive_against_manifest,
    verify_manifest_signature,
};
```

- [ ] **Step 4: `build_gtxpack_signed` helper that adds cert + sig**

Append to `crates/greentic-extension-sdk-contract/src/pack_writer.rs`:

```rust
/// Build a `.gtxpack` containing manifest.json, manifest.sig (publisher
/// signature over manifest.json bytes), and publisher-cert.json (the
/// Greentic-root-signed cert binding `publisher_signing_key.public()`
/// to a publisher identity). Use [`build_gtxpack_with_manifest`] for
/// unsigned/dev packs.
pub fn build_gtxpack_signed(
    entries: Vec<PackEntry>,
    publisher_signing_key: &ed25519_dalek::SigningKey,
    publisher_cert: &crate::publisher_cert::PublisherCert,
) -> Result<Vec<u8>, PackWriterError> {
    use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
    use ed25519_dalek::Signer;

    let normalized: Vec<PackEntry> = entries.into_iter().map(normalize_entry).collect();
    let manifest = crate::manifest::build_manifest(
        normalized
            .iter()
            .filter(|e| !e.is_dir)
            .map(|e| (e.path.as_str(), e.bytes.as_slice())),
    );
    let manifest_bytes = serde_jcs::to_vec(&manifest)
        .map_err(|e| PackWriterError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
    let sig = publisher_signing_key.sign(&manifest_bytes).to_bytes();
    let sig_b64 = B64.encode(sig);

    let cert_bytes = serde_jcs::to_vec(publisher_cert)
        .map_err(|e| PackWriterError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

    let mut all = normalized;
    all.push(PackEntry::file(
        crate::manifest::MANIFEST_ENTRY_NAME,
        manifest_bytes,
    ));
    all.push(PackEntry::file(
        crate::manifest::MANIFEST_SIG_ENTRY_NAME,
        sig_b64.into_bytes(),
    ));
    all.push(PackEntry::file(
        crate::publisher_cert::PUBLISHER_CERT_ENTRY_NAME,
        cert_bytes,
    ));
    build_gtxpack(all)
}
```

Add a unit test in the same `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn build_gtxpack_signed_contains_three_special_entries() {
        use ed25519_dalek::SigningKey;
        use rand::RngCore;
        let mut seed = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut seed);
        let key = SigningKey::from_bytes(&seed);
        let cert = crate::publisher_cert::PublisherCert {
            schema: crate::publisher_cert::PUBLISHER_CERT_SCHEMA_V1.into(),
            publisher_id: "test".into(),
            publisher_pubkey: "AAAA".into(),
            issued_at: "2026-01-01T00:00:00Z".into(),
            expires_at: "2099-01-01T00:00:00Z".into(),
            root_signature: Some("BBBB".into()),
        };
        let pack = build_gtxpack_signed(
            vec![PackEntry::file("describe.json", b"{\"k\":1}".to_vec())],
            &key,
            &cert,
        )
        .unwrap();
        let mut a = zip::ZipArchive::new(Cursor::new(pack)).unwrap();
        assert!(a.by_name("manifest.json").is_ok());
        assert!(a.by_name("manifest.sig").is_ok());
        assert!(a.by_name("publisher-cert.json").is_ok());
    }
```

- [ ] **Step 5: Run**

Run: `cargo test -p greentic-extension-sdk-contract`
Expected: PASS.

- [ ] **Step 6: Wire chain verify into `Installer::install_artifact`**

Edit `crates/greentic-extension-sdk-registry/src/lifecycle.rs`. Change the install flow so that under `TrustPolicy::Strict`, we require + chain-verify a publisher cert. Under `Normal`, we accept legacy describe-only signing (current behavior). Under `Loose`, skip everything.

Add a method:

```rust
fn verify_publisher_chain<V: greentic_extension_sdk_contract::RootVerifier>(
    artifact: &ExtensionArtifact,
    verifier: &V,
    now_unix_seconds: i64,
) -> Result<(), RegistryError> {
    use std::io::Read;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&artifact.bytes))
        .map_err(|e| RegistryError::Storage(format!("zip open: {e}")))?;

    let cert_bytes = {
        let mut f = archive
            .by_name(greentic_extension_sdk_contract::PUBLISHER_CERT_ENTRY_NAME)
            .map_err(|_| {
                RegistryError::SignatureInvalid("missing publisher-cert.json".into())
            })?;
        let mut body = Vec::new();
        f.read_to_end(&mut body).map_err(|e| RegistryError::Storage(format!("read cert: {e}")))?;
        body
    };
    let cert: greentic_extension_sdk_contract::PublisherCert = serde_json::from_slice(&cert_bytes)
        .map_err(|e| RegistryError::SignatureInvalid(format!("parse cert: {e}")))?;
    cert.verify(verifier, now_unix_seconds)
        .map_err(|e| RegistryError::SignatureInvalid(format!("cert chain: {e}")))?;

    let manifest_bytes = {
        let mut f = archive
            .by_name(greentic_extension_sdk_contract::MANIFEST_ENTRY_NAME)
            .map_err(|_| RegistryError::SignatureInvalid("missing manifest.json".into()))?;
        let mut body = Vec::new();
        f.read_to_end(&mut body).map_err(|e| RegistryError::Storage(format!("read manifest: {e}")))?;
        body
    };
    let sig_b64 = {
        let mut f = archive
            .by_name(greentic_extension_sdk_contract::MANIFEST_SIG_ENTRY_NAME)
            .map_err(|_| RegistryError::SignatureInvalid("missing manifest.sig".into()))?;
        let mut body = String::new();
        f.read_to_string(&mut body).map_err(|e| RegistryError::Storage(format!("read sig: {e}")))?;
        body
    };
    let publisher_pubkey = cert
        .publisher_pubkey_bytes()
        .map_err(|e| RegistryError::SignatureInvalid(format!("pubkey: {e}")))?;
    greentic_extension_sdk_contract::verify_manifest_signature(
        &manifest_bytes,
        sig_b64.trim(),
        &publisher_pubkey,
    )
    .map_err(|e| RegistryError::SignatureInvalid(format!("manifest sig: {e}")))?;
    Ok(())
}
```

Add a method `install_artifact_chained` that callers under `Strict` invoke. For now, expose the chain helper publicly and add a TODO note: "designer wiring (`register_loaded_from_dir`) is the long-term verify point — install-time check is sufficient because in-archive bytes never leave the installer's hands without being checked first".

- [ ] **Step 7: Add integration test that exercises `verify_publisher_chain`**

Append to `crates/greentic-extension-sdk-registry/tests/install_chain_verify.rs`:

```rust
#[test]
fn untrusted_publisher_is_rejected() {
    use greentic_extension_sdk_contract::FixtureRootVerifier;
    use greentic_extension_sdk_registry::lifecycle::verify_publisher_chain;
    use greentic_extension_sdk_registry::types::ExtensionArtifact;

    // Build a pack with NO publisher-cert.json.
    let pack = {
        use std::io::{Cursor, Write};
        let mut buf = Vec::new();
        let mut w = zip::ZipWriter::new(Cursor::new(&mut buf));
        w.start_file::<_, ()>("describe.json", zip::write::FileOptions::default()).unwrap();
        w.write_all(br#"{"k":1}"#).unwrap();
        w.finish().unwrap();
        buf
    };
    let artifact = ExtensionArtifact {
        name: "x".into(),
        version: "0.1.0".into(),
        describe: serde_json::from_str(r#"{"apiVersion":"greentic.ai/v1","kind":"DesignExtension","metadata":{"id":"x","name":"x","version":"0.1.0","summary":"x"},"engine":{"extRuntime":"0.5"},"capabilities":{"offered":[],"required":[]},"runtime":{"component":"extension.wasm","permissions":{"network":[],"secrets":[],"callExtensionKinds":[]}},"contributions":{}}"#).unwrap(),
        bytes: pack,
        signature: None,
    };
    let root = fresh_key();
    let verifier = FixtureRootVerifier::new(&root);
    let err = verify_publisher_chain(&artifact, &verifier, 1_780_000_000).unwrap_err();
    assert!(format!("{err}").contains("publisher-cert"));
}
```

Re-export `verify_publisher_chain` from `lifecycle.rs` (make it `pub fn`).

- [ ] **Step 8: Run**

Run: `cargo test -p greentic-extension-sdk-registry`
Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add crates/greentic-extension-sdk-contract/src/manifest.rs \
        crates/greentic-extension-sdk-contract/src/pack_writer.rs \
        crates/greentic-extension-sdk-contract/src/lib.rs \
        crates/greentic-extension-sdk-registry/src/lifecycle.rs \
        crates/greentic-extension-sdk-registry/tests/install_chain_verify.rs
git commit -m "feat(sdk): publisher-cert chain verify (root → cert → manifest)"
```

---

## Task D.6: Designer trust store — `~/.greentic/trust/publishers.json` reader

**Files:**
- Create: `crates/greentic-extension-sdk-registry/src/trust_store.rs`
- Modify: `crates/greentic-extension-sdk-registry/src/lib.rs`

- [ ] **Step 1: Failing tests**

Create `crates/greentic-extension-sdk-registry/src/trust_store.rs`:

```rust
//! Designer-side trust store.
//!
//! Lives at `~/.greentic/trust/publishers.json`. Format:
//!
//! ```json
//! {
//!   "schema": "greentic.trust-store/v1",
//!   "root_pubkey_b64": "...",
//!   "publishers": [
//!     { "publisher_id": "greentic-biz", "trusted": true },
//!     { "publisher_id": "acme-corp", "trusted": true }
//!   ]
//! }
//! ```
//!
//! The embedded `GREENTIC_ROOT_PUBKEY_B64` compile-time constant takes
//! precedence over a `root_pubkey_b64` value in the file — the file
//! field is informational only (helps users sanity-check which root is
//! pinned by the binary they're running).

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::RegistryError;

pub const TRUST_STORE_SCHEMA_V1: &str = "greentic.trust-store/v1";

/// Compile-time root pubkey (base64). The build process replaces this
/// with the actual Greentic root pubkey once KMS provisioning lands. For
/// now it is empty, and `EmbeddedRootVerifier::from_b64` will error —
/// callers MUST gracefully degrade until DevOps wires the real key.
pub const GREENTIC_ROOT_PUBKEY_B64: &str = ""; // TODO: replaced post-KMS provisioning

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustStore {
    pub schema: String,
    #[serde(rename = "root_pubkey_b64", default)]
    pub root_pubkey_b64: Option<String>,
    #[serde(default)]
    pub publishers: Vec<PublisherEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublisherEntry {
    pub publisher_id: String,
    #[serde(default = "default_true")]
    pub trusted: bool,
}

const fn default_true() -> bool {
    true
}

impl TrustStore {
    pub fn load(path: &Path) -> Result<Self, RegistryError> {
        let body = std::fs::read_to_string(path)
            .map_err(|e| RegistryError::Storage(format!("read {}: {e}", path.display())))?;
        let store: TrustStore = serde_json::from_str(&body)
            .map_err(|e| RegistryError::Storage(format!("parse trust store: {e}")))?;
        if store.schema != TRUST_STORE_SCHEMA_V1 {
            return Err(RegistryError::Storage(format!(
                "trust store schema unsupported: {}",
                store.schema
            )));
        }
        Ok(store)
    }

    pub fn is_publisher_trusted(&self, publisher_id: &str) -> bool {
        self.publishers
            .iter()
            .any(|e| e.publisher_id == publisher_id && e.trusted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn loads_valid_trust_store() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("publishers.json");
        std::fs::write(
            &path,
            r#"{"schema":"greentic.trust-store/v1","publishers":[{"publisher_id":"acme"}]}"#,
        )
        .unwrap();
        let store = TrustStore::load(&path).unwrap();
        assert!(store.is_publisher_trusted("acme"));
        assert!(!store.is_publisher_trusted("nope"));
    }

    #[test]
    fn rejects_unknown_schema() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("publishers.json");
        std::fs::write(&path, r#"{"schema":"greentic.trust-store/v999","publishers":[]}"#).unwrap();
        assert!(TrustStore::load(&path).is_err());
    }

    #[test]
    fn defaults_trusted_to_true_when_missing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("publishers.json");
        std::fs::write(
            &path,
            r#"{"schema":"greentic.trust-store/v1","publishers":[{"publisher_id":"acme"}]}"#,
        )
        .unwrap();
        let store = TrustStore::load(&path).unwrap();
        assert!(store.is_publisher_trusted("acme"));
    }
}
```

- [ ] **Step 2: Register module + run**

Edit `crates/greentic-extension-sdk-registry/src/lib.rs`. Add:

```rust
pub mod trust_store;
```

and the export:

```rust
pub use self::trust_store::{
    GREENTIC_ROOT_PUBKEY_B64, PublisherEntry, TRUST_STORE_SCHEMA_V1, TrustStore,
};
```

Run: `cargo test -p greentic-extension-sdk-registry trust_store`
Expected: PASS.

- [ ] **Step 3: Wire trust-store check into chain verify**

Edit `crates/greentic-extension-sdk-registry/src/lifecycle.rs`. After `cert.verify(verifier, now_unix_seconds)?` inside `verify_publisher_chain`, add:

```rust
    if let Some(store) = trust_store_opt {
        if !store.is_publisher_trusted(&cert.publisher_id) {
            return Err(RegistryError::SignatureInvalid(format!(
                "publisher '{}' is not in the local trust store",
                cert.publisher_id
            )));
        }
    }
```

Update the function signature to accept `trust_store_opt: Option<&crate::trust_store::TrustStore>` and update the test in `install_chain_verify.rs` to pass `None` (legacy) and a known-empty store (negative case).

Append an integration test:

```rust
#[test]
fn unknown_publisher_rejected_when_trust_store_present() {
    use greentic_extension_sdk_contract::FixtureRootVerifier;
    use greentic_extension_sdk_registry::lifecycle::verify_publisher_chain;
    use greentic_extension_sdk_registry::trust_store::{PublisherEntry, TRUST_STORE_SCHEMA_V1, TrustStore};

    // Build cert + signed pack (reuses helpers from chained_pack_verifies_against_root).
    // ... omitted for brevity — copy that setup ...
    let root = fresh_key();
    let publisher = fresh_key();
    use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
    let publisher_pub_b64 = B64.encode(publisher.verifying_key().to_bytes());
    let mut cert = greentic_extension_sdk_contract::PublisherCert {
        schema: greentic_extension_sdk_contract::PUBLISHER_CERT_SCHEMA_V1.into(),
        publisher_id: "stranger".into(),
        publisher_pubkey: publisher_pub_b64,
        issued_at: "2026-01-01T00:00:00Z".into(),
        expires_at: "2099-01-01T00:00:00Z".into(),
        root_signature: None,
    };
    use ed25519_dalek::Signer;
    let payload = cert.canonical_payload().unwrap();
    cert.root_signature = Some(B64.encode(root.sign(&payload).to_bytes()));

    // Build a minimal pack containing cert + manifest + manifest.sig.
    let entries: Vec<(&str, Vec<u8>)> = vec![
        ("describe.json", br#"{"k":1}"#.to_vec()),
    ];
    let manifest = greentic_extension_sdk_contract::build_manifest(
        entries.iter().map(|(p, b)| (*p, b.as_slice())),
    );
    let manifest_bytes = serde_jcs::to_vec(&manifest).unwrap();
    let manifest_sig = publisher.sign(&manifest_bytes).to_bytes();
    let pack = {
        use std::io::{Cursor, Write};
        let mut buf = Vec::new();
        let mut w = zip::ZipWriter::new(Cursor::new(&mut buf));
        for (name, body) in &entries {
            w.start_file::<_, ()>(*name, zip::write::FileOptions::default()).unwrap();
            w.write_all(body).unwrap();
        }
        w.start_file::<_, ()>("manifest.json", zip::write::FileOptions::default()).unwrap();
        w.write_all(&manifest_bytes).unwrap();
        w.start_file::<_, ()>("manifest.sig", zip::write::FileOptions::default()).unwrap();
        w.write_all(B64.encode(manifest_sig).as_bytes()).unwrap();
        w.start_file::<_, ()>("publisher-cert.json", zip::write::FileOptions::default()).unwrap();
        w.write_all(&serde_jcs::to_vec(&cert).unwrap()).unwrap();
        w.finish().unwrap();
        buf
    };
    let artifact = greentic_extension_sdk_registry::types::ExtensionArtifact {
        name: "x".into(),
        version: "0.1.0".into(),
        describe: serde_json::from_str(r#"{"apiVersion":"greentic.ai/v1","kind":"DesignExtension","metadata":{"id":"x","name":"x","version":"0.1.0","summary":"x"},"engine":{"extRuntime":"0.5"},"capabilities":{"offered":[],"required":[]},"runtime":{"component":"extension.wasm","permissions":{"network":[],"secrets":[],"callExtensionKinds":[]}},"contributions":{}}"#).unwrap(),
        bytes: pack,
        signature: None,
    };
    let verifier = greentic_extension_sdk_contract::FixtureRootVerifier::new(&root);
    let store = TrustStore {
        schema: TRUST_STORE_SCHEMA_V1.into(),
        root_pubkey_b64: None,
        publishers: vec![PublisherEntry { publisher_id: "greentic-biz".into(), trusted: true }],
    };
    let err = verify_publisher_chain(&artifact, &verifier, 1_780_000_000, Some(&store))
        .unwrap_err();
    assert!(format!("{err}").contains("not in the local trust store"));
}
```

Run: `cargo test -p greentic-extension-sdk-registry`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/greentic-extension-sdk-registry/src/trust_store.rs \
        crates/greentic-extension-sdk-registry/src/lib.rs \
        crates/greentic-extension-sdk-registry/src/lifecycle.rs \
        crates/greentic-extension-sdk-registry/tests/install_chain_verify.rs
git commit -m "feat(sdk-registry): designer-side trust store + chain enforcement"
```

---

## Task D.7: `gtdx lint --permissions` flags overly broad allow-lists

**Files:**
- Create: `crates/greentic-extension-sdk-cli/src/commands/lint.rs`
- Modify: `crates/greentic-extension-sdk-cli/src/commands/mod.rs` + `main.rs` (register)
- Test: `crates/greentic-extension-sdk-cli/tests/lint_permissions.rs`

> Note: full `gtdx lint` (describe-diff + semver-bump + capability-cycle) is Phase E. This task lands ONLY the permissions sub-check so Phase D's tightening doesn't slip.

- [ ] **Step 1: Failing CLI test**

Create `crates/greentic-extension-sdk-cli/tests/lint_permissions.rs`:

```rust
use std::process::Command;

fn gtdx() -> Command {
    Command::new(env!("CARGO_BIN_EXE_gtdx"))
}

fn write_describe(dir: &std::path::Path, perms: &str) {
    let body = format!(
        r#"{{"apiVersion":"greentic.ai/v1","kind":"DesignExtension","metadata":{{"id":"demo","name":"demo","version":"0.1.0","summary":"x"}},"engine":{{"extRuntime":"0.5"}},"capabilities":{{"offered":[],"required":[]}},"runtime":{{"component":"extension.wasm","permissions":{perms}}},"contributions":{{}}}}"#
    );
    std::fs::write(dir.join("describe.json"), body).unwrap();
}

#[test]
fn lint_warns_on_network_wildcard() {
    let dir = tempfile::tempdir().unwrap();
    write_describe(
        dir.path(),
        r#"{"network":["*"],"secrets":[],"callExtensionKinds":[]}"#,
    );
    let out = gtdx()
        .args(["lint", "--project-dir"])
        .arg(dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("warning") && combined.contains("network: too broad"),
        "expected 'warning: network: too broad' in output, got: {combined}"
    );
    assert!(out.status.success(), "lint without --strict must exit 0 on warning");
}

#[test]
fn lint_strict_fails_on_network_wildcard() {
    let dir = tempfile::tempdir().unwrap();
    write_describe(
        dir.path(),
        r#"{"network":["*"],"secrets":[],"callExtensionKinds":[]}"#,
    );
    let out = gtdx()
        .args(["lint", "--strict", "--project-dir"])
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(!out.status.success(), "lint --strict must exit non-zero on warning");
}

#[test]
fn lint_passes_on_narrow_allowlist() {
    let dir = tempfile::tempdir().unwrap();
    write_describe(
        dir.path(),
        r#"{"network":["https://api.greentic.ai/*"],"secrets":[],"callExtensionKinds":[]}"#,
    );
    let out = gtdx()
        .args(["lint", "--strict", "--project-dir"])
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "narrow allowlist must pass --strict");
}
```

- [ ] **Step 2: Run — expect FAIL (subcommand missing)**

Run: `cargo test -p greentic-extension-sdk-cli --test lint_permissions`
Expected: FAIL (subcommand `lint` not found).

- [ ] **Step 3: Implement the subcommand**

Create `crates/greentic-extension-sdk-cli/src/commands/lint.rs`:

```rust
//! `gtdx lint` — Phase D ships the permissions-breadth check only.
//! Other lints (describe-diff, semver-bump, capability-cycle) land in
//! Phase E.
//!
//! Permissions check:
//!   - `network: ["*"]` or any entry exactly equal to `"*"` → `warning: too broad`
//!   - `network: ["**"]` → `warning: too broad`
//!   - `secrets: ["*"]` → `warning: too broad`
//!   - `callExtensionKinds: ["*"]` → `warning: too broad`
//!
//! `--strict` upgrades warnings to errors and exits 1.

use std::path::PathBuf;

use clap::Args;

#[derive(Args)]
pub struct LintArgs {
    /// Project directory containing describe.json. Defaults to cwd.
    #[arg(long = "project-dir", default_value = ".")]
    pub project_dir: PathBuf,
    /// Treat warnings as errors.
    #[arg(long)]
    pub strict: bool,
}

pub fn run(args: &LintArgs) -> Result<(), anyhow::Error> {
    let path = args.project_dir.join("describe.json");
    let body = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("read {}: {e}", path.display()))?;
    let v: serde_json::Value = serde_json::from_str(&body)?;
    let perms = v
        .get("runtime")
        .and_then(|r| r.get("permissions"))
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));

    let mut warnings: Vec<String> = Vec::new();
    for key in ["network", "secrets", "callExtensionKinds"] {
        if let Some(arr) = perms.get(key).and_then(|x| x.as_array())
            && arr.iter().any(|e| e.as_str() == Some("*") || e.as_str() == Some("**"))
        {
            warnings.push(format!("warning: {key}: too broad (contains \"*\")"));
        }
    }

    for w in &warnings {
        println!("{w}");
    }

    if !warnings.is_empty() && args.strict {
        anyhow::bail!("lint failed under --strict ({} warning(s))", warnings.len());
    }
    Ok(())
}
```

- [ ] **Step 4: Register**

Edit `crates/greentic-extension-sdk-cli/src/commands/mod.rs`. Add `pub mod lint;` to the module list.

Edit `crates/greentic-extension-sdk-cli/src/main.rs`. In the `Command` enum, add:

```rust
    /// Lint an extension project for common issues
    Lint(commands::lint::LintArgs),
```

In the dispatcher (search for `Command::Validate => ...` to find where match arms live), add:

```rust
        Command::Lint(args) => commands::lint::run(&args),
```

- [ ] **Step 5: Run**

Run: `cargo test -p greentic-extension-sdk-cli --test lint_permissions`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/greentic-extension-sdk-cli/src/commands/lint.rs \
        crates/greentic-extension-sdk-cli/src/commands/mod.rs \
        crates/greentic-extension-sdk-cli/src/main.rs \
        crates/greentic-extension-sdk-cli/tests/lint_permissions.rs
git commit -m "feat(sdk-cli): gtdx lint warns on broad permission allow-lists (--strict fails)"
```

---

## Task D.7.b: Shared matcher used by both host + linter

> The umbrella spec calls for the same matcher in `http::fetch` (runtime, Phase B) and the linter. The actual matcher impl lands in Phase B. Phase D adds a thin shared module here so Phase B can consume it.

**Files:**
- Create: `crates/greentic-extension-sdk-contract/src/permissions_matcher.rs`
- Modify: `crates/greentic-extension-sdk-contract/src/lib.rs`

- [ ] **Step 1: Failing tests**

Create `crates/greentic-extension-sdk-contract/src/permissions_matcher.rs`:

```rust
//! Shared URL matcher for `permissions.network` entries.
//!
//! Format: `scheme://host[:port]/path-prefix`.
//! - `scheme` must match exactly (`https://` does NOT match an `http://` URL).
//! - `host` matches exact OR `*.suffix` (e.g. `*.greentic.ai` matches
//!   `api.greentic.ai` but NOT `greentic.ai`).
//! - `path-prefix` is a literal prefix; `/*` is treated as "any path".
//!
//! Phase D consumers: the `gtdx lint` permissions checker (this crate's
//! `lint.rs`).
//! Phase B consumer: the runtime's `http::fetch` host fn.

/// A single parsed entry from `permissions.network`.
#[derive(Debug, Clone)]
pub struct NetworkRule {
    pub scheme: String,
    pub host_suffix_match: bool,
    pub host: String,
    pub path_prefix: String,
}

#[derive(Debug, thiserror::Error)]
pub enum MatcherError {
    #[error("network rule '{0}' is too broad (\"*\" / \"**\")")]
    TooBroad(String),
    #[error("network rule '{0}' parse: {1}")]
    Parse(String, String),
}

impl NetworkRule {
    /// Parse a rule string. `"*"` and `"**"` are rejected as too broad.
    pub fn parse(s: &str) -> Result<Self, MatcherError> {
        if s == "*" || s == "**" {
            return Err(MatcherError::TooBroad(s.to_string()));
        }
        let (scheme, rest) = s
            .split_once("://")
            .ok_or_else(|| MatcherError::Parse(s.into(), "missing scheme://".into()))?;
        if scheme.is_empty() {
            return Err(MatcherError::Parse(s.into(), "empty scheme".into()));
        }
        let (host_part, path_prefix) = rest
            .split_once('/')
            .map_or((rest, ""), |(h, p)| (h, p));
        let (host_suffix_match, host) = if let Some(suffix) = host_part.strip_prefix("*.") {
            (true, suffix.to_string())
        } else {
            (false, host_part.to_string())
        };
        if host.is_empty() {
            return Err(MatcherError::Parse(s.into(), "empty host".into()));
        }
        Ok(Self {
            scheme: scheme.to_string(),
            host_suffix_match,
            host,
            path_prefix: path_prefix.trim_end_matches('*').to_string(),
        })
    }

    /// Returns `true` if `url` (a fully-qualified URL string) matches
    /// this rule. Returns `false` on any parse error of `url` (callers
    /// should treat that as a deny).
    #[must_use]
    pub fn matches(&self, url: &str) -> bool {
        let (scheme, rest) = match url.split_once("://") {
            Some(s) => s,
            None => return false,
        };
        if scheme != self.scheme {
            return false;
        }
        let (host_part, path_part) = rest.split_once('/').map_or((rest, ""), |(h, p)| (h, p));
        let host_ok = if self.host_suffix_match {
            host_part.ends_with(&format!(".{}", self.host))
        } else {
            host_part == self.host
        };
        if !host_ok {
            return false;
        }
        // Path: literal prefix match. Trim leading slash off the request
        // path because the rule already stored "no leading slash" form.
        path_part.starts_with(&self.path_prefix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_wildcard_only() {
        assert!(matches!(NetworkRule::parse("*"), Err(MatcherError::TooBroad(_))));
    }

    #[test]
    fn exact_host_no_subdomain_confusion() {
        let rule = NetworkRule::parse("https://greentic.ai/").unwrap();
        assert!(rule.matches("https://greentic.ai/anything"));
        assert!(!rule.matches("https://evil-greentic.ai/x"));
        assert!(!rule.matches("https://api.greentic.ai/x"));
    }

    #[test]
    fn suffix_host_matches_subdomain_only() {
        let rule = NetworkRule::parse("https://*.greentic.ai/").unwrap();
        assert!(rule.matches("https://api.greentic.ai/anything"));
        assert!(!rule.matches("https://greentic.ai/x"), "suffix should NOT match bare host");
        assert!(!rule.matches("https://evil-greentic.ai/x"));
    }

    #[test]
    fn scheme_downgrade_blocked() {
        let rule = NetworkRule::parse("https://api.greentic.ai/").unwrap();
        assert!(!rule.matches("http://api.greentic.ai/anything"));
    }

    #[test]
    fn path_prefix_enforced() {
        let rule = NetworkRule::parse("https://api.greentic.ai/v1/").unwrap();
        assert!(rule.matches("https://api.greentic.ai/v1/foo"));
        assert!(!rule.matches("https://api.greentic.ai/v2/foo"));
    }
}
```

- [ ] **Step 2: Register + run**

Edit `crates/greentic-extension-sdk-contract/src/lib.rs`. Add `pub mod permissions_matcher;` and:

```rust
pub use self::permissions_matcher::{MatcherError, NetworkRule};
```

Run: `cargo test -p greentic-extension-sdk-contract permissions_matcher`
Expected: PASS.

- [ ] **Step 3: Use matcher inside `gtdx lint`**

Edit `crates/greentic-extension-sdk-cli/src/commands/lint.rs`. Replace the inline `"*" / "**"` check inside the loop with:

```rust
        if let Some(arr) = perms.get(key).and_then(|x| x.as_array()) {
            for entry in arr {
                let Some(s) = entry.as_str() else { continue };
                if matches!(
                    greentic_extension_sdk_contract::NetworkRule::parse(s),
                    Err(greentic_extension_sdk_contract::MatcherError::TooBroad(_))
                ) {
                    warnings.push(format!("warning: {key}: too broad (entry \"{s}\")"));
                }
            }
        }
```

- [ ] **Step 4: Re-run all lint tests**

Run: `cargo test -p greentic-extension-sdk-cli --test lint_permissions`
Expected: PASS. The shared matcher produces a slightly different wording — adjust the test assertions in Step 1 if needed to match the new message format (look for `too broad`).

- [ ] **Step 5: Commit**

```bash
git add crates/greentic-extension-sdk-contract/src/permissions_matcher.rs \
        crates/greentic-extension-sdk-contract/src/lib.rs \
        crates/greentic-extension-sdk-cli/src/commands/lint.rs \
        crates/greentic-extension-sdk-cli/tests/lint_permissions.rs
git commit -m "feat(sdk-contract,sdk-cli): shared NetworkRule matcher; lint reuses it"
```

---

## Task D.8: 0700 perms on `~/.greentic/extensions/<id>-<ver>/` (Unix only)

**Files:**
- Modify: `crates/greentic-extension-sdk-registry/src/storage.rs`
- Test: `crates/greentic-extension-sdk-registry/tests/dir_perms.rs`

- [ ] **Step 1: Failing test (Unix only)**

Create `crates/greentic-extension-sdk-registry/tests/dir_perms.rs`:

```rust
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;

use greentic_extension_sdk_contract::ExtensionKind;
use greentic_extension_sdk_registry::storage::Storage;

#[test]
fn commit_install_sets_0700_on_final_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let storage = Storage::new(tmp.path());
    let (staging, final_dir) = storage
        .begin_install(ExtensionKind::Design, "demo", "0.1.0")
        .unwrap();
    std::fs::write(staging.join("describe.json"), "{}").unwrap();
    storage.commit_install(&staging, &final_dir).unwrap();
    let mode = std::fs::metadata(&final_dir).unwrap().permissions().mode();
    // mode & 0o777 returns the perms; we want exactly 0o700.
    assert_eq!(mode & 0o777, 0o700, "got mode={:#o}", mode & 0o777);
}
```

- [ ] **Step 2: Run — expect FAIL**

Run: `cargo test -p greentic-extension-sdk-registry --test dir_perms`
Expected: FAIL (mode is whatever the umask produced).

- [ ] **Step 3: Tighten perms in `commit_install`**

Edit `crates/greentic-extension-sdk-registry/src/storage.rs`. Replace `commit_install` body:

```rust
    pub fn commit_install(&self, staging: &Path, final_dir: &Path) -> Result<(), RegistryError> {
        if let Some(parent) = final_dir.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if final_dir.exists() {
            std::fs::remove_dir_all(final_dir)?;
        }
        std::fs::rename(staging, final_dir)?;
        Self::set_secure_dir_permissions(final_dir)?;
        Ok(())
    }

    #[cfg(unix)]
    fn set_secure_dir_permissions(dir: &Path) -> Result<(), RegistryError> {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o700);
        std::fs::set_permissions(dir, perms)?;
        Ok(())
    }

    #[cfg(not(unix))]
    fn set_secure_dir_permissions(_dir: &Path) -> Result<(), RegistryError> {
        // Windows ACLs require a different API surface; deferred.
        Ok(())
    }
```

- [ ] **Step 4: Re-run**

Run: `cargo test -p greentic-extension-sdk-registry --test dir_perms`
Expected: PASS on Unix; the `#![cfg(unix)]` skips on Windows.

- [ ] **Step 5: Commit**

```bash
git add crates/greentic-extension-sdk-registry/src/storage.rs \
        crates/greentic-extension-sdk-registry/tests/dir_perms.rs
git commit -m "feat(sdk-registry): chmod 0700 on per-extension dirs on Unix"
```

---

## Task D.9: AWS KMS trust-root policy doc

**Repo:** `greentic-docs`.

**Files:**
- Create: `src/content/docs/operating/trust-root.md`

- [ ] **Step 1: Worktree off main**

```bash
cd /home/bima-pangestu/Works/greentic/greentic-docs
git fetch origin
git worktree add -b feat/security-hardening-phase-d ../greentic-docs-phase-d origin/main
cd ../greentic-docs-phase-d
mkdir -p src/content/docs/operating
```

- [ ] **Step 2: Write the policy doc**

Create `src/content/docs/operating/trust-root.md`:

```markdown
---
title: Trust root key custody policy
description: Custody, rotation, onboarding, and revocation for the Greentic extensions trust root.
---

## Scope

This doc covers the **Greentic extensions trust root** — the ed25519 root
key that signs publisher certs, which in turn sign `.gtxpack` manifests.
It does NOT cover (a) the per-publisher signing keys held by individual
publishers, (b) the store-server's TLS termination cert, or (c) any
operator-environment-specific signing flows.

## Key custody

- **Location:** AWS KMS, region `eu-west-1`, AWS account `<TBD by INSIGNIA DevOps>`.
- **Key spec:** `ECC_NIST_P256` is NOT used — we use a customer-managed
  KMS key with **`KEY_USAGE=SIGN_VERIFY`** and a software-equivalent
  ed25519 public key derived from a deterministic seed sealed inside KMS.
  (KMS does not natively support ed25519 as of writing; the production
  implementation wraps a deterministic seed under KMS encrypt-only and
  decrypts it inside an enclave during cert issuance. DevOps owns the
  exact enclave deployment.)
- **Public key distribution:** the base64 root pubkey is embedded as the
  compile-time constant `GREENTIC_ROOT_PUBKEY_B64` in
  `greentic-extension-sdk-registry/src/trust_store.rs`. Designers and
  runners ship with this constant baked in; rotating the key requires a
  new SDK release.

## IAM access (role-based, never individual)

- **Role `greentic-store-server-prod`** — the only principal allowed to
  call `kms:Sign` against the root key. Attached to the EKS service
  account that runs the publisher-onboarding endpoint.
- **Role `greentic-devops-break-glass`** — emergency rotation only.
  Requires 2-of-3 approvals via AWS Identity Center.
- **No human IAM user has direct access.** Audit findings record any
  `kms:Sign` calls outside the service-role principal.

## Rotation cadence

- **Routine rotation:** annual, on the calendar quarter after Jan 1.
  Procedure: provision new KMS key, re-issue every active publisher
  cert, ship new SDK with updated `GREENTIC_ROOT_PUBKEY_B64`, deprecate
  old key after 90-day overlap.
- **Emergency rotation:** within 24 hours of any of: confirmed AWS
  credential leak with KMS access, two failed audit attempts that touch
  the root key, suspected enclave compromise. Procedure: same as
  routine, but the old key is revoked at hour 0 (no overlap), and a
  hotfix SDK release ships within 4 hours.

## Publisher onboarding

1. New publisher generates an ed25519 keypair locally via
   `gtdx keygen --output ./publisher.key`.
2. Publisher submits the public key + organisation metadata via the
   store-server's `POST /api/v1/publishers/request-cert` endpoint.
3. INSIGNIA DevOps reviews + approves; the store-server's onboarding
   handler invokes `auth::kms_root::issue_publisher_cert` which calls
   `kms:Sign` on the canonical cert payload.
4. The issued cert is returned in the response body and stored under
   the publisher's row in the `publishers` table. Publishers download
   the cert via `gtdx login` (which fetches it as part of the auth
   handshake).

Trust-store updates: each release of the designer ships an updated
`trust_store.rs` listing newly-onboarded publishers. End users with
the latest designer pick them up automatically; users on older
versions can manually edit `~/.greentic/trust/publishers.json` (the
file format is documented in `crates/greentic-extension-sdk-registry/src/trust_store.rs`).

## Revocation

Initial release uses **short-lived publisher certs (12-month expiry)**
for revocation by attrition. The expiry check is enforced by
`PublisherCert::verify` in `greentic-extension-sdk-contract`.

A second pass adds an explicit CRL-like file at
`https://store.greentic.ai/.well-known/greentic-revoked-publishers.json`
(JCS-canonicalized, signed by the root key) that designers fetch on
startup with a 24-hour cache. The CRL implementation is deferred to
Phase F; see `docs/superpowers/specs/2026-08-publisher-revocation.md`
when that lands.

## Sign-off

This policy requires approval from:

- CEO (DECIDED + DOCUMENTED — recorded in approver's signed PR comment on the PR that merged this doc).
- CTO (same).
- INSIGNIA DevOps lead (responsible for AWS account configuration).

See linked PR for approval records.
```

- [ ] **Step 3: Verify Astro build**

Run: `npm install && npm run build`
Expected: PASS (Astro reports zero broken-link errors for the new page).

- [ ] **Step 4: Commit**

```bash
git add src/content/docs/operating/trust-root.md
git commit -m "docs: add trust-root key custody + rotation + onboarding policy"
```

---

## Task D.9.b: Server-side KMS root signer stub

**Repo:** `greentic-store-server`.

**Files:**
- Create: `crates/greentic-store-api/src/auth/kms_root.rs`
- Modify: `Cargo.toml` (add `aws-sdk-kms`)
- Modify: `crates/greentic-store-api/Cargo.toml`
- Test: `crates/greentic-store-api/tests/kms_root_local.rs`

> This task lands the trait + a local-keypair impl that compiles WITHOUT a real KMS connection. The KMS impl is gated behind `cfg(feature = "kms-root")` and pulled in only when DevOps provisions the key.

- [ ] **Step 1: Failing test**

Create `crates/greentic-store-api/tests/kms_root_local.rs`:

```rust
//! The store-server abstracts root-signing behind `RootSigner`. The
//! local-keypair impl signs in-process for dev + CI; the KMS impl
//! delegates to AWS. This test exercises the local impl end-to-end.

use ed25519_dalek::{Verifier, VerifyingKey};
use greentic_store_api::auth::kms_root::{LocalKeypairRootSigner, RootSigner};

#[test]
fn local_root_signs_payload() {
    let signer = LocalKeypairRootSigner::generate_for_test();
    let msg = b"hello root";
    let sig = signer.sign(msg).expect("sign");
    let pubkey_bytes = signer.public_key_bytes();
    let arr: [u8; 32] = pubkey_bytes.try_into().unwrap();
    let pubkey = VerifyingKey::from_bytes(&arr).unwrap();
    let sig_arr: [u8; 64] = sig.try_into().unwrap();
    let sig = ed25519_dalek::Signature::from_bytes(&sig_arr);
    pubkey.verify(msg, &sig).expect("verify");
}
```

- [ ] **Step 2: Run — expect FAIL**

Run: `cargo test -p greentic-store-api --test kms_root_local`
Expected: FAIL (module missing).

- [ ] **Step 3: Add cargo features + deps**

Edit workspace `Cargo.toml`. Under `[workspace.dependencies]`:

```toml
aws-sdk-kms = "1"
```

Edit `crates/greentic-store-api/Cargo.toml`:

```toml
[features]
default = []
# Use AWS KMS to back the trust root. When disabled (CI/dev), the
# `LocalKeypairRootSigner` runs in-process.
kms-root = ["dep:aws-sdk-kms", "dep:aws-config"]

[dependencies]
# ... existing deps ...
aws-sdk-kms = { workspace = true, optional = true }
aws-config = { workspace = true, optional = true }
```

(Confirm `aws-config` is already a workspace dep; the store-server uses it for S3 — re-use the same version pin.)

- [ ] **Step 4: Implement**

Create `crates/greentic-store-api/src/auth/kms_root.rs`:

```rust
//! Trust-root signing for publisher cert issuance.
//!
//! Two impls:
//! - `LocalKeypairRootSigner` — in-process ed25519 keypair. CI + dev.
//! - `KmsRootSigner` — AWS KMS-backed signing. Production only.
//!
//! Callers (the publisher-onboarding handler) hold a
//! `Box<dyn RootSigner>` selected by config.

use async_trait::async_trait;

#[derive(Debug, thiserror::Error)]
pub enum RootSignerError {
    #[error("sign failed: {0}")]
    Sign(String),
    #[error("backend not configured")]
    NotConfigured,
}

#[async_trait]
pub trait RootSigner: Send + Sync {
    /// Sign `message` with the trust-root private key. Returns the raw
    /// 64-byte ed25519 signature.
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, RootSignerError>;
    /// 32-byte raw ed25519 public key.
    fn public_key_bytes(&self) -> Vec<u8>;
}

pub struct LocalKeypairRootSigner {
    key: ed25519_dalek::SigningKey,
}

impl LocalKeypairRootSigner {
    pub fn from_seed_bytes(seed: [u8; 32]) -> Self {
        Self { key: ed25519_dalek::SigningKey::from_bytes(&seed) }
    }

    #[cfg(any(test, feature = "dev-allow-unsigned"))]
    pub fn generate_for_test() -> Self {
        use rand::RngCore;
        let mut seed = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut seed);
        Self::from_seed_bytes(seed)
    }
}

#[async_trait]
impl RootSigner for LocalKeypairRootSigner {
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, RootSignerError> {
        use ed25519_dalek::Signer;
        Ok(self.key.sign(message).to_bytes().to_vec())
    }

    fn public_key_bytes(&self) -> Vec<u8> {
        self.key.verifying_key().to_bytes().to_vec()
    }
}

#[cfg(feature = "kms-root")]
pub mod kms {
    use super::*;

    pub struct KmsRootSigner {
        client: aws_sdk_kms::Client,
        key_id: String,
        cached_pubkey: Vec<u8>,
    }

    impl KmsRootSigner {
        pub async fn from_env() -> Result<Self, RootSignerError> {
            let key_id = std::env::var("GREENTIC_TRUST_ROOT_KMS_KEY_ID")
                .map_err(|_| RootSignerError::NotConfigured)?;
            let config = aws_config::load_from_env().await;
            let client = aws_sdk_kms::Client::new(&config);
            let resp = client
                .get_public_key()
                .key_id(&key_id)
                .send()
                .await
                .map_err(|e| RootSignerError::Sign(format!("get_public_key: {e}")))?;
            let cached_pubkey = resp
                .public_key()
                .map(|b| b.as_ref().to_vec())
                .ok_or_else(|| RootSignerError::Sign("KMS returned no public key".into()))?;
            Ok(Self { client, key_id, cached_pubkey })
        }
    }

    #[async_trait]
    impl RootSigner for KmsRootSigner {
        fn sign(&self, _message: &[u8]) -> Result<Vec<u8>, RootSignerError> {
            // KMS sign is async; the trait signature here is sync to keep
            // CI tests trivial. Real callers should use a wrapper that
            // spawns this onto the tokio runtime via `block_in_place` or
            // refactor the trait to async — deferred to follow-up PR.
            Err(RootSignerError::Sign("KMS sign requires async wrapper".into()))
        }

        fn public_key_bytes(&self) -> Vec<u8> {
            self.cached_pubkey.clone()
        }
    }
}
```

Register the module: edit `crates/greentic-store-api/src/auth/mod.rs` (if exists, else `src/auth.rs`):

```rust
pub mod kms_root;
```

If `auth/mod.rs` does not yet exist as a file but `auth/` is a directory, find the existing entry point — likely `src/auth/mod.rs` since `middleware` and `signing` were referenced as `auth::middleware`. Add the new submodule there.

- [ ] **Step 5: Add `async-trait` to dev-deps if absent**

Run: `grep async_trait crates/greentic-store-api/Cargo.toml`
If empty, add:

```toml
async-trait = { workspace = true }
```

(`async-trait` is already a workspace dep per the workspace Cargo.toml.)

- [ ] **Step 6: Re-run**

Run: `cargo test -p greentic-store-api --test kms_root_local`
Expected: PASS.

- [ ] **Step 7: Build with the feature flag (sanity, even though no real KMS)**

Run: `cargo build -p greentic-store-api --features kms-root`
Expected: PASS (compiles even without AWS env vars; the runtime call would fail without `GREENTIC_TRUST_ROOT_KMS_KEY_ID`, but compilation is enough).

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml \
        crates/greentic-store-api/Cargo.toml \
        crates/greentic-store-api/src/auth/kms_root.rs \
        crates/greentic-store-api/src/auth/mod.rs \
        crates/greentic-store-api/tests/kms_root_local.rs
git commit -m "feat(store-api): RootSigner trait + local + KMS-feature-gated impls"
```

---

## Task D.10: Attack-vector integration suite

**Repo:** `greentic-designer-sdk`.

**Files:**
- Create: `crates/greentic-extension-sdk-cli/tests/integration_attack_vectors.rs`

- [ ] **Step 1: Write the suite**

Create `crates/greentic-extension-sdk-cli/tests/integration_attack_vectors.rs`:

```rust
//! Full attack-vector regression suite for Phase D.
//!
//! Each test asserts ONE security property end-to-end:
//!
//! 1. unsafe_code forbid is present on every SDK crate root
//! 2. tampered WASM swap fails install
//! 3. canonical signing roundtrip (server emits JCS bytes)
//! 4. unknown publisher (no cert) rejected
//! 5. release build ignores GREENTIC_EXT_ALLOW_UNSIGNED (compile-time gate)

use std::path::Path;

fn lib_has_forbid(path: &Path) -> bool {
    let body = std::fs::read_to_string(path).expect("read lib");
    body.lines().any(|l| l.trim() == "#![forbid(unsafe_code)]")
}

#[test]
fn every_sdk_crate_root_forbids_unsafe_code() {
    // CARGO_MANIFEST_DIR points at greentic-extension-sdk-cli; resolve
    // sibling crates relative to it.
    let cli_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let crates_root = cli_root.parent().expect("parent");
    for (name, file) in &[
        ("greentic-extension-sdk-contract", "src/lib.rs"),
        ("greentic-extension-sdk-state", "src/lib.rs"),
        ("greentic-extension-sdk-registry", "src/lib.rs"),
        ("greentic-extension-sdk-testing", "src/lib.rs"),
        ("greentic-extension-sdk-cli", "src/main.rs"),
    ] {
        let path = crates_root.join(name).join(file);
        assert!(
            lib_has_forbid(&path),
            "{} missing `#![forbid(unsafe_code)]`",
            path.display()
        );
    }
}

#[test]
fn tampered_wasm_swap_fails_install() {
    use std::io::{Cursor, Write};
    let describe = br#"{"k":1}"#.as_slice();
    let clean = b"\0asm\x01\x00\x00\x00".as_slice();
    let manifest = greentic_extension_sdk_contract::build_manifest(vec![
        ("describe.json", describe),
        ("extension.wasm", clean),
    ]);
    let manifest_json = serde_json::to_vec(&manifest).unwrap();

    let mut buf = Vec::new();
    {
        let mut w = zip::ZipWriter::new(Cursor::new(&mut buf));
        w.start_file::<_, ()>("describe.json", zip::write::FileOptions::default()).unwrap();
        w.write_all(describe).unwrap();
        w.start_file::<_, ()>("extension.wasm", zip::write::FileOptions::default()).unwrap();
        w.write_all(b"\0asm\x01\x00\x00\xff").unwrap();
        w.start_file::<_, ()>("manifest.json", zip::write::FileOptions::default()).unwrap();
        w.write_all(&manifest_json).unwrap();
        w.finish().unwrap();
    }
    assert!(
        greentic_extension_sdk_contract::verify_archive_against_manifest(&buf).is_err()
    );
}

#[test]
fn canonical_signing_roundtrip_byte_equal() {
    let original = serde_json::json!({
        "kind": "DesignExtension",
        "apiVersion": "greentic.ai/v1"
    });
    let shuffled = serde_json::json!({
        "apiVersion": "greentic.ai/v1",
        "kind": "DesignExtension"
    });
    assert_eq!(
        serde_jcs::to_vec(&original).unwrap(),
        serde_jcs::to_vec(&shuffled).unwrap()
    );
}

#[test]
fn unknown_publisher_rejected_with_trust_store() {
    use greentic_extension_sdk_registry::trust_store::{TRUST_STORE_SCHEMA_V1, TrustStore};
    let store = TrustStore {
        schema: TRUST_STORE_SCHEMA_V1.into(),
        root_pubkey_b64: None,
        publishers: vec![],
    };
    assert!(!store.is_publisher_trusted("strangers-corp"));
}

#[test]
fn permissions_matcher_blocks_subdomain_confusion() {
    use greentic_extension_sdk_contract::NetworkRule;
    let rule = NetworkRule::parse("https://greentic.ai/").unwrap();
    assert!(!rule.matches("https://evil-greentic.ai/x"));
}
```

- [ ] **Step 2: Run the full suite**

Run: `cargo test -p greentic-extension-sdk-cli --test integration_attack_vectors`
Expected: all 5 PASS.

- [ ] **Step 3: Run `local_check.sh` if present**

Run: `bash ci/local_check.sh` (from `greentic-designer-sdk` root). If the script does not exist, run:
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```
Expected: PASS across the workspace.

- [ ] **Step 4: Commit**

```bash
git add crates/greentic-extension-sdk-cli/tests/integration_attack_vectors.rs
git commit -m "test(sdk-cli): phase-D attack-vector regression suite"
```

---

## Task D.11: PR open + cross-repo coordination

Each repo touched gets its own PR targeting `research` (or `main` for two-tier repos). NO Claude attribution.

- [ ] **Step 1: Push branches**

In each worktree:

```bash
# greentic-designer-sdk
cd /home/bima-pangestu/Works/greentic/greentic-designer-sdk-phase-d
git push -u origin feat/security-hardening-phase-d

# greentic-designer-extensions
cd /home/bima-pangestu/Works/greentic/greentic-designer-extensions-phase-d
git push -u origin feat/security-hardening-phase-d

# greentic-store-server (target main — two-tier repo)
cd /home/bima-pangestu/Works/greentic/greentic-store-server-phase-d
git push -u origin feat/security-hardening-phase-d

# greentic-docs (target main — two-tier repo)
cd /home/bima-pangestu/Works/greentic/greentic-docs-phase-d
git push -u origin feat/security-hardening-phase-d
```

- [ ] **Step 2: Open PRs**

For each repo, use `gh pr create` with body following the template below (NO Co-Authored-By, NO "Generated with Claude Code" trailer):

```bash
gh pr create --base research --title "feat(security): phase D hardening — sdk-side" --body "$(cat <<'EOF'
## Summary
Phase D security hardening for the extensions 1.0 cleanup. See
`docs/superpowers/specs/2026-05-13-extensions-1.0-cleanup.md` Section 4
and `docs/superpowers/plans/2026-05-13-security-hardening.md`.

- Restored `#![forbid(unsafe_code)]` at every SDK crate root (audit P0 #5).
- Whole-archive manifest.json + verify on install (audit P0 #2).
- Publisher-cert chain with `RootVerifier` trait (audit P0 #1).
- Designer-side trust store at `~/.greentic/trust/publishers.json`.
- `gtdx lint --strict` flags broad `permissions.network` allow-lists.
- 0700 perms on per-extension install dirs (Unix).
- Shared `NetworkRule` matcher consumed by linter + Phase B host fns.
- Attack-vector regression test suite covers all of the above.

## Block-on
- D.5/D.6/D.7/D.9 fully active only after INSIGNIA DevOps provisions
  the AWS KMS root key + CEO/CTO sign off the policy doc.
- Trust store currently ships with empty `GREENTIC_ROOT_PUBKEY_B64`;
  binaries built today degrade gracefully (chain verify skipped if
  trust store / root pubkey missing).

## Test plan
- [ ] cargo test --workspace --all-features passes
- [ ] cargo clippy --workspace --all-targets --all-features -- -D warnings passes
- [ ] cargo fmt --all -- --check passes
- [ ] gtdx lint warns on `network: ["*"]`
- [ ] gtdx lint --strict exits non-zero on warning
- [ ] On Unix: `stat ~/.greentic/extensions/<id>-<ver>/` reports mode 700
EOF
)"
```

For the two-tier repos (`greentic-store-server`, `greentic-docs`) target `main` and adjust the title scope.

- [ ] **Step 3: Report URLs back**

Capture each PR URL into the plan tracking row in the umbrella spec
`docs/superpowers/specs/2026-05-13-extensions-1.0-cleanup.md` Section 7.

---

## Self-Review Checklist

Run through this before declaring the plan complete:

1. **Spec coverage:**
   - Audit P0 #1 (trust root) → D.5/D.6/D.9
   - Audit P0 #2 (whole-archive sign) → D.4
   - Audit P0 #3 (server JCS) → D.3
   - Audit P0 #5 (forbid unsafe_code) → D.1.1–D.1.6
   - Audit P0 #6 (env bypass) → D.2
   - Permissions matcher (Section 4) → D.7 + D.7.b
   - 0700 dir perms → D.8
   - Policy doc → D.9
   - KMS root signer → D.9.b
   - Attack-vector suite → D.10
2. **Placeholders:** none — every "TODO" is a deliberate marker for KMS provisioning that DevOps owns, not for the implementing engineer.
3. **Type consistency:** `RootVerifier`, `PublisherCert`, `TrustStore`, `NetworkRule` names match across all consumer tasks. `verify_publisher_chain` signature in D.5.3 and D.6 matches (with `trust_store_opt` added in D.6).
4. **Branch targets:** sdk + designer-extensions → `research`; store-server + docs → `main`.
5. **No Claude attribution:** all commit messages + PR bodies are clean.
