# Extensions 1.0 Cleanup — Umbrella Spec

> **Status (2026-05-17): SHIPPED on `research` line.** All in-scope phases (A, B, C, D minus D.5, E) merged. Plan body preserved as historical record.
>
> **Open items (escalated, not solo-actionable):**
> - **D.5+ trust root + key custody** — needs CEO/CTO decision on master-key custody before implementation can start.
> - **Store-server Fargate redeploy** — D.3 (#27 in greentic-store-server) merged to research; production redeploy is a devops/access task.
>
> **PR map by phase:**
>
> | Phase | What | PRs |
> |---|---|---|
> | A.1+A.2 | Contract 0.5→1.2.0-research typed describe + migrator | greenticai/greentic-designer-sdk#6 |
> | A.3 | Designer SDK contract bump + resolve_runtime_ref | greenticai/greentic-designer-sdk (Phase A) |
> | A.4 | Per-repo v2 migration | greentic-biz/greentic-adaptive-card-mcp#50, llm-extensions#15, provider-extensions#24, deployer-extensions#24, bundle-extensions#40, dw-canvas-extension#7, dw-extensions#21, components-public#54/#55 |
> | B | Host functions (i18n, secrets, http, broker) | greentic-biz/greentic-designer-extensions#52 |
> | C | Ext-runtime → v2 describe | greentic-biz/greentic-designer-extensions#54/#55/#56 |
> | D.1 | `#![forbid(unsafe_code)]` on 5 SDK crate roots | greenticai/greentic-designer-sdk#11 |
> | D.2 | Gate `GREENTIC_EXT_ALLOW_UNSIGNED` behind feature | greentic-biz/greentic-designer-extensions#57 |
> | D.3 | JCS canonical signing on server | greentic-biz/greentic-store-server#27 (needs Fargate redeploy) |
> | D.4.1 | Manifest schema + builder + verifier | greenticai/greentic-designer-sdk#13 |
> | D.4.2/3 | `build_gtxpack_with_manifest` + packer call-site swap | greenticai/greentic-designer-sdk#14 |
> | D.4.runtime | ext-runtime verifies manifest on install | greentic-biz/greentic-designer-extensions#58 |
> | D.5+ | Trust root | **BLOCKED on org decision** |
> | E.1 | Stale doc/store-URL sweep | greentic-biz/greentic-deployer-extensions#26, greentic-biz/greentic-designer#321, greentic-biz/greentic-provider-extensions#27 |
> | E.2 | Contract / pkg version drift fixes + guard tests | greenticai/greentic-designer-sdk#12 |
> | E.3 | Scaffold toolchain pin + wit-bindgen-rt 0.41 (all kinds) | greenticai/greentic-designer-sdk#15 |
> | E.4 | `Kind::Llm` + `templates/llm/` tree | greenticai/greentic-designer-sdk#20 |
> | E.5.a-d | `gtdx lint` with 3 rules + 4 fixtures | greenticai/greentic-designer-sdk#18 |
> | E.5.e | `W_DESCRIBE_DIFF_BREAKING` describe-diff rule | greenticai/greentic-designer-sdk#22 |
> | E.6 | `MockHost` composable mock layer | greenticai/greentic-designer-sdk#17 |
> | E.7.a | `gtdx dev --mount` strict-parity mode | greenticai/greentic-designer-sdk#19 |
> | followup | All 6 scaffold describe templates → v2 shape | greenticai/greentic-designer-sdk#21 |
> | followup | `gtdx-version` cascade across 8 extension repos | greentic-biz/greentic-llm-extensions#18, provider-extensions#28, deployer-extensions#27, bundle-extensions#46, adaptive-card-mcp#53, dw-canvas-extension#8, dw-extensions#22, greenticai/components-public#56 |
> | followup | Workspace 1.2.0→1.2.1-research bump + publish | greenticai/greentic-designer-sdk#16 |
>
> ---
>
> **Original (draft) sections below.** Anything checked `- [ ]` was the design intent — actual delivery is captured in the PR map above. Anything that diverged from the plan (scope cuts, deferred sub-tasks) is called out in the relevant PR body.

---

> **Status:** draft (2026-05-13). Author: Bima.
> **Goal:** Ship a `1.2.x` research-line release of the Greentic extensions ecosystem with zero open audit findings from the May-2026 audit.
> **Driver:** an audit triggered by the AC `component_ref` decouple work uncovered 14 P0 + 25 P1 gaps spanning contract, runtime, security, DX, taxonomy, and observability. We're already bumping the contract for the decouple — bundle the rest of the cleanup into the same release window.

---

## 1. Audit summary

Six parallel agents reviewed the extensions system on `2026-05-13`. Full findings recorded in memory `project_extensions_audit_may2026`.

**Headline P0s:**

| # | Area | Gap | Evidence |
| --- | --- | --- | --- |
| 1 | Security | No trust root — any keypair passes verify | `greentic-designer-sdk/crates/greentic-extension-sdk-registry/src/lifecycle.rs:132-142` |
| 2 | Security | WASM binary not signed (only describe.json JCS-signed) | `crates/greentic-extension-sdk-contract/schemas/describe-v1.json` (no `componentSha256`) |
| 3 | Security | Server signs non-canonical (`serde_json::to_vec`) while client verifies JCS (`serde_jcs`) | `greentic-store-server/.../extensions.rs:393` vs `signature.rs:52-79` |
| 4 | Security | `memoryLimitMB` zero-enforced; wasmtime has no fuel/epoch/StoreLimits | `greentic-ext-runtime/src/runtime.rs:71-84` |
| 5 | Security | `#![forbid(unsafe_code)]` MISSING from sdk-contract, sdk-state, sdk-registry, ext-runtime | grep across crate roots |
| 6 | Security | `GREENTIC_EXT_ALLOW_UNSIGNED=1` env bypass | `runtime.rs:144-150` |
| 7 | Contract | `contributions` is untyped `type: object` in schema | `describe-v1.json:47-49` |
| 8 | Contract | No version compat block (`min_designer_version`, `contract_version`) | `describe/mod.rs:157-162` |
| 9 | Runtime | Two-version coexistence broken — `loaded` map keyed by id only | `greentic-ext-runtime/src/loaded.rs:11-13` |
| 10 | Runtime | `invoke_tool` sync wasmtime in async axum (no `spawn_blocking`) | `greentic-designer/src/ui/tool_bridge/dispatch.rs:483` |
| 11 | Runtime | Install path skips sha256 verification | `lifecycle.rs:45-93` |
| 12 | Taxonomy | `ProviderExtension` dispatch never wired | `greentic-designer-extensions/CLAUDE.md:7-21` |
| 13 | Taxonomy | Broker `call_extension` returns `"not implemented in 4B.0"` | `host_state.rs:114-116` |
| 14 | Taxonomy | `http::fetch` + `secrets::get` + `i18n::t/tf` are all stubs (`"not implemented in 4B.0"`) | `host_state.rs:71-129` |

**Confirmed FIXED** (no action): AC `validation_mode=warn` bug. Fixed in `component-adaptive-card@1.2.0` commit `845cbd3`.

---

## 2. Phase breakdown

The cleanup splits cleanly along subsystem boundaries. Each phase has its own implementation plan and produces working+testable software independently.

| Phase | Subsystem | Plan doc | Primary repo | Depends on | Blocking org decision? |
| --- | --- | --- | --- | --- | --- |
| **A** | Contract 0.5.0 bump | `plans/2026-05-13-contract-0.5.0-bump.md` (this repo) | `greentic-designer-sdk` | — | No |
| **B** | Wire 4B.0 host stubs | `greentic-designer-extensions/docs/superpowers/plans/2026-05-13-host-functions.md` | `greentic-designer-extensions` | A (permissions types) | No |
| **C** | Runtime hardening | `greentic-designer-extensions/docs/superpowers/plans/2026-05-13-runtime-hardening.md` | `greentic-designer-extensions` | A merged | No |
| **D** | Security hardening | `plans/2026-05-13-security-hardening.md` (this repo) | multi-repo | A (componentSha256), B (real perms) | **YES — trust root key custody (CEO/CTO)** |
| **E** | DX cleanup | `plans/2026-05-13-dx-cleanup.md` (this repo) | multi-repo | A (for gtdx lint), independent for docs | No |

**Sequencing rationale:**

1. **A first** — contract is the root dependency. Everyone else builds on the bumped types. We block downstream until A's contract types compile in `greentic-extension-sdk-contract@0.5.0`.
2. **B in parallel with late-A** — host stub implementations only loosely depend on `Permissions` types. Once A's permissions section is settled (Task A.4 or so), B can start.
3. **C after A merged** — runtime hardening rewrites `loaded.rs` keying + adds wasmtime caps; conflicts with A's contract-consumer migration. Wait for A to merge to research before starting C.
4. **D engineering parts in parallel** — `forbid(unsafe_code)` restore, `serde_jcs` server fix, network matcher fix don't need org sign-off. The trust-root + sign-whole-`.gtxpack` work IS blocked.
5. **E always in parallel** — docs/version-drift/raw-IP fixes are independent. `gtdx lint` depends on A.

---

## 3. Affected repos

14 repos in scope. All checked out and on canonical branch (research where exists, main for two-tier):

| Repo | Branch | Role |
| --- | --- | --- |
| `greentic-designer-sdk` | research | **Contract owner.** Spec + plans A/D/E live here. |
| `greentic-designer-extensions` | research | Runtime crate (`greentic-ext-runtime`). Plans B + C live here. |
| `greentic-designer` | research | Consumer (designer host). Migrates per A. |
| `greentic-adaptive-card-mcp` | research | Reference design extension. Migrates per A; original decouple use case. |
| `greentic-llm-extensions` | research | Reference design extensions. Migrates per A. |
| `greentic-provider-extensions` | research | Provider extensions. Migrates per A + benefits from C (dispatch wiring). |
| `greentic-deployer-extensions` | research | Deploy extensions. Migrates per A. |
| `greentic-bundle-extensions` | research | Bundle extensions. Migrates per A. |
| `greentic-messaging-providers` | research | Messaging provider extensions. Migrates per A. |
| `greentic-store-server` | research | Server-side signing. Fixed in D (canonical signing). |
| `greentic-docs` | main (two-tier) | Public docs. Fixed in E (repo/crate names, raw IP). |
| `greentic-cards2pack` | main (two-tier) | Fixed in E (OCI `:latest` → `:stable`). |
| `greentic-secrets` | main (two-tier) | Backend for `secrets::get` host fn in B. |
| `greentic-i18n` | main (two-tier) | Backend for `i18n::t/tf` host fn in B. |

**Skipped:** `greentic-dw-providers` (active WIP on `feat/unified-catalog`). DW examples' OCI `:latest` cleanup deferred to a separate PR coordinated with the catalog work.

---

## 4. Success criteria (definition of "1.2.x clean")

A release qualifies as "extensions 1.0 cleanup complete" when ALL of the following are true. Each item maps to a phase + task; verification is `cargo test` or grep, not "looks good":

### Contract (Phase A)
- `greentic-extension-sdk-contract = "0.5.0"` published to crates.io.
- `describe-v1.json` has typed `contributions.nodeTypes[]`, `contributions.tools[]`, `contributions.recipes[]`, `contributions.knowledge[]`, `contributions.prompts[]`, `contributions.schemas[]`. Verify: `jq '.properties.contributions.properties | keys' describe-v1.json` returns the 6 keys.
- `Runtime.components: Map<ComponentId, RuntimeRef>` field exists. `nodeType.runtime_ref: Option<ComponentId>` + `tool.runtime_ref: Option<ComponentId>` exist. Verify: tests in `sdk-contract` round-trip these.
- `compat: { min_designer_version, min_runner_version, contract_version }` block exists in describe + parsed as `semver::VersionReq`. Verify: parse test on invalid spec fails.
- `localization: { default_locale, strings }` block exists. `nodeType.label`/`metadata.summary`/`metadata.description` accept `LocalizedString`. Verify: round-trip test of multi-locale describe.
- `deprecated: { since, replaced_by, removal_in }` on nodeType + capability.
- `Signature.algorithm: enum { Ed25519 }`, `signature.key_id: Option<String>` exists.
- Per-component `sha256` field exists in `RuntimeComponent`. Verify: AC's describe.json populated with computed sha256.

### Host stubs (Phase B)
- `host_state.rs` does NOT contain the string `"not implemented in 4B.0"` anywhere. Verify: `grep -r '"not implemented in 4B.0"' greentic-designer-extensions/` returns zero matches.
- `i18n::t/tf` resolves via `greentic-i18n` translator threaded through `HostState`. Verify: round-trip test with a non-en locale returns the translation, not the key.
- `secrets::get` calls into `greentic-secrets` backend. Verify: integration test with a fixture secrets store.
- `http::fetch` checks URL against `permissions.network` using a strict matcher (scheme + host + path-prefix, not substring). Verify: 3 attack-vector tests pass (open-redirect path, scheme downgrade, subdomain confusion).
- `broker::call_extension` dispatches to another loaded extension's WIT export. Verify: integration test with two fixture extensions where A calls B.

### Runtime (Phase C)
- `loaded` map keyed by `(ExtensionId, Version)`. Two versions of same id can co-exist. Verify: integration test.
- `wasmtime::Config` enables fuel + epoch interruption. `Store` has `StoreLimits` honoring `memoryLimitMB`. Verify: tests that allocate >limit fail, that infinite-loop hits epoch.
- `invoke_tool` wrapped in `spawn_blocking` from async callers. Verify: tokio task-trace inspection in integration test.
- `.cwasm` cache present + trusted (signed cache file or content-addressed sha256 path).
- `Installer::install` verifies `Sha256(bytes) == metadata.artifact_sha256` BEFORE extract. Verify: corruption-injection test fails the install.
- `tracing::error!` events include `ext_id` + `tool_name` on every failure path. Verify: grep `tracing::error!` and assert `ext_id` parameter.
- Per-extension `prometheus`/`tracing-opentelemetry` metrics: invocation_count, latency_histogram, error_count. Verify: `/metrics` endpoint surfaces them.
- `RuntimeEvent::ExtensionUpdated` triggers in-flight Plan cancel-and-restart OR returns the prev version until plan completes. Verify: integration test mutates ext mid-plan.
- `Capability::resolve()` called in `Installer::install`; install fails if `required` graph unsatisfied. Verify: install of an extension declaring `required: [missing-cap]` fails fast.

### Security (Phase D)
- `Installer::install` checks `signature.public_key` against an allowlist (Greentic root + optional org-trusted keys from config). Verify: install with unknown publisher key fails.
- Server signs `serde_jcs::to_vec(&req.describe)` (matches client). Verify: round-trip server-sign → client-verify integration test.
- `.gtxpack` signature covers all archive entries (whole-zip signed envelope or per-entry sha tree). Verify: tamper-injection test for `extension.wasm` swap fails verification.
- `#![forbid(unsafe_code)]` present at `lib.rs` of: `greentic-extension-sdk-contract`, `greentic-extension-sdk-state`, `greentic-extension-sdk-registry`, `greentic-extension-sdk-testing`, `greentic-extension-sdk-cli`, and `greentic-ext-runtime`. Verify: grep.
- `GREENTIC_EXT_ALLOW_UNSIGNED` env gated behind compile-time `cfg(feature = "dev-allow-unsigned")` only. Verify: release build without the feature ignores the env.
- `permissions.network` matcher is `url::Url` parse + scheme + host-suffix + path-prefix. Verify: open-redirect, subdomain-confusion, scheme-downgrade tests fail to bypass.
- Per-extension state directory permissions enforced (0700 on Unix). Verify: install integration test inspects `stat`.

### DX (Phase E)
- `gtdx new --kind=llm` works. Verify: scaffold + cargo build + install succeed in test harness.
- `gtdx-cli.md`, `writing-extensions.md`, `publishing-extensions.md`, `provider-extensions.md`, `bundle-extensions.md` reference `greenticai/greentic-designer-sdk` and crate `greentic-extension-sdk-cli`. Verify: grep for `greentic-biz/greentic-designer-extensions` returns zero matches.
- `CONTRACT_VERSION` constant matches WIT directory + README + scaffold doc output. Verify: grep across `greentic-extension-sdk-cli/src/`, `embedded-wit/`, README.
- `rust-toolchain.toml` in scaffold template matches SDK (`1.95.0`). Verify: grep.
- `wit-bindgen-rt` version in scaffold matches reference impls (single canonical version). Verify: grep across scaffold templates + reference extension Cargo.tomls.
- `github-action.md` does NOT contain raw IP `62.171.174.152`. Verify: grep.
- `gtdx lint` exists; produces describe-diff vs installed + semver-bump check + capability cycle check. Verify: subcommand tests.
- `greentic-extension-sdk-testing::MockHost` provides `logging`, `i18n`, `secrets`, `broker`, `http` mock implementations. Verify: doc test instantiates + invokes.
- `gtdx dev --mount ./my-ext/` skips pack step and points designer at the source dir. Verify: doc-tested workflow.
- No `:latest` in `greentic-cards2pack/src/`. Verify: grep `:latest` against `oci://` refs.

### Organizational
- Trust-root key custody policy DECIDED + DOCUMENTED in `greentic-docs/src/content/docs/operating/trust-root.md`. Verify: file exists, names a KMS/HSM, names rotation cadence.
- The publisher-key allowlist source (config file path, environment variable, or remote-fetched manifest) is DECIDED + DOCUMENTED.

---

## 5. Decisions needed from Bima before execution

1. **Trust-root key custody** (blocks Phase D's whole-`.gtxpack` signing flow). Three options sketched:
   - **(a) Single Greentic root in KMS**, all publisher keys are sub-certs we issue. Highest control, slowest publisher onboarding.
   - **(b) Greentic root + open publisher registration** (Sigstore-style transparency log). Lowest friction, requires running a log service.
   - **(c) Allowlist file shipped with designer + manually updated.** Cheapest, no third-party trust. Doesn't scale past internal extensions.
2. **Contract bump: minor break vs major break?** Phase A IS breaking (typed `contributions` rejects loose payloads). Two options:
   - **(a) Bump to `0.5.0`** and require all extension authors to migrate. Cleaner contract, paperwork.
   - **(b) Ship `0.4.x` with `Either<Untyped, Typed>` deserialization** that accepts both — `0.5.0` later removes the untyped arm.
3. **Scope of Phase E's `gtdx dev --mount`** — strict feature parity with packed install, or "dev-only" with explicit limitations doc?
4. **Should the 1.2.x release wait for Phase D's organizational decisions** OR ship Phases A/B/C/E first as "1.2.x partial" and Phase D follows in 1.3.x?

---

## 6. Out of scope

Explicitly NOT in this cleanup:

- **Multi-tenant `tenants` field implementation.** The schema slot stays reserved; no code path activates. Picked up in a separate "extensions 1.1" cycle.
- **TUF / Sigstore / transparency log.** Phase D ships a static allowlist; transparency comes later.
- **CPU fuel limit per-extension calibration.** We ship fuel-limit-on (binary), not per-extension tuning.
- **`greentic-runner` extension support.** Runner stays extension-unaware. Designer-only consumer.
- **DW family (`greentic-dw-providers`) example JSON cleanup.** Coordinated with `feat/unified-catalog` separately.
- **WIT contract version 1.0.** SDK stays at WIT 0.5 line — the contract 0.5.0 bump only touches Rust types and JSON schema, not WIT.

---

## 7. Tracking

When each phase's plan is written, this spec gets a row in the table:

| Phase | Plan written | Plan approved | Impl in progress | Merged | Released |
| --- | --- | --- | --- | --- | --- |
| A | ✅ 2026-05-13 [link](../plans/2026-05-13-contract-0.5.0-bump.md) | ⬜ | ⬜ | ⬜ | ⬜ |
| B | ✅ 2026-05-13 [link](../../../../greentic-designer-extensions/docs/superpowers/plans/2026-05-13-host-functions-implementation.md) | ⬜ | ⬜ | ⬜ | ⬜ |
| C | ✅ 2026-05-13 [link](../../../../greentic-designer-extensions/docs/superpowers/plans/2026-05-13-runtime-hardening.md) | ⬜ | ⬜ | ⬜ | ⬜ |
| D | ✅ 2026-05-13 [link](../plans/2026-05-13-security-hardening.md) | ⬜ | ⬜ | ⬜ | ⬜ |
| E | ✅ 2026-05-13 [link](../plans/2026-05-13-dx-cleanup.md) | ⬜ | ⬜ | ⬜ | ⬜ |

**Inter-plan consistency review (2026-05-13)**: cross-checked Plan A's contract shape against Plan B/C consumption sites. Plan A explicitly retains `Permissions { network: Vec<String>, secrets: Vec<String>, call_extension_kinds: Vec<String> }` as-is (line 2135 of Plan A) — Plan B's pre-conditions match. Plan C declares Plan A as merge-dependency in header + sequences `sdk-contract = "0.5"` bump as first action. All 5 plans target `research`, strip Claude attribution, follow Conventional Commits.

**Known inter-plan flags** (resolve during execution review):
- **Plan B flag**: `greentic-i18n-lib` does NOT expose JSON catalog loader publicly; Plan B vendors via `include_str!`. If a cleaner public API is desired, address in greentic-i18n repo as a separate PR upstream of Plan B execution.
- **Plan B flag**: `greentic-secrets-core::SecretsBackend` trait is heavier than runtime needs; Plan B introduces a narrow port + designer-side adapter. Keep this boundary stable; do NOT widen the runtime port without re-review.
- **Plan B flag**: `invoke_tool` signature shifts to `self: &Arc<Self>` for brokered dispatch (B.5). Designer call sites already hold `Arc<ExtensionRuntime>` so this is source-compatible; flag for reviewers.
- **Plan A note**: Deployer extension `contributions.targets[]` field becomes `execution.legacy_targets` AND mirrors as typed `contributions.recipes[]` for designer's recipe picker. Recipe picker migration to native typed `recipes[]` lives in Plan E (`gtdx lint` runtime_ref validation will catch this once consumers fully migrate).
- **Plan D note**: `forbid(unsafe_code)` already present at contract crate root — Phase D's D.1 audit will confirm zero changes needed for that one crate; other 5 crate roots still need it.
- **Plan C note**: One `unsafe_code` ALLOW required for `wasmtime::Component::deserialize_file` in `cache.rs` only — isolated, scope-gated. Other crate files keep `#![forbid(unsafe_code)]`.

Update this row on every PR open/merge.
