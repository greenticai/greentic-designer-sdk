# Audit Remediation P1 — Contract Crate Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the foundation of the May-2026 audit remediation in `greentic-extension-sdk-contract`: bind the whole-archive manifest into the signed describe, add the trust-anchor machinery (RootVerifier + PublisherCert), harden untrusted-input parsing, and make fail-open paths fail-closed.

**Architecture:** `describe.json` carries a new `manifestSha256` field; signing covers it, it covers `manifest.json`, and the manifest covers every other file (describe.json is excluded — self-protected by its signature). A `RootVerifier` trait + `PublisherCert` provide the chain to a Greentic root key; the production root pubkey is a clearly-marked placeholder (org-blocked) while a `FixtureRootVerifier` makes the path testable today.

**Tech Stack:** Rust edition 2024, `ed25519-dalek`, `sha2`, `serde_jcs` (RFC 8785 JCS), `serde_json`, `jsonschema`, `zip`, `thiserror`.

**Spec:** `docs/superpowers/specs/2026-05-28-audit-remediation-design.md`

**Working dir:** `crates/greentic-extension-sdk-contract` (all paths below are relative to this crate unless noted). Branch: `research`.

**Commit convention:** conventional commits, NO Claude attribution. Run `cargo fmt --all`, `cargo clippy -p greentic-extension-sdk-contract --all-targets -- -D warnings`, `cargo test -p greentic-extension-sdk-contract` green before each commit.

---

## File Structure

| File | Responsibility | Change |
|---|---|---|
| `src/describe/mod.rs` | `DescribeJson` shape | Add `manifest_sha256` field (struct + raw + TryFrom) |
| `schemas/describe-v2.json` | v2 JSON schema | Allow/require `manifestSha256` |
| `src/manifest.rs` | manifest build + verify | Exclude `describe.json`; size caps; map lookup |
| `src/signature.rs` | sign/verify | `bind_manifest`, `verify_manifest_binding`, anchored verify, rename self-consistent, `verify_strict` |
| `src/root_verifier.rs` | trust anchor (NEW) | `RootVerifier` trait + `EmbeddedRootVerifier` (placeholder) + `FixtureRootVerifier` |
| `src/publisher_cert.rs` | publisher cert (NEW) | `PublisherCert` parse + verify against root |
| `src/capability.rs` | capability ref | `version_req` → `Result` (fail-closed) |
| `src/sha256.rs` | digest type | `from_str` validates bytes before slicing; use `hex::encode` |
| `src/migration.rs` | v0.4→v2 migrate | Strip stale `signature` + warn |
| `src/pack_writer.rs` | gtxpack writer | use `hex::encode` |
| `src/error.rs` | error enum | Add `CertInvalid`, `TrustRootUnavailable` |
| `src/lib.rs` | exports | Register new modules + symbols |
| `Cargo.toml` (workspace) | version | Bump to `1.2.4-research`; add `testing` feature |

---

## Task 1: Add `manifestSha256` to `DescribeJson`

**Files:**
- Modify: `src/describe/mod.rs`
- Modify: `schemas/describe-v2.json`
- Test: `tests/describe_roundtrip.rs`

- [ ] **Step 1: Write the failing test**

Append to `tests/describe_roundtrip.rs`:

```rust
#[test]
fn manifest_sha256_roundtrips() {
    let raw = greentic_extension_sdk_contract_test_fixture(); // existing helper or inline minimal JSON below
    let mut describe: greentic_extension_sdk_contract::DescribeJson =
        serde_json::from_str(&raw).expect("parse base fixture");
    describe.manifest_sha256 = Some("a".repeat(64));
    let json = serde_json::to_string(&describe).unwrap();
    assert!(json.contains("\"manifestSha256\":\"aaaaaaaa"), "field must serialize camelCase");
    let back: greentic_extension_sdk_contract::DescribeJson =
        serde_json::from_str(&json).unwrap();
    assert_eq!(back.manifest_sha256.as_deref(), Some(&"a".repeat(64)[..]));
}
```

If no shared fixture helper exists, inline a minimal valid v2 describe JSON string at the top of the test (copy from an existing passing test in this file — the file already constructs describe values).

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-contract --test describe_roundtrip manifest_sha256_roundtrips`
Expected: FAIL — `no field manifest_sha256 on type DescribeJson` (compile error).

- [ ] **Step 3: Add the field to both structs + TryFrom**

In `src/describe/mod.rs`, add to `DescribeJson` (after the `signature` field, before the closing brace):

```rust
    /// SHA-256 (lowercase hex) of the canonical `manifest.json`. Binds the
    /// whole-archive ledger into the signed describe (audit C2/H7). Optional
    /// only for backward compat during migration; production packs MUST set it.
    #[serde(rename = "manifestSha256", default, skip_serializing_if = "Option::is_none")]
    pub manifest_sha256: Option<String>,
```

Add the identical field to `DescribeJsonRaw`:

```rust
    #[serde(rename = "manifestSha256", default)]
    manifest_sha256: Option<String>,
```

In `impl TryFrom<DescribeJsonRaw> for DescribeJson`, add to the constructed struct literal:

```rust
            manifest_sha256: raw.manifest_sha256,
```

- [ ] **Step 4: Allow the field in the JSON schema**

In `schemas/describe-v2.json`, add to the top-level `properties` object:

```json
    "manifestSha256": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
```

(Do NOT add it to `required` — keep optional for migration. If `additionalProperties: false` is set at top level, this property entry is what allows it through.)

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p greentic-extension-sdk-contract --test describe_roundtrip manifest_sha256_roundtrips`
Expected: PASS.

- [ ] **Step 6: Run full crate tests + clippy + fmt**

Run: `cargo fmt --all && cargo clippy -p greentic-extension-sdk-contract --all-targets -- -D warnings && cargo test -p greentic-extension-sdk-contract`
Expected: PASS (existing schema tests still green because the field is optional).

- [ ] **Step 7: Commit**

```bash
git add src/describe/mod.rs schemas/describe-v2.json tests/describe_roundtrip.rs
git commit -m "feat(contract): add manifestSha256 field to describe.json (audit C2)"
```

---

## Task 2: Exclude `describe.json` from the manifest

**Files:**
- Modify: `src/manifest.rs:75` (the `build_manifest` filter)
- Test: `src/manifest.rs` (`#[cfg(test)]`)

- [ ] **Step 1: Write the failing test**

In `src/manifest.rs` tests module, add:

```rust
    #[test]
    fn build_manifest_excludes_describe_json() {
        let m = build_manifest(vec![
            ("describe.json", &br#"{"k":1}"#[..]),
            ("extension.wasm", &b"\0asm"[..]),
            ("manifest.json", &b"{}"[..]),
        ]);
        let paths: Vec<&str> = m.entries.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, vec!["extension.wasm"], "describe.json + manifest.json excluded");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-contract build_manifest_excludes_describe_json`
Expected: FAIL — assertion: left `["describe.json", "extension.wasm"]`, right `["extension.wasm"]`.

- [ ] **Step 3: Update the filter**

In `src/manifest.rs`, the `build_manifest` filter currently reads:

```rust
            if p == MANIFEST_ENTRY_NAME || p.ends_with('/') {
                return None;
            }
```

Change to:

```rust
            if p == MANIFEST_ENTRY_NAME || p == DESCRIBE_ENTRY_NAME || p.ends_with('/') {
                return None;
            }
```

Add the constant near `MANIFEST_ENTRY_NAME`:

```rust
pub const DESCRIBE_ENTRY_NAME: &str = "describe.json";
```

In `verify_archive_against_manifest`, the loop that checks archive entries must also skip `describe.json` (it is no longer in the manifest). Find:

```rust
        if name == MANIFEST_ENTRY_NAME || entry.is_dir() {
            continue;
        }
```

Change to:

```rust
        if name == MANIFEST_ENTRY_NAME || name == DESCRIBE_ENTRY_NAME || entry.is_dir() {
            continue;
        }
```

- [ ] **Step 4: Fix the existing test that assumes describe.json is in the manifest**

The existing `build_manifest_sorts_entries_and_excludes_self` test asserts `describe.json` is present. Update its expectation:

```rust
        assert_eq!(paths, vec!["a.wasm", "z.md"]);
```

The existing `verify_passes_on_intact_archive` and tamper tests include `describe.json` as an archive entry but it is now skipped by both build and verify — they still pass because the loop skips it. Confirm by running.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p greentic-extension-sdk-contract manifest`
Expected: PASS (both new + adjusted tests).

- [ ] **Step 6: Commit**

```bash
git add src/manifest.rs
git commit -m "feat(contract): exclude describe.json from manifest (self-protected by signature)"
```

---

## Task 3: `bind_manifest` + `verify_manifest_binding`

**Files:**
- Modify: `src/signature.rs`
- Modify: `src/lib.rs` (export)
- Test: `tests/signature_rt.rs`

- [ ] **Step 1: Write the failing test**

Append to `tests/signature_rt.rs`:

```rust
use greentic_extension_sdk_contract::{build_manifest, bind_manifest, verify_manifest_binding};

#[test]
fn bind_then_verify_manifest_binding() {
    let manifest = build_manifest(vec![("extension.wasm", &b"\0asm"[..])]);
    let manifest_bytes = serde_jcs::to_vec(&manifest).unwrap();
    let mut describe = signature_rt_fixture(); // existing helper that returns a DescribeJson
    bind_manifest(&mut describe, &manifest_bytes);
    assert!(describe.manifest_sha256.is_some());
    verify_manifest_binding(&describe, &manifest_bytes).expect("binding holds");

    // Tamper the manifest → binding must fail.
    let tampered = serde_jcs::to_vec(&build_manifest(vec![("evil.wasm", &b"x"[..])])).unwrap();
    assert!(verify_manifest_binding(&describe, &tampered).is_err());
}
```

If `signature_rt_fixture()` does not exist, reuse whatever construction the existing tests in `signature_rt.rs` already use to obtain a `DescribeJson` (the file already signs describe values).

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-contract --test signature_rt bind_then_verify_manifest_binding`
Expected: FAIL — `bind_manifest`/`verify_manifest_binding` not found.

- [ ] **Step 3: Implement the helpers**

In `src/signature.rs`, add:

```rust
/// Compute `sha256(manifest_bytes)` and store it on the describe so the
/// describe signature transitively covers the whole-archive manifest.
/// Call BEFORE `sign_describe`.
pub fn bind_manifest(describe: &mut DescribeJson, manifest_bytes: &[u8]) {
    describe.manifest_sha256 = Some(artifact_sha256(manifest_bytes));
}

/// Verify the describe's `manifest_sha256` matches the supplied manifest bytes.
///
/// # Errors
/// `SignatureInvalid` if the field is absent or does not match.
pub fn verify_manifest_binding(
    describe: &DescribeJson,
    manifest_bytes: &[u8],
) -> Result<(), ContractError> {
    let expected = describe
        .manifest_sha256
        .as_deref()
        .ok_or_else(|| ContractError::SignatureInvalid("describe.manifestSha256 missing".into()))?;
    let actual = artifact_sha256(manifest_bytes);
    if expected != actual {
        return Err(ContractError::SignatureInvalid(format!(
            "manifestSha256 mismatch: describe={expected}, computed={actual}"
        )));
    }
    Ok(())
}
```

In `src/lib.rs`, add to the `signature` re-export list:

```rust
    artifact_sha256, bind_manifest, canonical_signing_payload, sign_describe, sign_ed25519,
    verify_describe, verify_ed25519, verify_manifest_binding,
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p greentic-extension-sdk-contract --test signature_rt bind_then_verify_manifest_binding`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/signature.rs src/lib.rs tests/signature_rt.rs
git commit -m "feat(contract): bind_manifest + verify_manifest_binding (audit C2/H7)"
```

---

## Task 4: Size caps + map lookup in `verify_archive_against_manifest`

**Files:**
- Modify: `src/manifest.rs`
- Test: `src/manifest.rs` (`#[cfg(test)]`)

- [ ] **Step 1: Write the failing tests**

In `src/manifest.rs` tests, add:

```rust
    #[test]
    fn verify_rejects_oversize_declared_entry() {
        // An entry whose declared (header) size exceeds the cap is rejected
        // before any decompression read.
        let huge = MAX_ENTRY_BYTES + 1;
        let mut manifest = build_manifest(vec![("extension.wasm", &b"\0asm"[..])]);
        manifest.entries[0].size = huge;
        let manifest_json = serde_json::to_vec(&manifest).unwrap();
        let with_manifest: Vec<(&str, &[u8])> = vec![
            ("extension.wasm", b"\0asm"),
            ("manifest.json", &manifest_json),
        ];
        let bytes = build_zip(&with_manifest);
        let err = verify_archive_against_manifest(&bytes).unwrap_err();
        assert!(matches!(err, ManifestError::EntryTooLarge { .. }), "got {err:?}");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-contract verify_rejects_oversize_declared_entry`
Expected: FAIL — `MAX_ENTRY_BYTES` / `EntryTooLarge` not found.

- [ ] **Step 3: Add caps, error variant, and map lookup**

In `src/manifest.rs`, add near the top:

```rust
/// Per-entry uncompressed byte cap (64 MiB) — defends against decompression
/// bombs. Generous enough for any real extension wasm/asset.
pub const MAX_ENTRY_BYTES: u64 = 64 * 1024 * 1024;
/// Whole-archive uncompressed byte cap (256 MiB).
pub const MAX_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;
```

Add to `ManifestError`:

```rust
    #[error("entry '{path}' declared size {size} exceeds cap {cap}")]
    EntryTooLarge { path: String, size: u64, cap: u64 },
    #[error("archive uncompressed size exceeds cap {cap}")]
    ArchiveTooLarge { cap: u64 },
```

Rewrite the entry loop in `verify_archive_against_manifest`. Replace the body from the `for i in 0..archive.len()` loop with:

```rust
    use std::io::Read;
    let lookup: std::collections::BTreeMap<&str, &ManifestEntry> =
        manifest.entries.iter().map(|r| (r.path.as_str(), r)).collect();

    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut total: u64 = 0;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();
        if name == MANIFEST_ENTRY_NAME || name == DESCRIBE_ENTRY_NAME || entry.is_dir() {
            continue;
        }
        let row = *lookup
            .get(name.as_str())
            .ok_or_else(|| ManifestError::UnexpectedEntry(name.clone()))?;
        // Reject by declared header size before reading a single byte.
        if entry.size() > MAX_ENTRY_BYTES {
            return Err(ManifestError::EntryTooLarge {
                path: name,
                size: entry.size(),
                cap: MAX_ENTRY_BYTES,
            });
        }
        total = total.saturating_add(entry.size());
        if total > MAX_ARCHIVE_BYTES {
            return Err(ManifestError::ArchiveTooLarge { cap: MAX_ARCHIVE_BYTES });
        }
        // Read with a hard limit in case the header size lies.
        let mut body = Vec::new();
        entry
            .by_ref()
            .take(MAX_ENTRY_BYTES + 1)
            .read_to_end(&mut body)?;
        if body.len() as u64 > MAX_ENTRY_BYTES {
            return Err(ManifestError::EntryTooLarge {
                path: name,
                size: body.len() as u64,
                cap: MAX_ENTRY_BYTES,
            });
        }
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
```

Also cap the `manifest.json` read itself. Replace the manifest read block:

```rust
    let manifest: Manifest = {
        let mut f = archive
            .by_name(MANIFEST_ENTRY_NAME)
            .map_err(|_| ManifestError::Missing)?;
        if f.size() > MAX_ENTRY_BYTES {
            return Err(ManifestError::EntryTooLarge {
                path: MANIFEST_ENTRY_NAME.to_string(),
                size: f.size(),
                cap: MAX_ENTRY_BYTES,
            });
        }
        let mut body = Vec::new();
        f.by_ref().take(MAX_ENTRY_BYTES + 1).read_to_end(&mut body)?;
        serde_json::from_slice(&body)
            .map_err(|e| ManifestError::UnsupportedSchema(format!("parse: {e}")))?
    };
```

(Ensure `use std::io::Read;` is present at the top of the function or module.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p greentic-extension-sdk-contract manifest`
Expected: PASS (new cap test + all existing manifest tests).

- [ ] **Step 5: Commit**

```bash
git add src/manifest.rs
git commit -m "fix(contract): size caps + O(1) lookup in manifest verify (audit H2/M1)"
```

---

## Task 5: `version_req` fails closed

**Files:**
- Modify: `src/capability.rs`
- Test: `tests/capability.rs`

- [ ] **Step 1: Write the failing test**

Append to `tests/capability.rs`:

```rust
#[test]
fn version_req_rejects_garbage_instead_of_star() {
    let bad = greentic_extension_sdk_contract::CapabilityRef {
        id: "greentic.cap.test.v1".parse().unwrap(),
        version: "not-a-semver-req".into(),
        deprecated: None,
    };
    assert!(bad.version_req().is_err(), "malformed version must NOT silently become *");
}
```

(Match the `CapabilityRef` construction style used elsewhere in `tests/capability.rs`; adjust the `id` parse to the crate's `CapabilityId` API if needed.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-contract --test capability version_req_rejects_garbage_instead_of_star`
Expected: FAIL — `version_req()` returns `VersionReq`, not `Result`; `.is_err()` does not compile.

- [ ] **Step 3: Make it fallible**

In `src/capability.rs`, change:

```rust
    pub fn version_req(&self) -> VersionReq {
        VersionReq::parse(&self.version).unwrap_or(VersionReq::STAR)
    }
```

to:

```rust
    /// Parse the version requirement. Fails closed — a malformed string is an
    /// error, never a silent `*` match-everything (audit M2).
    ///
    /// # Errors
    /// `MalformedVersion` if `self.version` is not a valid semver requirement.
    pub fn version_req(&self) -> Result<VersionReq, ContractError> {
        VersionReq::parse(&self.version)
            .map_err(|e| ContractError::MalformedVersion(format!("{}: {e}", self.version)))
    }
```

Ensure `use crate::error::ContractError;` is present in `capability.rs`.

- [ ] **Step 4: Fix call sites in this crate**

Run: `grep -rn "version_req()" src/ ../` to find callers. Update each to handle the `Result` (propagate with `?` or `.map_err`). Within the contract crate, fix any usage. (Cross-crate callers in registry/cli/runtime are handled in their own phase plans.)

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p greentic-extension-sdk-contract --test capability && cargo clippy -p greentic-extension-sdk-contract --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/capability.rs
git commit -m "fix(contract): version_req fails closed on malformed input (audit M2)"
```

---

## Task 6: `Sha256::from_str` validates bytes before slicing

**Files:**
- Modify: `src/sha256.rs`
- Test: `tests/sha256.rs`

- [ ] **Step 1: Write the failing test**

Append to `tests/sha256.rs`:

```rust
#[test]
fn from_str_rejects_multibyte_without_panicking() {
    // 62 ascii + one 2-byte char = 64 bytes, 63 chars. Must error, not panic.
    let s = format!("{}{}", "a".repeat(62), "é");
    let parsed: Result<greentic_extension_sdk_contract::Sha256, _> = s.parse();
    assert!(parsed.is_err());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-contract --test sha256 from_str_rejects_multibyte_without_panicking`
Expected: FAIL — panics with "byte index ... is not a char boundary" (or similar), surfaced as test failure.

- [ ] **Step 3: Validate against bytes**

In `src/sha256.rs`, replace the body of `from_str` with a byte-based implementation:

```rust
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let raw = s.as_bytes();
        if raw.len() != 64 {
            return Err(ContractError::MalformedSha256(format!(
                "expected 64 hex chars, got {}",
                raw.len()
            )));
        }
        let mut bytes = [0u8; 32];
        for i in 0..32 {
            let hi = hex_val(raw[i * 2])?;
            let lo = hex_val(raw[i * 2 + 1])?;
            bytes[i] = (hi << 4) | lo;
        }
        Ok(Self(bytes))
    }
```

Add a free function in `src/sha256.rs`:

```rust
fn hex_val(b: u8) -> Result<u8, ContractError> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        other => Err(ContractError::MalformedSha256(format!(
            "non-lowercase-hex byte {other:#x}"
        ))),
    }
}
```

Also replace `as_hex`'s manual loop with the shared encoder (Task 9 dedup, but safe to do now):

```rust
    #[must_use]
    pub fn as_hex(&self) -> String {
        crate::hex::encode(&self.0)
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p greentic-extension-sdk-contract --test sha256`
Expected: PASS (new test + existing roundtrip tests).

- [ ] **Step 5: Commit**

```bash
git add src/sha256.rs
git commit -m "fix(contract): Sha256::from_str validates bytes, no slice panic (audit L1)"
```

---

## Task 7: Migration strips stale signature

**Files:**
- Modify: `src/migration.rs:75-77`
- Test: `tests/migration_value.rs`

- [ ] **Step 1: Write the failing test**

Append to `tests/migration_value.rs`:

```rust
#[test]
fn migration_strips_stale_signature_and_warns() {
    let v1 = serde_json::json!({
        "apiVersion": "greentic.ai/v1",
        "signature": { "algorithm": "ed25519", "publicKey": "x", "value": "y" }
        // ... plus whatever minimal v1 fields the existing migration tests use
    });
    let (out, report) = greentic_extension_sdk_contract::migrate_v0_4_x_value(&v1)
        .expect("migrate");
    assert!(out.get("signature").is_none(), "stale signature must be dropped");
    assert!(
        report.warnings().iter().any(|w| w.contains("signature")),
        "must warn re-sign required; got {:?}", report.warnings()
    );
}
```

Match the exact minimal v1 input + `MigrationReport` accessor (`warnings()` or equivalent) used by the existing tests in `tests/migration_value.rs` / `tests/migration_report.rs`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-contract --test migration_value migration_strips_stale_signature_and_warns`
Expected: FAIL — output still carries `signature`.

- [ ] **Step 3: Strip + warn**

In `src/migration.rs`, replace:

```rust
    if let Some(sig) = obj.get("signature").cloned() {
        out.insert("signature".into(), sig);
    }
```

with:

```rust
    // A v1 signature was computed over v1 canonical bytes; after migration the
    // canonical form differs entirely, so carrying it would be misleading.
    // Drop it and require re-signing (audit L2).
    if obj.get("signature").is_some() {
        report.warn("dropped v1 signature — migrated descriptor must be re-signed");
    }
```

Use the exact `report.warn(...)` method name the `MigrationReport` type exposes (check `src/migration.rs` for the existing warn API).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p greentic-extension-sdk-contract migration`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/migration.rs
git commit -m "fix(contract): drop stale signature on migration + warn re-sign (audit L2)"
```

---

## Task 8: `error.rs` new variants + hex dedup + `verify_strict`

**Files:**
- Modify: `src/error.rs`, `src/pack_writer.rs`, `src/signature.rs`

- [ ] **Step 1: Add error variants**

In `src/error.rs`, add to `ContractError`:

```rust
    /// Publisher certificate failed to parse or verify against the root.
    #[error("publisher cert invalid: {0}")]
    CertInvalid(String),

    /// The trust root is not available (e.g. production root key not yet
    /// provisioned — org-blocked).
    #[error("trust root unavailable: {0}")]
    TrustRootUnavailable(String),
```

- [ ] **Step 2: Dedup hex in pack_writer**

In `src/pack_writer.rs`, find the local hex fold loop (around line 130, `sha256_hex`) and any `format!("{:02x}")` usage. Replace the manual encode with `crate::hex::encode(&digest)`. Example for `sha256_hex`:

```rust
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    crate::hex::encode(&digest)
}
```

- [ ] **Step 3: Switch verify to `verify_strict`**

In `src/signature.rs` `verify_ed25519`, change:

```rust
    key.verify(payload, &signature)
        .map_err(|e| ContractError::SignatureInvalid(format!("verify: {e}")))
```

to:

```rust
    key.verify_strict(payload, &signature)
        .map_err(|e| ContractError::SignatureInvalid(format!("verify: {e}")))
```

Update the `use ed25519_dalek::...` import to bring `Verifier` (already imported) — `verify_strict` is an inherent method on `VerifyingKey`, so no extra import is needed; remove the now-unused `Verifier` import only if clippy flags it.

- [ ] **Step 4: Run tests + clippy**

Run: `cargo test -p greentic-extension-sdk-contract && cargo clippy -p greentic-extension-sdk-contract --all-targets -- -D warnings`
Expected: PASS. Existing signature roundtrip tests still verify (keys generated by `sign_describe` are valid non-torsion keys, so `verify_strict` accepts them).

- [ ] **Step 5: Commit**

```bash
git add src/error.rs src/pack_writer.rs src/signature.rs
git commit -m "refactor(contract): hex dedup + verify_strict + cert error variants (audit L3)"
```

---

## Task 9: `PublisherCert` module

**Files:**
- Create: `src/publisher_cert.rs`
- Modify: `src/lib.rs`
- Test: `tests/publisher_cert.rs` (new)

- [ ] **Step 1: Write the failing test**

Create `tests/publisher_cert.rs`:

```rust
use ed25519_dalek::{Signer, SigningKey};
use greentic_extension_sdk_contract::PublisherCert;
use rand::rngs::OsRng;

fn b64(bytes: &[u8]) -> String {
    use base64::{Engine, engine::general_purpose::STANDARD};
    STANDARD.encode(bytes)
}

#[test]
fn cert_verifies_against_issuing_root_and_rejects_wrong_root() {
    let root = SigningKey::generate(&mut OsRng);
    let publisher = SigningKey::generate(&mut OsRng);
    let pub_bytes = publisher.verifying_key().to_bytes();
    let root_sig = root.sign(&pub_bytes);

    let cert = PublisherCert {
        publisher_public_key: b64(&pub_bytes),
        root_signature: b64(&root_sig.to_bytes()),
        key_id: None,
        not_after: None,
    };

    // Correct root → returns the publisher key.
    let resolved = cert.verify(&root.verifying_key()).expect("valid cert");
    assert_eq!(resolved.to_bytes(), pub_bytes);

    // Wrong root → reject.
    let other_root = SigningKey::generate(&mut OsRng);
    assert!(cert.verify(&other_root.verifying_key()).is_err());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-contract --test publisher_cert`
Expected: FAIL — `PublisherCert` not found.

- [ ] **Step 3: Implement the module**

Create `src/publisher_cert.rs`:

```rust
//! `PublisherCert` — a Greentic-root-signed attestation binding a publisher's
//! ed25519 public key. The root signs the publisher's 32-byte public key;
//! verification recovers the authorized publisher key (audit C1 machinery).

use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::error::ContractError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PublisherCert {
    /// Base64 of the publisher's 32-byte ed25519 public key.
    #[serde(rename = "publisherPublicKey")]
    pub publisher_public_key: String,
    /// Base64 of the root's 64-byte ed25519 signature over the publisher key.
    #[serde(rename = "rootSignature")]
    pub root_signature: String,
    #[serde(rename = "keyId", default, skip_serializing_if = "Option::is_none")]
    pub key_id: Option<String>,
    /// Optional RFC3339 expiry. Enforcement is the caller's responsibility.
    #[serde(rename = "notAfter", default, skip_serializing_if = "Option::is_none")]
    pub not_after: Option<String>,
}

impl PublisherCert {
    /// Verify this cert was signed by `root`. Returns the authorized publisher
    /// verifying key on success.
    ///
    /// # Errors
    /// `CertInvalid` on any decode/length/signature failure.
    pub fn verify(&self, root: &VerifyingKey) -> Result<VerifyingKey, ContractError> {
        let pub_bytes = B64
            .decode(&self.publisher_public_key)
            .map_err(|e| ContractError::CertInvalid(format!("publisher key b64: {e}")))?;
        let pub_arr: [u8; 32] = pub_bytes
            .as_slice()
            .try_into()
            .map_err(|_| ContractError::CertInvalid("publisher key length != 32".into()))?;
        let sig_bytes = B64
            .decode(&self.root_signature)
            .map_err(|e| ContractError::CertInvalid(format!("root sig b64: {e}")))?;
        let sig_arr: [u8; 64] = sig_bytes
            .as_slice()
            .try_into()
            .map_err(|_| ContractError::CertInvalid("root sig length != 64".into()))?;
        let signature = Signature::from_bytes(&sig_arr);
        root.verify(&pub_arr, &signature)
            .map_err(|e| ContractError::CertInvalid(format!("root signature: {e}")))?;
        VerifyingKey::from_bytes(&pub_arr)
            .map_err(|e| ContractError::CertInvalid(format!("publisher key parse: {e}")))
    }
}
```

In `src/lib.rs`:

```rust
pub mod publisher_cert;
// ...
pub use self::publisher_cert::PublisherCert;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p greentic-extension-sdk-contract --test publisher_cert`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/publisher_cert.rs src/lib.rs tests/publisher_cert.rs
git commit -m "feat(contract): PublisherCert root-signed key attestation (audit C1)"
```

---

## Task 10: `RootVerifier` trait + verifiers + `testing` feature

**Files:**
- Create: `src/root_verifier.rs`
- Modify: `src/lib.rs`, `Cargo.toml` (this crate)
- Test: `tests/root_verifier.rs` (new)

- [ ] **Step 1: Add a `testing` feature**

In `crates/greentic-extension-sdk-contract/Cargo.toml`, add:

```toml
[features]
default = []
# Exposes FixtureRootVerifier + test-only trust helpers for downstream crates.
testing = []
```

- [ ] **Step 2: Write the failing test**

Create `tests/root_verifier.rs`:

```rust
use ed25519_dalek::{Signer, SigningKey};
use greentic_extension_sdk_contract::{FixtureRootVerifier, PublisherCert, RootVerifier};
use rand::rngs::OsRng;

fn b64(bytes: &[u8]) -> String {
    use base64::{Engine, engine::general_purpose::STANDARD};
    STANDARD.encode(bytes)
}

#[test]
fn fixture_root_verifier_resolves_publisher_from_cert() {
    let root = SigningKey::generate(&mut OsRng);
    let publisher = SigningKey::generate(&mut OsRng);
    let pub_bytes = publisher.verifying_key().to_bytes();
    let cert = PublisherCert {
        publisher_public_key: b64(&pub_bytes),
        root_signature: b64(&root.sign(&pub_bytes).to_bytes()),
        key_id: None,
        not_after: None,
    };
    let verifier = FixtureRootVerifier::new(root.verifying_key());
    let resolved = verifier.verify_cert(&cert).expect("cert chains to fixture root");
    assert_eq!(resolved.to_bytes(), pub_bytes);
}
```

This test requires the `testing` feature. Run it with `--features testing`.

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-contract --features testing --test root_verifier`
Expected: FAIL — `RootVerifier` / `FixtureRootVerifier` not found.

- [ ] **Step 4: Implement the module**

Create `src/root_verifier.rs`:

```rust
//! Trust anchor for publisher certs (audit C1 machinery).
//!
//! Production verification chains a [`PublisherCert`] to a Greentic root key.
//! The production root public key is **not yet provisioned** — that decision
//! (KMS custody, HSM vs KMS, rotation/DR) is org-blocked. Until then,
//! [`EmbeddedRootVerifier::from_embedded`] returns
//! [`ContractError::TrustRootUnavailable`]. The Strict-via-trust-store path
//! (registry crate) and the Normal/TOFU path do not require this root, so the
//! machinery is fully testable today via [`FixtureRootVerifier`].

use ed25519_dalek::VerifyingKey;

use crate::error::ContractError;
use crate::publisher_cert::PublisherCert;

/// Resolves an authorized publisher key from a [`PublisherCert`] by checking
/// the cert chains to a trusted root.
pub trait RootVerifier {
    /// Verify `cert` against the trusted root, returning the authorized
    /// publisher key.
    ///
    /// # Errors
    /// `CertInvalid` if the chain does not verify; `TrustRootUnavailable` if
    /// no root is configured.
    fn verify_cert(&self, cert: &PublisherCert) -> Result<VerifyingKey, ContractError>;
}

/// Base64-encoded production Greentic root public key.
///
/// TODO(org): provision the production root key (KMS) and paste its 32-byte
/// ed25519 public key here (base64). Until then the embedded verifier is
/// unavailable and Strict-via-cert installs are blocked — see spec residual.
const PROD_ROOT_PUBKEY_B64: &str = "";

/// Verifier backed by the compiled-in production root key.
pub struct EmbeddedRootVerifier {
    root: VerifyingKey,
}

impl EmbeddedRootVerifier {
    /// Construct from the embedded production root key.
    ///
    /// # Errors
    /// `TrustRootUnavailable` while the production key is unprovisioned
    /// (org-blocked), or `CertInvalid` if the embedded value is malformed.
    pub fn from_embedded() -> Result<Self, ContractError> {
        if PROD_ROOT_PUBKEY_B64.is_empty() {
            return Err(ContractError::TrustRootUnavailable(
                "production root key not yet provisioned (org-blocked)".into(),
            ));
        }
        use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
        let bytes = B64
            .decode(PROD_ROOT_PUBKEY_B64)
            .map_err(|e| ContractError::CertInvalid(format!("embedded root b64: {e}")))?;
        let arr: [u8; 32] = bytes
            .as_slice()
            .try_into()
            .map_err(|_| ContractError::CertInvalid("embedded root length != 32".into()))?;
        let root = VerifyingKey::from_bytes(&arr)
            .map_err(|e| ContractError::CertInvalid(format!("embedded root parse: {e}")))?;
        Ok(Self { root })
    }
}

impl RootVerifier for EmbeddedRootVerifier {
    fn verify_cert(&self, cert: &PublisherCert) -> Result<VerifyingKey, ContractError> {
        cert.verify(&self.root)
    }
}

/// Test-only verifier with a caller-supplied root key.
#[cfg(any(test, feature = "testing"))]
pub struct FixtureRootVerifier {
    root: VerifyingKey,
}

#[cfg(any(test, feature = "testing"))]
impl FixtureRootVerifier {
    #[must_use]
    pub fn new(root: VerifyingKey) -> Self {
        Self { root }
    }
}

#[cfg(any(test, feature = "testing"))]
impl RootVerifier for FixtureRootVerifier {
    fn verify_cert(&self, cert: &PublisherCert) -> Result<VerifyingKey, ContractError> {
        cert.verify(&self.root)
    }
}
```

In `src/lib.rs`:

```rust
pub mod root_verifier;
// ...
pub use self::root_verifier::{EmbeddedRootVerifier, RootVerifier};
#[cfg(any(test, feature = "testing"))]
pub use self::root_verifier::FixtureRootVerifier;
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p greentic-extension-sdk-contract --features testing --test root_verifier`
Expected: PASS.

- [ ] **Step 6: Verify embedded verifier is cleanly unavailable**

Add to `tests/root_verifier.rs`:

```rust
#[test]
fn embedded_root_is_unavailable_until_provisioned() {
    let err = greentic_extension_sdk_contract::EmbeddedRootVerifier::from_embedded().unwrap_err();
    assert!(matches!(
        err,
        greentic_extension_sdk_contract::ContractError::TrustRootUnavailable(_)
    ));
}
```

Run: `cargo test -p greentic-extension-sdk-contract --features testing --test root_verifier`
Expected: PASS. (Ensure `ContractError` is exported from `lib.rs` — it already is.)

- [ ] **Step 7: Commit**

```bash
git add src/root_verifier.rs src/lib.rs Cargo.toml tests/root_verifier.rs
git commit -m "feat(contract): RootVerifier trait + embedded/fixture verifiers (audit C1)"
```

---

## Task 11: Trust-anchored verify + rename self-consistent

**Files:**
- Modify: `src/signature.rs`, `src/lib.rs`
- Test: `tests/signature_v2.rs`

- [ ] **Step 1: Write the failing test**

Append to `tests/signature_v2.rs`:

```rust
use ed25519_dalek::{SigningKey, VerifyingKey};
use greentic_extension_sdk_contract::{
    sign_describe, verify_describe_self_consistent, verify_describe_with_key,
};
use rand::rngs::OsRng;

#[test]
fn verify_with_key_rejects_mismatched_signer() {
    let signer = SigningKey::generate(&mut OsRng);
    let mut describe = signature_v2_fixture(); // existing helper returning DescribeJson
    sign_describe(&mut describe, &signer).unwrap();

    // Self-consistent check passes (signature matches embedded key).
    verify_describe_self_consistent(&describe).expect("self-consistent");

    // Anchored check against the real signer key passes.
    verify_describe_with_key(&describe, &signer.verifying_key()).expect("anchored ok");

    // Anchored check against a DIFFERENT trusted key fails — this is the C1 fix.
    let attacker: VerifyingKey = SigningKey::generate(&mut OsRng).verifying_key();
    assert!(verify_describe_with_key(&describe, &attacker).is_err());
}
```

Reuse the describe-construction helper the existing `signature_v2.rs` tests use.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-contract --test signature_v2 verify_with_key_rejects_mismatched_signer`
Expected: FAIL — `verify_describe_self_consistent` / `verify_describe_with_key` not found.

- [ ] **Step 3: Add anchored verify + rename**

In `src/signature.rs`, rename the existing `verify_describe` to `verify_describe_self_consistent` and add a doc warning + the anchored variant:

```rust
/// Integrity-only check: verifies the inline signature against the key the
/// describe *asserts about itself*. This proves the describe has not changed
/// since signing, but NOT who signed it — an attacker can re-sign with their
/// own key. Callers needing authenticity MUST use [`verify_describe_with_key`]
/// against a trust-anchored key (audit C1).
pub fn verify_describe_self_consistent(describe: &DescribeJson) -> Result<(), ContractError> {
    let sig = describe
        .signature
        .as_ref()
        .ok_or_else(|| ContractError::SignatureInvalid("missing signature field".into()))?;
    if !matches!(sig.algorithm, crate::describe::SignatureAlgorithm::Ed25519) {
        return Err(ContractError::SignatureInvalid("unsupported algorithm".into()));
    }
    let payload = canonical_signing_payload(describe)?;
    verify_ed25519(&sig.public_key, &sig.value, &payload)
}

/// Authenticity check: verifies the inline signature was produced by
/// `trusted_key` AND that the describe's self-asserted key matches it. This is
/// the C1-correct verification — the key must come from a trust anchor
/// (trust store, TOFU pin, or a `PublisherCert` resolved via `RootVerifier`),
/// never from the artifact alone.
pub fn verify_describe_with_key(
    describe: &DescribeJson,
    trusted_key: &ed25519_dalek::VerifyingKey,
) -> Result<(), ContractError> {
    let sig = describe
        .signature
        .as_ref()
        .ok_or_else(|| ContractError::SignatureInvalid("missing signature field".into()))?;
    if !matches!(sig.algorithm, crate::describe::SignatureAlgorithm::Ed25519) {
        return Err(ContractError::SignatureInvalid("unsupported algorithm".into()));
    }
    // The self-asserted key must equal the trusted key (decoded form).
    let asserted = decode_ed25519_pubkey(&sig.public_key)?;
    if asserted.to_bytes() != trusted_key.to_bytes() {
        return Err(ContractError::SignatureInvalid(
            "describe signing key does not match the trusted key".into(),
        ));
    }
    let payload = canonical_signing_payload(describe)?;
    let sig_bytes = base64::engine::general_purpose::STANDARD
        .decode(&sig.value)
        .map_err(|e| ContractError::SignatureInvalid(format!("sig b64: {e}")))?;
    let sig_arr: [u8; 64] = sig_bytes
        .as_slice()
        .try_into()
        .map_err(|_| ContractError::SignatureInvalid("sig length != 64".into()))?;
    trusted_key
        .verify_strict(&payload, &ed25519_dalek::Signature::from_bytes(&sig_arr))
        .map_err(|e| ContractError::SignatureInvalid(format!("verify: {e}")))
}
```

Add the shared decode helper + the needed imports (`base64::Engine`) at the top of `src/signature.rs`:

```rust
fn decode_ed25519_pubkey(s: &str) -> Result<ed25519_dalek::VerifyingKey, ContractError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(strip_prefix(s))
        .map_err(|e| ContractError::SignatureInvalid(format!("pubkey b64: {e}")))?;
    let arr: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| ContractError::SignatureInvalid("pubkey length != 32".into()))?;
    ed25519_dalek::VerifyingKey::from_bytes(&arr)
        .map_err(|e| ContractError::SignatureInvalid(format!("pubkey parse: {e}")))
}
```

- [ ] **Step 4: Update intra-crate call sites + exports**

In `src/lib.rs`, update the signature re-export: replace `verify_describe` with `verify_describe_self_consistent, verify_describe_with_key`. Keep a deprecated alias for transitional cross-crate compatibility:

```rust
#[deprecated(note = "use verify_describe_with_key (authenticity) or verify_describe_self_consistent (integrity-only)")]
pub use self::signature::verify_describe_self_consistent as verify_describe;
```

Run `grep -rn "verify_describe\b" src/` and update any in-crate caller to the appropriate new name.

- [ ] **Step 5: Run tests + clippy to verify they pass**

Run: `cargo test -p greentic-extension-sdk-contract && cargo clippy -p greentic-extension-sdk-contract --all-targets -- -D warnings`
Expected: PASS. (The `#[deprecated]` alias keeps downstream crates compiling until their own phase plans migrate them; allow the deprecation warning only at the alias definition.)

- [ ] **Step 6: Commit**

```bash
git add src/signature.rs src/lib.rs tests/signature_v2.rs
git commit -m "feat(contract): trust-anchored verify_describe_with_key + rename self-consistent (audit C1)"
```

---

## Task 12: Bump contract crate version

**Files:**
- Modify: workspace `Cargo.toml` (the `[workspace.package] version` or the contract crate version field)

- [ ] **Step 1: Bump version**

Determine where the version is set (the contract crate uses `version.workspace = true`). In the workspace root `Cargo.toml`, bump `[workspace.package] version` from `1.2.3-research` to `1.2.4-research` (breaking describe format).

- [ ] **Step 2: Update path-dep pins if pinned with `=`**

Run: `grep -rn "1.2.3-research" --include=Cargo.toml .` across the workspace and update any `=1.2.3-research` pin to `=1.2.4-research`.

- [ ] **Step 3: Verify workspace still builds**

Run: `cargo build -p greentic-extension-sdk-contract && cargo test -p greentic-extension-sdk-contract`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml
git commit -m "chore(contract): bump to 1.2.4-research (breaking describe/manifest format)"
```

---

## Phase Exit Criteria

- [ ] `cargo fmt --all --check` clean.
- [ ] `cargo clippy -p greentic-extension-sdk-contract --all-targets -- -D warnings` clean.
- [ ] `cargo test -p greentic-extension-sdk-contract` and `--features testing` green.
- [ ] New symbols exported: `manifest_sha256` field, `bind_manifest`, `verify_manifest_binding`, `verify_describe_with_key`, `verify_describe_self_consistent`, `PublisherCert`, `RootVerifier`, `EmbeddedRootVerifier`, `FixtureRootVerifier`, `DESCRIBE_ENTRY_NAME`, `MAX_ENTRY_BYTES`, `MAX_ARCHIVE_BYTES`.
- [ ] `verify_describe` remains as a `#[deprecated]` alias so registry/cli/runtime still compile until P2/P3/P5 migrate them.

**Next:** P2 (registry) plan is written against these concrete signatures after P1 lands.
