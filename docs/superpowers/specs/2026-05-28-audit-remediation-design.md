# Design: May 2026 Audit Remediation (`research` branch)

> **Status (2026-05-28): APPROVED, pre-implementation.**
> Closes the open findings from the May 2026 extension-SDK audit so the
> `research` line is clean when promoted to `develop`. One residual is
> org-blocked (embedded production root pubkey + KMS issuance, D.5); all
> supporting machinery and the Strict/TOFU trust model land regardless.

## Context

A read-only audit of `greentic-designer-sdk` (6 crates, ~15.4k LOC) plus the
`greentic-ext-runtime` consumer surfaced a broken supply-chain trust chain and
a set of HIGH/MEDIUM/LOW hardening gaps. Phase-D of the original audit shipped
D.1–D.4 (`forbid(unsafe_code)`, `dev-allow-unsigned` gate, JCS server signing,
manifest schema + verifier). D.5 (trust root) was parked "blocked on org
decision". This spec covers everything that can land without that org decision,
and scaffolds the part that cannot.

### Decisions locked during brainstorming

| Decision | Choice |
|---|---|
| C1 (root-of-trust) scope | Build the machinery now (RootVerifier + trust store + publisher cert + TOFU); embedding the **production** root pubkey + KMS issuance stays org-blocked |
| Repo scope | `greentic-designer-sdk` + `greentic-designer-extensions` runtime locally; `greentic-store-server` is a documented follow-up (not present locally; its remaining work is the org-blocked KMS piece) |
| Manifest binding model | **A** — `manifest_sha256` field inside the signed `describe.json` (single signature, least disruption to the shipped describe-signing model) |
| Legacy-pack transition | **Strict-by-default + dev feature gate** — production rejects unsigned/unmanifested packs; `dev-allow-unsigned` is the only bypass |

## Architecture: the trust chain (acyclic)

```
RootVerifier (embedded root pubkey | KMS — PROD KEY PENDING ORG)
      │ verifies
      ▼
PublisherCert  { publisher_pubkey, signed_by_root }
      │ authorizes
      ▼
sign(describe.json)  ── covers ──▶  describe.manifest_sha256
      │                                    │ covers
      │ covers describe content            ▼
      │                               manifest.json
      │                                    │ covers (sha256 per entry)
      │                                    ▼
      └────────────────────▶  extension.wasm + assets + …
                              (describe.json EXCLUDED from manifest —
                               it is self-protected by its own signature)
```

The cycle (describe contains its own signature *and* would be a manifest entry)
is broken by excluding `describe.json` from the manifest. `describe.json` is
protected by its signature; the signature covers `manifest_sha256`;
`manifest_sha256` covers `manifest.json`; the manifest covers every other file.

### Verify path (shared by `gtdx` and the runtime host)

1. Resolve the publisher key via `RootVerifier`:
   - **Strict** — pubkey must be in the trust store, or carry a `PublisherCert`
     chaining to the root.
   - **Normal** — TOFU: pin the pubkey on first install; subsequent installs of
     the same extension id must present the pinned key.
   - **Loose** — only reachable behind the `dev-allow-unsigned` Cargo feature.
2. `verify_describe(describe, resolved_key)` — ed25519 signature check against
   the **resolved** key, not the self-asserted one.
3. `describe.manifest_sha256 == sha256(manifest.json)`.
4. `verify_archive_against_manifest(bytes)` — recompute every entry's sha256.
5. **Install download only:** `sha256(artifact.bytes) == expected digest`
   (signed/advertised) **before** `extract_to_staging`.

## Findings → work items

Severity tags reference the audit report. `[ORG]` = residual blocked on org
decision.

### P1 — `greentic-extension-sdk-contract` (breaking → version bump)

- **C2/H7 (bind):** add `manifest_sha256: String` to `DescribeJson`;
  `build_manifest` excludes `describe.json`; add `bind_manifest(&mut describe,
  &manifest)` and verify the binding in the verify path.
- **C1 (machinery):** new modules
  - `publisher_cert.rs` — `PublisherCert` struct, parse, `verify(root_key)`.
  - `root_verifier.rs` — `RootVerifier` trait, `EmbeddedRootVerifier`
    (const root pubkey — **placeholder + `// TODO(org): prod root key` [ORG]**),
    `FixtureRootVerifier` (`#[cfg(test)]`/`testing` feature only).
  - Re-export from `lib.rs`.
- **C1 (verify):** `verify_describe` gains a trust-anchored variant that takes
  the resolved key; rename the no-anchor form to
  `verify_describe_self_consistent` and document it as integrity-only (not
  authentication). Update all call sites.
- **H2/M1:** size caps in `verify_archive_against_manifest` — consult
  `entry.size()`, enforce per-entry + per-archive limits, read via `take(limit)`;
  replace the O(n²) linear scan with a `BTreeMap<&str, &ManifestEntry>`.
- **M2:** `version_req` returns `Result` (fail-closed; no silent `*`).
- **L1:** `Sha256::from_str` validates ASCII-hex bytes before slicing.
- **L2:** migration v0.4→v2 strips the now-invalid `signature` and emits a
  `report.warn` requiring re-sign.
- **L3 / nits:** route all hex through `hex::encode`; switch ed25519 verify to
  `verify_strict`.

### P2 — `greentic-extension-sdk-registry`

- **C3:** thread an expected digest into `ExtensionArtifact`; verify
  `sha256(artifact.bytes)` before `extract_to_staging`.
- **C4:** call `confirm_install(&describe, accept_permissions)` in the install
  path; make `TrustPolicy` Strict/Normal/Loose behave distinctly; add
  `trust_store.rs` (`~/.greentic/trust/publishers.json` loader + allowlist +
  TOFU pin/compare).
- **H1:** require `https://` for remote registries (loopback allowed only behind
  an explicit insecure dev flag); validate scheme at config load; OCI refuses
  `ClientProtocol::Http`.
- **H2/H3:** reqwest `.timeout()` + `.connect_timeout()`; streamed download with
  a running byte-count cap; extraction enforces per-entry + total uncompressed
  caps.
- **H4:** credentials written atomically with mode `0o600`
  (`OpenOptions::mode().create_new` into a temp file in the same dir + rename);
  parent dir `0o700`.
- **H5 (path):** validate `metadata.id`/`version`
  (`^[a-z0-9][a-z0-9._-]*$`, reject path separators / `..`); assert the resolved
  install dir is a canonical child of the extensions root before any write.
- **M (OCI):** support and prefer `@sha256:` digest references + TOFU pinning;
  select the artifact layer by media type instead of `layers.next()`.
- **M (errors):** replace `unwrap_or_default()` on a corrupt index with
  log-and-abort (never silently overwrite); make `reference()` and the client
  builder fallible (drop `expect`).
- **M5:** run `schema::validate_describe_json` uniformly across all backends;
  cross-check downloaded `id`/`version` equal what was requested.

### P3 — `greentic-extension-sdk-cli` (`gtdx`)

- **H5:** `publish --sign` loads a PKCS8 PEM via a shared key-loader (same as
  `sign.rs`/`keygen.rs`); error messages name the expected path + format.
- **H6:** restrict the `GITHUB_TOKEN` OCI fallback to `ghcr.io`; log which
  env var/host a credential is sent to (stderr).
- **C4 / verify:** `gtdx verify` runs the full chain (signature +
  `manifest_sha256` + manifest), not just describe.
- **M2:** `dev/installer.rs` stays `Loose` but prints the auto-accepted
  permissions.
- **M1 (login):** reject empty tokens; optional registry probe.
- **L1:** `verify.rs` replaces `expect` with a returned error.
- **L2:** `new --force` refuses to delete a directory lacking a scaffold marker
  (e.g. `.gtdx-contract.lock`) without confirmation.
- **L4:** `packer` skips-with-warning on symlinks pointing outside the project.

### P4 — `greentic-extension-sdk-state` & `-testing`

State:
- **H8:** fsync the parent directory after `rename` (per-platform guard).
- **M:** RAII guard cleans up `.tmp`/`.lock` on every error path; document the
  advisory-lock + last-writer-wins semantics; switch `fs2`→`fs4`; unique temp
  name.

Testing (published library):
- **M6:** public non-test APIs (`build_provider_fixture_gtxpack`,
  `encode_gtpack_with_pack_id`) return `Result`; remove `.expect("valid cap
  id")` on caller input.
- **M7:** `MockHttpClient`/`MockSecretsBackend` can be constructed from
  `describe.permissions` so undeclared access fails the test the way the runtime
  would; document the gap regardless.
- **H (gtxpack):** replace deprecated `mangled_name` with `enclosed_name()`;
  explicit symlink handling; fixture `sha256` computed from actual bytes.

### P5 — `greentic-designer-extensions` (runtime, branch `research`)

- **C1/C2:** `verify_dir_signature` adopts the trust-anchored chain
  (`RootVerifier` + `manifest_sha256`) instead of self-signed `verify_describe`.
- **Transition (Strict-by-default):** remove the fail-open in
  `verify_dir_manifest` — a missing `manifest.json` is rejected unless
  `dev-allow-unsigned` is enabled. Consequence: reference extensions must be
  rebuilt in the new format (cascade follow-up).

### store-server — documented follow-up (no code here)

JCS server signing shipped (D.3). The remaining pieces — KMS-backed root key,
issuing `PublisherCert`s, advertising signed artifact digests — are **[ORG]**
blocked. Spec records the required server-side changes; no code lands until the
repo is available and the org decision is made.

## Testing strategy

- TDD per item: failing test first, then implementation.
- New `crates/greentic-extension-sdk-cli/tests/integration_attack_vectors.rs`
  (D.10) — end-to-end: tampered wasm, swapped key, smuggled entry, http
  downgrade, oversized download, path-traversal id, missing manifest under
  Strict.
- Runtime: extend `signature_gate.rs` / `manifest_gate.rs` to assert Strict
  rejection of legacy packs and acceptance of the new chain.
- Each crate stays green under `cargo fmt --check`, `cargo clippy -D warnings`,
  `cargo test --workspace`.

## Versioning & rollout

- Bump the contract crate (e.g. `1.2.4-research`) — breaking `describe`/manifest
  format.
- Reference-extension cascade rebuild (the ~8 repos from the Phase-D plan) is a
  tracked follow-up; only repos present locally are touched here.
- **Residual [ORG]:** embedding the production root pubkey + KMS issuance. The
  code carries a clearly-marked placeholder; everything else (machinery, TOFU,
  Strict gate) is complete, so `research` is clean apart from that one const.

## Out of scope

- Unrelated refactors.
- The reference-extension rebuilds beyond locally-present repos.
- The org policy decision itself (KMS custody, HSM vs KMS, rotation/DR).
