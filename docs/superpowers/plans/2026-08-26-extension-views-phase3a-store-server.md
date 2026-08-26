# Extension Views — Phase 3a (store-server) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a view-bearing extension be published, and let the Admin console fetch its view assets in one request per version.

**Architecture:** Two independent changes to `greentic-store-server`. First, stop keeping a hand-maintained copy of the describe schema and consume the one embedded in `greentic-extension-sdk-contract`, so publish-time validation cannot drift from what the SDK produces. Second, add a bulk `view-assets` endpoint that opens the stored `.gtxpack` blob and returns only `assets/views/**` plus a manifest.

**Tech Stack:** Rust, Axum, `sqlx` (sqlite feature), `zip`, `aws_sdk_s3` via the `greentic-store-blob` crate, `jsonschema`.

**Spec:** `docs/superpowers/specs/2026-08-26-extension-views-phase3-admin-design.md` (this repo). Read its "Prerequisites" and "Asset path → Store-server contract" sections; the rest is Admin-side context.

## Global Constraints

- Work in `greentic-store-server`. Determine its integration branch from the remote before branching — it is **not** necessarily `main` (the working checkout sits on `develop`).
- Packs are stored whole as one blob per version, keyed `{describe_id}/{describe_version}/{signed_sha}.gtxpack`. Nothing is unpacked at publish. Reading one entry means: `state.blob.get(artifact_key)` → `zip::ZipArchive::new(Cursor::new(bytes))` → `by_name(...)`.
- The established precedent for that is `crates/greentic-store-api/src/handlers/agentic_workers/pack_files.rs`. Follow its shape. **There is no icon route in this repo** — do not look for one as precedent.
- Read routes for published extensions are public and anonymous, and say so explicitly in `handlers/extensions/artifact.rs`. They call `handlers::gate::ensure_entitled`, which only bites for paid listings.
- `MAX_ARTIFACT_BYTES` is 100 MiB and the router carries a matching `DefaultBodyLimit`. There is no per-file cap inside an archive and no ETag or `Cache-Control` on any blob route.
- The server never verifies `manifestSha256` — it only checks the field is 64 hex characters. Do not add verification here; Admin verifies what it receives. Changing that is a separate decision.
- No `unwrap()` / `panic!()` in library code. Tests may.
- Conventional commits.
- Never run `git stash` — this machine runs many sessions sharing stash stacks.

---

### Task 1: Consume the SDK's schema instead of a copy

**Files:**
- Modify: `crates/greentic-store-api/Cargo.toml` (add the contract dependency)
- Modify: `crates/greentic-store-api/src/publish/schema.rs`
- Delete: `schemas/describe-v2.json` (the drifted copy), once nothing reads it
- Test: `crates/greentic-store-api/src/publish/schema.rs` tests, or the existing publish tests

**Interfaces:**
- Consumes: `greentic_extension_sdk_contract::schema::validate_describe_json` (and/or `validate_describe_v2`), which embeds the canonical schema via `include_str!`.
- Produces: no new public API. `schema::validate` keeps its current signature so `publish.rs` is untouched.

A separate PR (`greentic-biz/greentic-store-server#170`) adds `views` and `permissions.ui` to the copy as an immediate unblock. This task removes the copy so the problem cannot recur. If that PR has landed, this task deletes what it patched; if it has not, this task supersedes it — check before starting and say which case you are in.

**The drift is worse than `views`, which is the argument for deleting the copy rather than patching it again.** #170 also found:

- `contributions.properties` is missing `guardrails` and `connection_test` too. Both are shipped SDK features, so a guardrail-contributing extension cannot be published today either, by the same mechanism and with the same silence.
- The `kind` enum here is *looser* than the SDK's, permitting `AgenticWorker` and `ComponentExtension`, which the SDK's enum does not carry. Validating more loosely is the failure mode nobody notices: it admits packs the toolchain itself rejects.
- Nearly all `description` text is stripped relative to the SDK's schema, and `schema.rs`'s own doc comment still cites SDK `1.2.0-research`.

Patching a copy that has drifted in four directions at once, one direction at a time, is how it got here.

- [ ] **Step 1: Write the failing test**

Add to `crates/greentic-store-api/src/publish/schema.rs`:

```rust
#[cfg(test)]
mod views_tests {
    use super::*;

    fn describe_with_views() -> serde_json::Value {
        serde_json::json!({
            "apiVersion": "greentic.ai/v2",
            "kind": "DesignExtension",
            "compat": {
                "min_designer_version": ">=1.2.0",
                "min_runner_version": "^1.2.0",
                "contract_version": "1.2.10"
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
                "permissions": {
                    "network": [], "secrets": [], "callExtensionKinds": [],
                    "ui": {
                        "fetchHosts": ["https://api.example.com/*"],
                        "platformApi": [{ "method": "GET", "path_pattern": "/api/flows" }]
                    }
                }
            },
            "contributions": {
                "views": [{
                    "id": "usage",
                    "surface": "admin",
                    "title_key": "view.usage.label",
                    "title_fallback": "Usage",
                    "entry": "index.html",
                    "placement": { "slot": "admin.sidebar" }
                }]
            }
        })
    }

    /// The failure this task exists for: a pack an author can build, lint and
    /// pack with a current SDK was rejected here, so the feature could not
    /// reach the store at all.
    #[test]
    fn a_view_bearing_describe_is_accepted() {
        validate(&describe_with_views()).expect("a view-bearing describe must publish");
    }

    /// Validating more loosely than the SDK is as wrong as validating more
    /// strictly: it lets through a pack the SDK would have caught.
    #[test]
    fn a_view_missing_its_entry_is_rejected() {
        let mut d = describe_with_views();
        d["contributions"]["views"][0]
            .as_object_mut()
            .expect("view object")
            .remove("entry");
        assert!(validate(&d).is_err(), "a view without `entry` must not publish");
    }

    #[test]
    fn an_unknown_contribution_key_is_still_rejected() {
        let mut d = describe_with_views();
        d["contributions"]["viewz"] = serde_json::json!([]);
        assert!(validate(&d).is_err());
    }
}
```

Adjust `validate`'s name and call shape to whatever `schema.rs` actually exposes — read it first.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p greentic-store-api views_tests`
Expected: `a_view_bearing_describe_is_accepted` FAILS on `additionalProperties`, unless the interim patch PR has already landed — in which case it passes and you are removing the copy rather than fixing it. Say which in your report.

- [ ] **Step 3: Depend on the contract crate**

Add to `crates/greentic-store-api/Cargo.toml`. Pin the same way the rest of the workspace pins external Greentic crates — check what `Cargo.toml` already does for `greentic-store-blob` and follow it. The stable line is what the store validates against, so prefer the crates.io release (`1.2.11` or newer) over a git tag unless the workspace convention says otherwise.

- [ ] **Step 4: Replace the copy with the crate's validator**

In `crates/greentic-store-api/src/publish/schema.rs`, delete the `include_str!` of the local schema and delegate to the contract crate's validator, mapping its error type into whatever this module already returns so `publish.rs` needs no change.

Keep the module doc honest about what changed and why:

```rust
//! Publish-time describe validation.
//!
//! The schema is NOT kept here. It is the one embedded in
//! `greentic-extension-sdk-contract`, which is where the SDK generates
//! describes from — so the store cannot validate to a different shape than
//! the toolchain produces.
//!
//! It used to be a copy in `schemas/describe-v2.json`, and the copy drifted:
//! `contributions.views[]` shipped in SDK 1.2.10 and the copy rejected it, so
//! a feature an author could build, lint and pack could not be published at
//! all. Validating more loosely would have been equally wrong — it would let
//! through packs the SDK itself rejects.
```

- [ ] **Step 5: Delete the copy**

Remove `schemas/describe-v2.json` and any build-script or test reference to it. Grep for the filename across the repo first; if `describe-v1.json` or `describe-mcp-v1.json` are still read locally, leave those alone — only the v2 copy is superseded.

- [ ] **Step 6: Run the tests**

Run: `cargo test -p greentic-store-api`
Expected: PASS, including the three new tests and every existing publish test.

- [ ] **Step 7: Commit**

```bash
git add crates/greentic-store-api/Cargo.toml \
        crates/greentic-store-api/src/publish/schema.rs
git rm schemas/describe-v2.json
git commit -m "fix: validate publishes against the SDK's schema, not a copy of it"
```

---

### Task 2: The `view-assets` endpoint

**Files:**
- Create: `crates/greentic-store-api/src/handlers/extensions/view_assets.rs`
- Modify: `crates/greentic-store-api/src/handlers/extensions/mod.rs` (export the handler)
- Modify: `crates/greentic-store-api/src/router.rs` (register the route)
- Test: alongside the handler, plus an integration test if the repo has one for `artifact`

**Interfaces:**
- Consumes: `state.blob.get(artifact_key)`; the version row lookup `artifact.rs` already performs; `handlers::gate::ensure_entitled`.
- Produces: `GET /api/v1/extensions/{name}/{version}/view-assets`.

**Response shape — fix this before writing code, because Admin's sync is written against it:**

A `application/zip` body containing only entries under `assets/views/`, plus one added entry `view-manifest.json` at the archive root:

```json
{
  "schema": "greentic.view-assets/v1",
  "extension_id": "greentic.example",
  "version": "0.1.0",
  "entries": [
    { "path": "assets/views/usage/index.html", "sha256": "…", "size": 1234, "content_type": "text/html" }
  ]
}
```

`sha256` and `size` are computed from the bytes actually placed in this archive, not copied from the pack's `manifest.json` — Admin verifies against what it received. `content_type` is derived from the extension, from a fixed allowlist; anything unrecognised is `application/octet-stream`.

Bulk, not per-file, and the reason belongs in the module doc: the pack is one blob capped at 100 MiB, and there is no ETag or `Cache-Control` on any blob route here, so a per-file endpoint would mean one whole-blob fetch per CSS file. The only consumer is Admin's sync, which wants everything for a version at once.

- [ ] **Step 1: Write the failing test**

Create the test alongside the handler. Read how `artifact.rs` is tested first and mirror its fixture approach — if it builds a `.gtxpack` in memory, do the same; if it uses a fixture file, add one.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal in-memory .gtxpack carrying two view files and one
    /// unrelated entry, to prove the endpoint filters rather than repacks
    /// everything.
    fn pack_with_views() -> Vec<u8> {
        // zip: describe.json, extension.wasm, assets/views/usage/index.html,
        //      assets/views/usage/app.js, i18n/en.json
        // (follow the repo's existing in-memory zip helper if one exists)
        todo_build_pack()
    }

    #[test]
    fn only_view_assets_are_returned() {
        let out = extract_view_assets(&pack_with_views()).expect("extracts");
        let names = zip_entry_names(&out);
        assert!(names.iter().any(|n| n == "assets/views/usage/index.html"));
        assert!(names.iter().any(|n| n == "assets/views/usage/app.js"));
        assert!(names.iter().any(|n| n == "view-manifest.json"));
        assert!(
            !names.iter().any(|n| n == "i18n/en.json" || n == "extension.wasm"),
            "only assets/views/** may be returned, got {names:?}"
        );
    }

    #[test]
    fn the_manifest_describes_what_the_archive_contains() {
        let out = extract_view_assets(&pack_with_views()).expect("extracts");
        let manifest = read_manifest(&out);
        let entries = manifest["entries"].as_array().expect("entries");
        assert_eq!(entries.len(), 2, "one entry per view file, not per pack file");
        for e in entries {
            let path = e["path"].as_str().expect("path");
            let declared = e["sha256"].as_str().expect("sha256");
            assert_eq!(
                declared,
                sha256_hex(&read_entry(&out, path)),
                "manifest sha256 must describe the bytes actually shipped, \
                 not the ones the pack claimed"
            );
        }
    }

    #[test]
    fn a_pack_with_no_views_yields_an_empty_manifest_not_an_error() {
        let out = extract_view_assets(&pack_without_views()).expect("extracts");
        let manifest = read_manifest(&out);
        assert!(manifest["entries"].as_array().expect("entries").is_empty());
    }
}
```

Replace `todo_build_pack()` and the helper names with the repo's real equivalents — those placeholders must not survive into the commit.

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p greentic-store-api view_assets`
Expected: FAIL to compile — `extract_view_assets` does not exist.

- [ ] **Step 3: Write the extractor**

A pure function, separate from the handler, so it is testable without HTTP or storage:

```rust
/// Filter a stored `.gtxpack` down to its view assets, plus a manifest
/// describing exactly what came out.
///
/// Bulk rather than per-file on purpose: a pack is one blob of up to 100 MiB
/// and no blob route here carries an ETag or `Cache-Control`, so serving
/// individual files would mean re-fetching the whole blob per file. The only
/// consumer is the Admin console's catalog sync, which materialises a whole
/// version at once.
pub(crate) fn extract_view_assets(pack: &[u8]) -> Result<Vec<u8>, ViewAssetError> {
```

Walk the archive once. For each entry under `assets/views/`, copy the bytes into the output archive and record `{path, sha256, size, content_type}`. Write `view-manifest.json` last. Return the new archive's bytes.

Guard rails worth having in the extractor rather than the handler:
- Skip directory entries.
- Reject an entry whose name escapes `assets/views/` after normalisation — a malicious pack should not be able to steer this, even though publish should have caught it.
- Cap the total extracted size and return a typed error past it; the pack cap does not bound the *expanded* size.

- [ ] **Step 4: Write the handler and register the route**

Mirror `artifact.rs`: resolve `{name}/{version}` to a version row, `ensure_entitled`, `blob.get(artifact_key)`, call `extract_view_assets`, return `application/zip` with `Content-Length`.

Public and anonymous, exactly like `artifact` — add the same explanatory comment, since a reader will otherwise assume the omission of an `Auth` extractor is a mistake.

Register in `crates/greentic-store-api/src/router.rs` immediately after the `{version}/artifact` line, so the route table reads in the order a caller would discover them.

- [ ] **Step 5: Run the tests**

Run: `cargo test -p greentic-store-api`
Expected: PASS.

- [ ] **Step 6: Run the gate**

Look for `ci/local_check.sh` or the repo's equivalent and run it in the foreground. Otherwise at minimum `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`. Paste the real output.

- [ ] **Step 7: Commit**

```bash
git add crates/greentic-store-api/src/handlers/extensions/view_assets.rs \
        crates/greentic-store-api/src/handlers/extensions/mod.rs \
        crates/greentic-store-api/src/router.rs
git commit -m "feat: serve an extension version's view assets in one request"
```

---

## What this plan deliberately leaves out

- **No `manifestSha256` verification.** The server has never verified it, and Admin verifies what it receives against the manifest this endpoint generates. Making the store verify the pack's own ledger is a real improvement and a separate decision.
- **No caching.** No ETag, no `Cache-Control`. Admin caches by sha256 on its side, and the sync is the only caller. Adding HTTP caching here without a second consumer is speculative.
- **No range support** on `view-assets`, though `artifact.rs` has range-parsing that could be factored out. A view-asset archive is small; resumable download is not a need it has.

## Follow-on

Phase 3b (Admin backend) consumes this endpoint. Its sync-time materialisation is written against the `greentic.view-assets/v1` manifest shape above — if that shape changes here, it changes there.
