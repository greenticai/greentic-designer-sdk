# Audit Remediation — P1 Complete + Hand-off for P2–P5

> **Status (2026-05-28):** P1 (contract crate) COMPLETE and reviewed (spec +
> code-quality per task, plus a final holistic review: "P1 SOUND"). Branch
> `fix/integrity-verification`. A parallel effort (audit IDs M8–M15 / H4–H5 /
> M1–M3) is concurrently landing the registry/cli/state consumer work on the
> same branch. This note hands off the remaining trust-model wiring so the
> P1 machinery is not orphaned.

## What P1 delivered (contract crate `greentic-extension-sdk-contract`, v1.3.0-research)

The acyclic trust chain primitives + the C1 authenticity machinery:

```
RootVerifier (EmbeddedRootVerifier | FixtureRootVerifier)
   └─ verify_cert(&PublisherCert) -> trusted publisher VerifyingKey
sign(describe.json)  covers  describe.manifestSha256
        │ verify_describe_with_key(describe, trusted_key)   ← C1 authenticity
        ▼
   manifest_sha256  covers  manifest.json
        │ verify_manifest_binding(describe, manifest_bytes)  ← C2 binding
        ▼
   manifest.json    covers  every other file (describe.json EXCLUDED)
        │ verify_archive_against_manifest(zip_bytes)         ← C2/H2 (size-capped)
```

New/changed public API (all exported from `lib.rs`):
- `DescribeJson.manifest_sha256: Option<String>` (field, camelCase `manifestSha256`)
- `bind_manifest(&mut DescribeJson, &[u8])` — call BEFORE `sign_describe`
- `verify_manifest_binding(&DescribeJson, &[u8]) -> Result<()>`
- `verify_describe_with_key(&DescribeJson, &VerifyingKey) -> Result<()>` — **authenticity** (anchored)
- `verify_describe_self_consistent(&DescribeJson) -> Result<()>` — **integrity-only** (renamed from `verify_describe`)
- `verify_describe` — `#[deprecated]` alias of `_self_consistent` (transitional)
- `PublisherCert` + `PublisherCert::verify(&VerifyingKey) -> Result<VerifyingKey>`
- `RootVerifier` trait, `EmbeddedRootVerifier`, `FixtureRootVerifier` (feature `testing`)
- `DESCRIBE_ENTRY_NAME`, `MAX_ENTRY_BYTES` (64 MiB), `MAX_ARCHIVE_BYTES` (256 MiB)
- `CapabilityRef::version_req() -> Result<VersionReq, _>` (was infallible; **fail-closed**)
- `ContractError::{CertInvalid, TrustRootUnavailable}`

All ed25519 verification uses `verify_strict`. Gates green with and without
`--features testing`.

## ⚠️ Deprecation will NOT warn you

`#[deprecated]` on a `pub use` re-export does **not** propagate to downstream
call sites (rustc only honors it on the item's own definition). So
`verify_describe` callers in other crates compile clean under `-D warnings`
with no nudge. **Migrators must `grep -rn "verify_describe\b"` deliberately.**

## Remaining work to close the trust model (P2–P5)

Spec: `docs/superpowers/specs/2026-05-28-audit-remediation-design.md` (P2–P5).
The registry already does C2 (see `registry/src/lifecycle.rs` `verify_integrity`
composing `verify_archive_against_manifest` + `verify_manifest_binding` — landed
in parallel commit `67528c0`). The gaps below are what remains.

### P2 — registry (`greentic-extension-sdk-registry`)
1. **C1 authenticity (the key orphan risk):** `lifecycle.rs:189` still calls the
   deprecated `verify_describe` (integrity-only). Replace with
   `verify_describe_with_key(&describe, &trusted_key)` where `trusted_key` is
   resolved from a **trust anchor**, NOT the artifact:
   - Add `trust_store.rs`: `~/.greentic/trust/publishers.json` loader + allowlist
     + **TOFU pin/compare** (pin pubkey per extension id on first install;
     subsequent installs must match the pinned key).
   - `TrustPolicy`: make Strict / Normal / Loose actually distinct — Strict = key
     in trust store OR a `PublisherCert` resolved via `RootVerifier`; Normal =
     TOFU; Loose = dev feature gate only.
2. **C3:** verify `sha256(artifact.bytes) == expected digest` (signed/advertised)
   BEFORE `extract_to_staging`. Thread the expected digest into `ExtensionArtifact`.
3. **C4:** call `confirm_install(&describe, accept_permissions)` in the install
   path (currently dead code — never invoked).
4. (Already landed in parallel: https enforcement `a3a4022`, creds 0600 `a6bd3f2`,
   path-traversal `b891cfe`, staging rollback `67e27b5`, C2 enforcement `67528c0`.)

### P3 — CLI (`greentic-extension-sdk-cli`)
1. **bind+sign producer (no production producer binds today):** `dev/packer.rs`
   `build_pack` calls `build_gtxpack_with_manifest` but never `bind_manifest`
   nor signs → CLI packs ship `manifestSha256: None`, unsigned. In the
   pack/publish flow: build manifest → `bind_manifest(&mut describe, &manifest_bytes)`
   → `sign_describe(&mut describe, &key)` → write signed describe + manifest.
2. **H5:** `publish --sign` must load PKCS8 PEM (same as `sign.rs`/`keygen.rs`),
   not a raw 32-byte seed at a hardcoded path.
3. `gtdx verify` should run the full chain (`verify_describe_with_key` or at least
   `_self_consistent` + `verify_manifest_binding` + `verify_archive_against_manifest`).
4. (Already landed in parallel: H4 parse_pack_name `b9cd21f`, M8/M9/M10 wasm
   selection/dist/key-id `443713a`, lint breaking-bump WIP.)

### P4 — state + testing
- state: already landed in parallel (`820a936` RMW + lock). Confirm dir-fsync
  (audit H8) is included; if not, add parent-dir `sync_all()` after rename.
- testing crate: `build_provider_fixture_gtxpack` / `encode_gtpack_with_pack_id`
  should return `Result`; mocks should optionally enforce declared permissions.

### P5 — runtime (`greentic-designer-extensions`, separate repo, branch `research`)
- `verify_dir_signature` (`runtime.rs:235`): replace `verify_describe` with
  `verify_describe_with_key` (anchored) + add `verify_manifest_binding`.
- `verify_dir_manifest` (`runtime.rs:261`): remove the fail-open on missing
  `manifest.json` — reject unless `dev-allow-unsigned`. Reference extensions
  must be rebuilt in the new format (cascade).

### store-server (org-blocked)
KMS root key + `PublisherCert` issuance + advertised signed digests. Not local.

## Residual (org-blocked, fails closed)
`EmbeddedRootVerifier::from_embedded()` returns `TrustRootUnavailable` while
`PROD_ROOT_PUBKEY_B64` (`root_verifier.rs:34`) is empty. Provision the prod
Greentic root pubkey (KMS) to enable Strict-via-cert. TOFU + trust-store paths
do NOT need it, so P2 can proceed without the org decision.

## P1 commits (on `fix/integrity-verification`)
`2a20550`,`9e4063c` (T1) · `f674b8a`,`1fef5dd` (T2) · `858489a`,`19627e0` (T3) ·
`34c7460`,`c7ba14a` (T4) · `3c9499f` (T5) · `cbb9568` (T6) · `9ceded0` (T7) ·
`0f93645` (T8) · `71d90cb` (T9) · `29d6aed` (T10) · `c2c3836`,`efcf344` (T11) ·
`ce32a5c` (T12 version bump).
