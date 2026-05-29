# Audit Remediation — P1 Complete + Hand-off for P2–P5

> **Status (2026-05-28):** P1 (contract crate) COMPLETE and reviewed (spec +
> code-quality per task, plus a final holistic review: "P1 SOUND"). Branch
> `fix/integrity-verification`. A parallel effort (audit IDs M8–M15 / H4–H5 /
> M1–M3) is concurrently landing the registry/cli/state consumer work on the
> same branch. This note hands off the remaining trust-model wiring so the
> P1 machinery is not orphaned.

## What P1 delivered (contract crate `greentic-extension-sdk-contract`, v1.2.4-research)

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

> **Status (2026-05-29):** P2, P3, and P4 are COMPLETE on
> `fix/integrity-verification`. Trust chain producer→verify is closed
> end-to-end locally; only P5 (separate `greentic-designer-extensions` repo)
> and the org-blocked store-server / prod root key remain. New commits:
> C2 producer `a8ae3ef`, H5 PKCS8 PEM `4dc7c04`, verify full-chain `578aa60`,
> H8 state dir-fsync `7b3917e`, P4 testing M6/M7 `4efeb85`. (C1/C3/C4 landed
> earlier: `d6aaec2`, `617b8a8`, `67528c0`, `c2c3836`.)

### P2 — registry (`greentic-extension-sdk-registry`) — ✅ SHIPPED
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

### P3 — CLI (`greentic-extension-sdk-cli`) — ✅ SHIPPED
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

### P4 — state + testing — ✅ SHIPPED
- state: RMW + lock landed in parallel (`820a936`); dir-fsync (audit H8) added in
  `7b3917e` (parent-dir `sync_all()` after rename, Unix; no-op on Windows).
- testing crate (`4efeb85`): `build_provider_fixture_gtxpack` /
  `encode_gtpack_with_pack_id` now return `anyhow::Result` (M6); `MockHttpClient`
  / `MockSecretsBackend` optionally enforce declared permissions via
  `restrict_to_hosts` / `restrict_to` (M7).

### P5 — runtime (`greentic-designer-extensions`, separate repo) — 🟡 PARTIAL (PR #68 → `research`)
Done (branch `fix/audit-p5-trust-chain`, `greentic-biz/greentic-designer-extensions` PR #68):
- Bumped contract `=1.2.3-research` → `=1.2.4-research` (local `[patch.crates-io]` path).
- `verify_dir_signature`: `verify_describe` → `verify_describe_self_consistent`.
- `verify_dir_manifest`: **fails closed** — missing `manifest.json` rejected
  (dev-allow-unsigned escape kept), `verify_manifest_binding` added (describe↔
  manifest), per-entry hash check retained. Test fixtures rebuilt as real
  bind→sign→manifest packs; `manifest_gate.rs` rewritten to the fail-closed model.

Remaining:
- **Do NOT publish `1.2.4-research` to crates.io.** `-research` versions are
  intentionally registry-blocked (`release.yml` `publish-crates` skips tags
  containing `research`, added in `fb57181` / audit H6/M16). Research consumers
  resolve via the `[patch.crates-io]` local sibling checkout — that patch is the
  intended state and stays. crates.io publish only happens when the work
  graduates research→develop under a non-research version.
- Anchored authenticity: `verify_dir_signature` → `verify_describe_with_key`
  needs a runtime trust store + the org-provisioned root key (org-blocked).

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
