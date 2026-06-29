# Generic ComponentExtension Adapter — Multi-Session Plan

**Date:** 2026-06-25
**Owner:** Bima
**Origin:** WhatsApp thread with Maarten Ectors (founder), 2026-06-25.

## What Maarten asked for

> "We don't convert [components to extensions] one by one. We have a **ComponentExtension**
> that just points to any gtc wasm and can dynamically provide what the designer needs —
> similar to how we have one **MCP-Adapter** which can convert any wasix mcp into a gtc
> component. You provide the store/repo/OCI url and potentially we can take the description
> from the wasm because each component has a description."

Two non-negotiables from that:
1. **One generic adapter**, not a per-component hand-authored extension.
2. **Description auto-derived from the wasm itself** (component carries its own description).
3. Input is a **store/repo/OCI URL**, not a local path.

## Key finding from code audit (do NOT skip — it changes the work)

The runtime is **already generic** for components-as-tools. Confirmed:

- `greentic-runner/crates/greentic-aw-runtime/src/tools.rs:79-92` (list) and `:171-191`
  (dispatch) — any tool whose `extension_id` starts with `component:` is resolved against
  `ComponentToolCatalog` by `(component_ref, operation)`. **No per-component wrapper needed.**
- `ToolRef { extension_id, tool_name }` — `greentic-aw-runtime/src/config.rs:15-18`. A
  literal `component:<ref>` already works at runtime.
- `PackRuntimeComponentInvoker::list_operations()` —
  `greentic-runner/crates/greentic-runner-host/src/runner/component_invoker.rs:37-151` —
  already enumerates every component operation across loaded packs.

**The real gaps are:**
- **A. Operation descriptions.** `greentic_types::ComponentOperation`
  (`greentic-types/src/component.rs:168-175`) has **no `description` field**. Today the runner
  synthesizes a boilerplate string (`component_invoker.rs:46-65`,
  `describe_operation()` → `"Invoke operation '{op}' of component '{ref}'"`). But WIT doc
  comments ARE already extracted at build time in
  `greentic-component/crates/greentic-component/src/describe.rs:139-167` (`func.docs.contents`)
  — they're just not carried into the manifest operations. This is the "description from the
  wasm" Maarten wants. **Wire it through.**
- **B. Designer-side discovery.** The designer lists agentic-worker tools via
  `list_tools_by_capability()` (`greentic-designer/src/ui/routes/extensions.rs:437-461`) which
  iterates **loaded extensions only**. There is no generic component catalog there. This is
  what makes people hand-author one extension per component today.

## The template to mirror: MCP-Adapter

One adapter, config-driven, admin-registered, dynamic `list_tools`. Reference files:

- `greentic-runner/crates/greentic-aw-runtime/src/mcp_source/source.rs` — `McpToolSource`:
  `new()` (:36), `catalog_for_role()` (:80), `list_server_tools()` (:274), `dispatch_route()` (:321).
- `greentic-runner/crates/greentic-aw-runtime/src/mcp_source/types.rs` — `McpToolCatalog`,
  `McpRoute`, `McpToolEntry`, `Transport`, `CATALOG_TTL` (5 min).
- Designer mirror: `greentic-designer/src/ui/mcp/catalog/snapshot.rs` — `build_snapshot()` (:22),
  `dispatch_route()` (:251).
- Admin registration shape: `greentic-designer-admin/src/domain/mcp_server.rs` — `mcp_servers`
  table, `MCP_ROLE_AGENTIC_WORKER`, per-tenant, multi-role, `transport: http | local-wasm`,
  `component_ref`/`component_version`/`component_digest`, `allowed_tools` whitelist.

The component adapter should reuse the **same shape**: an admin-registered list of component
URLs per-tenant, role-filtered, TTL-cached, introspected for tools dynamically.

---

## DESIGN DECISION — confirm in Session 0 before coding

Two readings of "ComponentExtension". They share Sessions 1–2; they diverge after.

- **Path A — Generic admin-registered adapter (RECOMMENDED, matches MCP-Adapter + "don't
  convert one by one").** Operator/designer registers a component by URL into a per-tenant
  catalog (new `component_tools` table mirroring `mcp_servers`). A `ComponentToolSource`
  dynamically pulls + introspects it and surfaces its operations as tools. Nothing is
  generated or hand-authored; registration is data. `gtdx` command (Session 5) just writes a
  registration row.

- **Path B — SDK CLI generates a `describe.json` wrapper per component URL.** `gtdx extension
  new --component <url>` fetches the wasm, introspects exports, and emits a signed
  `.gtxpack` extension wrapping it. Closer to Bima's first phrasing, but still produces N
  artifacts and re-introduces per-component objects — weaker fit for "dynamically provide".

**Recommendation: Path A**, with the `gtdx` command as the *registration UX* (writes a
catalog entry), not a wrapper generator. Confirm with Maarten that "ComponentExtension" =
dynamic catalog, not generated artifact. The sessions below assume Path A and note where B
would differ.

---

## Sessions

Each session is sized to land in one focused working session with its own PR. Sessions 1–2
are prerequisites for everything; 3–6 build the adapter; 7 is UX/docs.

### Session 0 — Confirm design + lock contracts + runtime spike (no production code)
**Goal:** De-risk before building, and **freeze the 3 cross-repo contracts** so Tracks A/B/C
can run in parallel in separate sessions without diverging. This session is the gate; do not
fan out before it lands.
- Confirm Path A/B with Maarten (one message). Record the answer at top of this file.
- Spike: build a tiny `.gtpack` with one real component, hand-write an `AgentConfig` whose
  `tools` contains `{ extension_id: "component:<ref>", tool_name: "<op>" }`, run the
  agentic-worker loop, confirm the LLM can call it and get a result. (Uses
  `greentic-aw-runtime` test harness; see existing tests around `tools.rs`/`loop.rs`.)
- **Freeze these 3 contracts in writing (append to this file as an appendix):**
  1. **Operation description type** — exact shape of the new field on
     `greentic_types::ComponentOperation`, e.g. `pub description: Option<String>`. Consumed by
     Track A (produces it), Track C (reads it from manifest), S4 (shows it to the LLM).
  2. **`component_tools` admin API JSON** — request/response shape for
     `GET /api/v1/designer/tenant/me/component-tools` and the admin CRUD. Mirror the
     `mcp_servers` wire row (`id`, `name`, url/`component_ref`+`version`+`digest`,
     `allowed_operations`, `roles`). Produced by Track B (S3), consumed by S4 + S5. Write the
     JSON example, not just prose.
  3. **`component:<ref>` ToolRef convention** — already implemented in the runtime
     (`tools.rs:79-92` / `:171-191`); record it as frozen so nobody re-litigates the prefix.
- **Done when:** spike proves component-as-tool works at runtime; the A/B decision is
  recorded; and the 3 contracts are written down. No merge — this is a spike + contract doc.

### Session 1 — Carry component operation descriptions (greentic-types + greentic-component)
**Goal:** Make "description from the wasm" real. Foundational for both paths.
- `greentic-types/src/component.rs:168` — add `pub description: Option<String>` to
  `ComponentOperation`. Bump crate version per the release-train rules (see memory
  [[greentic-types-release-train-gate]] — this touches the consumer graph; coordinate).
- `greentic-component/crates/greentic-component/src/describe.rs:139-167` — the WIT
  `func.docs.contents` extraction already exists. Wire it into the manifest operation build so
  `ComponentManifest.operations[].description` is populated at `prepare`/`describe` time
  (`prepare.rs:46-144`, manifest in `manifest/mod.rs:34-60`).
- `greentic-runner-host/.../component_invoker.rs:46-65` — change `describe_operation()` to
  prefer the manifest description, falling back to the synthetic string only when absent.
- **Done when:** a built component's manifest carries real per-operation descriptions sourced
  from WIT docs, and the runner surfaces them to the LLM instead of boilerplate. Add a test
  with a WIT-documented component asserting the description round-trips.
- **Watch:** CLAUDE.md no-`unwrap`/`panic` rule; `cargo fmt`+clippy `-D warnings`; run
  `bash ci/local_check.sh` in each touched repo. Cross-repo version bump = separate PRs.

### Session 2 — Component fetch + introspection library  [SCOPED 2026-06-25, grounded in code]

**Home crate: `greentic-component` (crate `greentic-component`), behind the existing `store`
feature.** Verified both halves already live there with no new cross-crate dep and no cycle:
- `greentic-component/crates/greentic-component/Cargo.toml:56,105` already declares
  `store = ["dep:greentic-distributor-client", ...]` (optional). `greentic-distributor-client`
  depends only on `greentic-types`, so there is **no dependency cycle**.
- **Fetch primitive (corrected — NOT `greentic-component-store`, whose OCI is a stub):**
  `greentic_distributor_client::DistClient::ensure_cached(reference) -> ResolvedArtifact`
  (`greentic-distributor-client/src/dist.rs:2429`). Handles `oci://`, `repo://`, `store://`,
  `https://`, `file://`, and digest refs; SHA256-verifies; caches under `$GREENTIC_CACHE_DIR`.
  This is exactly the "store/repo/OCI url" Maarten named. (`greentic-component-store`'s
  `StoreLocator::Oci` returns `UnsupportedScheme` — do NOT use it for OCI.)
- **Introspection:** reuse `greentic-component`'s own `prepare`/`describe` to list operations
  (`name` + `input_schema`). NOTE: `description` will be `None` until the S1 source decision
  lands — that is fine; S2 fills name+schema, the description column is populated later.
- **Open implementation sub-task:** introspecting a *bare* fetched wasm (no manifest in hand)
  needs the WIT world / describe-export discovery. `describe::from_wit_world(path, world)` needs
  the world string; resolve it from the artifact (the distributor artifact may be a packaged
  component carrying its manifest, or a raw wasm whose world must be discovered). Settle this
  with a TDD fixture in the first S2 commit.

**Shape:** `#[cfg(feature = "store")] pub async fn resolve_component(url: &str) ->
Result<ResolvedComponent>` where `ResolvedComponent { component_ref, version, digest,
operations: Vec<{name, input_schema, description: Option<String>}> }`.

**Consumers / placement decision (grounded):** designer-admin ALREADY depends on
`greentic-distributor-client = "0.5"` (`greentic-designer-admin/Cargo.toml:56`) and probes MCP
servers SERVER-SIDE at registration (`src/routes/admin/tenant_mcp/probe.rs` — HTTP via
`greentic-mcp-client`, local-wasm via `greentic-mcp-exec.list_tools`). Two models:
- **Model 2 (start here — lighter):** the caller (gtdx S5, or a future designer UI action)
  fetch+introspects via `resolve_component` and POSTs `operations[]`. **S3 already accepts
  `operations[]` in the payload** (the `// TODO(S2)` is only for an optional server-side probe),
  so no admin change is needed to ship. Keeps wasmtime introspection out of the admin binary.
- **Model 1 (later, optional):** add `tenant_component_tools/probe.rs` mirroring the MCP probe
  for UI-driven registration. Incremental — admin already has distributor-client.

Original notes below.

---

**Goal:** Given a store/repo/OCI URL, pull the wasm, verify digest, list its operations +
descriptions. Reusable by both the designer adapter and the `gtdx` command.
- Survey existing fetch code first (reuse-first policy): `greentic-pack` OCI support
  (`crates/packc/tests/oci_stable_refs.rs`), greentic-component store/fetch, store-server
  registry. Do NOT write a new OCI client if one exists.
- Build a small function: `resolve_component(url) -> { component_ref, version, digest,
  operations: Vec<{name, description, input_schema}> }`. Verify SHA256. Cache locally
  (mirror MCP's `mcp_store_pull::ensure_cached`).
- **Done when:** given an OCI/store URL, you get back the operation list with descriptions,
  digest-verified, with a test against a fixture component.

### Session 3 — Admin registration for component tools (mirror `mcp_servers`)
**Goal:** Per-tenant catalog of registered component URLs, role-filtered to `agentic_worker`.
- `greentic-designer-admin` — add a `component_tools` table + domain type mirroring
  `domain/mcp_server.rs` (`id`, `name`, `url`/`component_ref`+`version`+`digest`,
  `allowed_operations`, `roles`, per-tenant). Reuse `MCP_ROLE_AGENTIC_WORKER` role constant
  pattern.
- API: `GET /api/v1/designer/tenant/me/component-tools` (designer read) +
  admin CRUD `/api/admin/tenants/{id}/component-tools`. Mirror the MCP routes exactly.
- **Done when:** an operator can register a component URL for a tenant and the designer
  endpoint returns it. Tests for CRUD + tenant scoping.

### Session 4 — `ComponentToolSource` + designer surfacing (the adapter)
**Goal:** The actual "one ComponentExtension". Mirror `McpToolSource` / `snapshot.rs`.
- New `ComponentToolSource` in the designer (mirror
  `greentic-designer/src/ui/mcp/catalog/snapshot.rs`): fetch registered components from admin,
  introspect via Session 2's library, build a per-tenant catalog of tools keyed
  `(component_ref, operation)` with TTL cache.
- Wire into `list_tools_by_capability()`
  (`greentic-designer/src/ui/routes/extensions.rs:437-461`): when
  `?capability=agentic_worker`, merge component-catalog tools alongside extension tools.
  Each tool's `extension_id` = `component:<ref>`, carrying the real description + input schema.
- Ensure the designer's tool binding serializes `component:<ref>` ToolRefs into the
  `AgentConfig` (the runtime already dispatches these — verified).
- **Done when:** registering a component URL makes its operations appear in the dw-composer
  tool picker with real descriptions, with NO hand-authored extension. The composed worker's
  `AgentConfig.tools` contains the `component:` refs.

### Session 5 — `gtdx` registration command
**Goal:** The CLI UX Bima promised. Under Path A this *registers a URL*, it does not generate
a wrapper.
- `greentic-designer-sdk/crates/greentic-extension-sdk-cli` — add subcommand next to
  `Command::New` (`main.rs:23-65`, `commands/new.rs` for the clap pattern). Suggested:
  `gtdx component register --url <store/repo/oci> [--tenant ...] [--allowed-ops ...]`.
- It calls Session 3's admin API; description is pulled automatically (Session 2). No local
  path, no manual description — per Maarten.
- (Path B alternative, only if Session 0 chose B: `gtdx extension new --component <url>`
  generates + signs a `describe.json` wrapper instead.)
- **Done when:** `gtdx component register --url <oci>` makes the component's tools show up in
  the designer for that tenant.

### Session 6 — End-to-end verification + tests
**Goal:** Prove the whole chain on a real run (closes the "proven in manual testing" gap that
currently only holds in the playground).
- Build a flow/DW that uses a registered component tool, bundle it
  (`greentic-bundle wizard apply`), and run via `gtc start` (see memory
  [[run-dw-application-gtpack]] for the exact bundle→start steps and provider requirements).
- Confirm: tool appears → LLM calls it → component executes → result returns. Capture logs.
- Add an integration test covering register → list → dispatch.
- **Done when:** a registered-by-URL component is callable by a live agentic worker through
  `gtc start`, with a test guarding the path.

### Session 7 — Composer UX polish + docs
**Goal:** Make it usable and documented.
- dw-composer: surface registered components in the tool picker with provenance ("from
  component <ref>"), reuse the `ToolConfigModal` usage-instructions feature (memory
  [[dw-tool-usage-instructions-feature]]) — usage_hint can default from the component
  description.
- Docs: update canonical docs in the same PR as code (workspace rule). i18n: any new strings
  must touch all 5 locales (en/es/id/ja/zh) or `locale-parity.test.ts` fails.
- **Done when:** docs describe "register a component as agentic-worker tools by URL", and the
  composer flow is clean.

---

## Cross-cutting rules (from CLAUDE.md — do not violate)
- `cd` into each sub-repo; run `bash ci/local_check.sh` before declaring done.
- No `unwrap()`/`panic!()` in production paths; `anyhow`/`thiserror`. `#![forbid(unsafe_code)]`.
- English only in source/tests/commits. Conventional Commits.
- Reuse-first: check shared crates before adding types. The `agents`/`dw.agent` plumbing is
  already in `greentic-types` 1.2.0-research + `greentic-bundle`.
- greentic-types bump cascades — coordinate the release train ([[greentic-types-release-train-gate]]).
- Designer origin is greentic-biz; PRs target `research`; no Claude co-author trailer there
  ([[greentic-workspace-gotchas]]).

## Parallelization across sessions

These sessions split cleanly across **different repos**, so parallel work has **no git
conflicts** — the only coupling is the 3 data contracts frozen in Session 0. Once S0 lands,
fan out into 3 independent tracks (each can be a separate Claude Code session / checkout).

```
S0  ── lock 3 contracts + spike  (GATE — must finish before fan-out)
        │
        ├── TRACK A:  S1 descriptions       (greentic-types + greentic-component)
        ├── TRACK B:  S3 admin registration  (greentic-designer-admin) — 0 deps, start immediately
        └── TRACK C:  S2 fetch + introspect  (lib; start on stubbed struct, finalize after S1)
                          │
                   ┌──────┴──────┐   convergence (needs A + B + C)
                   S4 adapter + designer ──┬── S5 gtdx  (needs only Track B; can start once S3 done)
                                           └── S6 e2e ── S7 UX/docs
```

| Track | Session | Repo | Can start | Depends on |
|-------|---------|------|-----------|------------|
| A | S1 | greentic-types, greentic-component | after S0 | contract #1 |
| B | S3 | greentic-designer-admin | after S0 (0 code deps) | contract #2 |
| C | S2 | fetch/introspect lib | after S0 (stub first) | contract #1 (to finalize) |
| — | S4 | greentic-designer | after A + B + C | all 3 contracts |
| — | S5 | greentic-designer-sdk (gtdx) | after B | contract #2 |
| — | S6 | greentic / greentic-bundle | after S4 + S5 | — |
| — | S7 | greentic-designer | after S4 | — |

**Convergence point = S4.** It's the only session that needs all three tracks, so treat
A/B/C finishing as the sync barrier.

**Coordination hazard:** S1 bumps `greentic-types`, which cascades to the consumer graph
(release-train gate — see [[greentic-types-release-train-gate]]). Track C and S4 must pin to
the research prerelease that carries the new `description` field. Agree the version string in
S0 so the parallel tracks don't fight over it.

## Dependency order (linear fallback if not parallelizing)
```
S0 → S1 → S2 → S3 → S4 → S5 → S6 → S7
```
S1 and S2 are independent and can always run in parallel even in the linear path.

---

# Appendix A — Frozen Contracts (Session 0 output)

These three contracts are LOCKED so Tracks A/B/C can proceed independently. Changing any of
them after fan-out forces re-sync across tracks — treat as a coordinated change.

## What S0 already discovered in code (important — narrows the work)

The runtime-side component-as-tool path is **already built and tested**, not just wired:

- `greentic_aw_runtime::ComponentToolSource` + `ComponentToolCatalog` + `ComponentInvoker`
  trait exist (`greentic-runner/crates/greentic-aw-runtime/src/component_source.rs`).
- The runtime `ComponentOperation` (`component_source.rs:53-58`) **already carries
  `description: String`** and `parameters`.
- `tools.rs:79-92` (list) and `:171-191` (dispatch) already route `component:<ref>` tools.
- Proven end-to-end by `tests/component_loop.rs` (mock LLM + fake invoker, no API key):
  tool offered → called → result in trail.

So the gap is NOT the runtime. It is: **(A)** the *manifest source* of descriptions
(`greentic_types::ComponentOperation` has none, so the runner-host invoker synthesizes
boilerplate), and **(B)** the designer-side discovery + admin registration that lets a
component be pointed at by URL.

## Contract #1 — operation description on the manifest type

`greentic_types::ComponentOperation` (`greentic-types/src/component.rs:168-175`) today:
```rust
pub struct ComponentOperation {
    pub name: String,
    pub input_schema: serde_json::Value,
    pub output_schema: serde_json::Value,
}
```
**Frozen change (Track A / S1):** add
```rust
    /// Human/LLM-facing description, sourced from the component's WIT doc comment
    /// at `prepare`/`describe` time. `None` when the WIT carries no doc.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Option::is_none"))]
    pub description: Option<String>,
```
- `Option<String>`, `serde(default, skip_serializing_if)` — backward compatible with existing
  manifests (older packs deserialize with `None`).
- **Consumer mapping (runner-host invoker,
  `greentic-runner-host/src/runner/component_invoker.rs:46-65`):** prefer
  `op.description` when `Some`; fall back to the current synthesized
  `"Invoke operation '{op}' of component '{ref}'"` when `None`. This feeds the already-existing
  runtime `ComponentOperation.description: String`.
- **Source of the value (Track A / S1):** WIT `func.docs.contents`, already extracted in
  `greentic-component/crates/greentic-component/src/describe.rs:139-167` — wire it into the
  manifest operation build (`prepare.rs`).
- **Version:** bumps `greentic-types`; coordinate the research prerelease string in S0 (see
  [[greentic-types-release-train-gate]]). Track C + S4 pin to that version.

## Contract #2 — `component_tools` admin registration JSON

Mirror `McpServerForDesigner` (`greentic-designer-admin/src/domain/mcp_server.rs:65-93`) and
its local-wasm fields, which already carry exactly what we need. **Frozen designer-read shape**
(`GET /api/v1/designer/tenant/me/component-tools`):
```json
{
  "components": [
    {
      "id": "refund-tools",
      "name": "Refund tools",
      "component_ref": "greentic.refund",
      "component_version": "1.2.3",
      "component_digest": "<sha256-hex>",
      "source_url": "oci://ghcr.io/acme/refund:1.2.3",
      "allowed_operations": ["issue_refund"],
      "roles": ["agentic_worker"],
      "operations": [
        {
          "name": "issue_refund",
          "description": "Issue a refund for an order",
          "input_schema": { "type": "object", "properties": { "order_id": { "type": "string" } } }
        }
      ]
    }
  ]
}
```
- `roles` reuses `MCP_ROLE_AGENTIC_WORKER` = `"agentic_worker"` (same constant family).
- `allowed_operations` = same semantics as MCP `allowed_tools`: `None` → all, `Some([])` →
  none, else whitelist.
- `operations[]` is the cached introspection result (mirror `McpToolInfo {name, description,
  input_schema}`), populated at registration by Track C's introspector.
- `source_url` accepts store / repo / OCI forms — the input Maarten specified.
- Admin CRUD mirrors `/api/admin/tenants/{id}/mcp-servers` →
  `/api/admin/tenants/{id}/component-tools`. New SQLx table + migration (use
  `scripts/new-migration.sh`, never hand-pick a counter).
- **Consumers:** S4 (`ComponentToolSource` in designer reads `GET …/component-tools`), S5
  (`gtdx` writes a row via admin CRUD).

## Contract #3 — `component:<ref>` ToolRef convention (already implemented; frozen as-is)

- `AgentConfig.tools[]` entry: `ToolRef { extension_id: "component:<component_ref>", tool_name:
  "<operation>" }` (`greentic-aw-runtime/src/config.rs:15-18`).
- The runtime list/dispatch keys every tool by the `(component_ref, operation)` tuple and
  strips the `component:` prefix (`tools.rs:79`, `:171`). **No change** — recorded here so no
  track re-litigates the prefix or invents `agent:`/`tool:` variants.
- S4 must serialize designer tool bindings into this exact shape; the runtime then dispatches
  with zero additional wiring.

## S0 spike result — PASS (2026-06-25)

```
cd greentic-runner && cargo test -p greentic-aw-runtime --features test-mock --test component_loop
running 2 tests
test empty_component_source_offers_no_tool ... ok
test component_tool_offered_called_and_result_in_trail ... ok
test result: ok. 2 passed; 0 failed
```
Proves the runtime `component:` path end-to-end with no LLM key: a component tool is offered to
the LLM, called, and its output lands in the trail; an empty source degrades to "no tools"
without panic. Confirms the runtime is NOT the gap.

# Progress log (verified by running tests, not agent claims)

**2026-06-25 (later) — S2 / Track C v1 built & GREEN (fetch + introspect-names).**
New module `greentic-component/crates/greentic-component/src/resolve.rs` (gated
`#[cfg(all(feature = "store", feature = "describe"))]`) + integration test
`tests/resolve_component.rs`. Registered in `lib.rs`.
- `resolve_component(reference, cache_dir) -> ResolvedComponent { component_ref, digest,
  operations: [{name, input_schema: Option, description: Option}] }`. Fetch via the
  **non-deprecated** distributor path `DistClient::parse_source → resolve → fetch` (NOT the
  deprecated `ensure_cached`; error type is `greentic_distributor_client::dist::DistError`).
  Introspect via `describe::from_wit_world(path, "")` (empty preferred-world → uses the world the
  component declares).
- **Verified:** unit tests (mapping/dedup/docs) 2/2 pass; **integration test fetches a REAL
  committed fixture (`tests/contract/fixtures/component_v0_6_0/component.wasm`) over `file://`,
  introspects, returns operations + digest — 2/2 pass, offline, no false-green.** `cargo fmt`
  clean; `cargo clippy --lib --features store,describe` clean (no warnings).
- **Scope of v1 (honest):** operations carry NAMES only. `input_schema` is `None` (WIT
  introspection does not yield it — confirmed) and `description` is `None` (the S1 source
  question). Schema/description **enrichment is the next slice** — it needs the component's
  manifest or describe export (the distributor `ResolvedArtifact` exposes `describe_artifact_ref`
  but no manifest), so the agentic worker can only see tool *names* until that lands.
- Did NOT run the package's full `--all-targets` clippy/test (it pulls heavy `cli`-feature test
  binaries unrelated to this change, and the box hit a disk-full during the attempt; freed by
  `cargo clean` on designer-admin's 230G target). Lib + the new tests are green.



**2026-06-25 (later) — ⚠️ S1 design flaw found: WIT `///` docs do NOT survive the build.**
While closing the component test-gap I added a REAL end-to-end test (build a WIT with a `///`
doc on `describe` → run the real `prepare_component` → assert the manifest op gets the doc).
**It FAILED (`left: None`).** Root cause, verified by dumping the real describe payload:
`wit_component::metadata::encode` → `decode_world` **strips `///` doc comments**, so
`describe::from_wit_world` emits `functions: [{key, name}]` with NO `docs`. And `describe::load`
tries `from_wit_world` FIRST and always succeeds for a valid component, so the baked-schema
fallbacks never run. Net: **`apply_operation_descriptions` is effectively dead code — it will
never populate a description from WIT docs in production.** The agent's 3 unit tests passed only
because they hand-built a `DescribePayload` with the `docs` shape the real pipeline never emits.
components-public ships no baked `schemas/v1` with docs either.
- Left the real e2e test in-tree as `#[ignore]` with a full explanation (living record). The 4
  agent unit tests still pass (they document the function's logic, not the real source).
- **Implication — the "description from the wasm" source must change.** Maarten's words were
  "each component has a description" → he likely means the **component-level** description, not
  per-WIT-function `///` docs. `greentic_types::ComponentOperation` also already had an
  `/// Operation-level descriptions` doc on a neighbouring field — operation descriptions are
  expected to be **authored in the manifest/describe**, not scavenged from WIT. **S1 needs a
  design correction before it delivers value** (options: component-level description on the
  tool; author per-op descriptions in the manifest/baked describe.json + reorder
  `describe::load` to prefer a docs-bearing schema; or make the build preserve WIT docs).
- The `greentic-types` field addition itself is still correct & useful (the runtime/designer
  need a place to carry the description regardless of source). Only the greentic-component
  WIT-scavenging wiring is the wrong source.

**2026-06-25 (later still) — deeper finding: NO automatic description source exists today.**
Before implementing an S1 fix I verified what description data actually exists/survives:
- `ComponentManifest` (`greentic-component/.../manifest/mod.rs:34`) has **no `description`/
  `summary` field** — only `name`.
- Real components-public manifests carry only the component `name`, per-operation `name`, and
  input/output schemas. The only `"description"` anywhere is on a *secret_requirement*, not the
  component or its operations.
- So all three "automatic" paths are unavailable: WIT `///` docs (stripped by the build),
  component-level description (field doesn't exist), authored op descriptions (components don't
  write them). **Maarten's premise "each component has a description" does not hold for the
  current component corpus — they have a `name`, not a description.**
- **S1 is therefore blocked on a PRODUCT decision, not code.** Options to surface to Maarten:
  (1) ship synthesized-from-name descriptions now (zero new data, weak quality — the runtime
  fallback already does this); (2) add an authored `description` to `ComponentManifest` +
  author discipline + backfill existing components; (3) make the build preserve WIT `///` docs
  (pipeline change) for the truly-automatic experience. **No more S1 code until this is chosen.**
- **Track C (S2 fetch+introspect) is NOT blocked by this** — it can list a component's
  operations by `name` + `input_schema` (which DO survive); the description column is filled by
  whatever source wins later. C remains viable to build now.


**2026-06-25 — S0 done; S1 (Track A) + S3 (Track B) implemented & verified locally:**

- **S3 / Track B — greentic-designer-admin: ✅ GREEN, self-contained, no blocker.**
  New: `migrations/20260625085030_tenant_component_tools.sql`, `src/domain/component_tool.rs`,
  `src/repo/component_tools/`, `src/routes/admin/tenant_component_tools/`,
  `src/routes/designer/component_tools.rs`, `tests/api_admin_tenant_component_tools.rs`; wired
  into authz + the admin/designer routers. **`cargo test --test
  api_admin_tenant_component_tools` → 7/7 pass** (frozen-contract shape, tenant scoping, role
  validation, auth, full CRUD). NOT yet run: full `local_check.sh` (npm typecheck/lint/build +
  clippy -D warnings). Has a `// TODO(S2)` where URL probe will populate `operations[]`.
- **S1 / Track A — greentic-types: ✅ source change good.** Added
  `ComponentOperation.description: Option<String>` (additive, `serde(default,
  skip_serializing_if)`); lib `component` tests **13/13 pass** incl. backward-compat serde.
  Caveat: whole-crate `cargo test --all-features` is red ONLY due to the pre-existing untracked
  `tests/capabilities_auth_caps.rs` (capability-fields breakage, unrelated to this change).
- **S1 / Track A — greentic-component: ✅ wiring + tests pass.** `prepare.rs`
  `apply_operation_descriptions()` copies WIT-function doc → manifest op description
  (manifest-authored value wins). **4 new prepare tests pass.** ⚠️ Compiles only via a
  **`[patch.crates-io] greentic-types = { path = ... }` marked DO-NOT-COMMIT** in `Cargo.toml`
  — must be removed and replaced by a published-version dep before commit.
  ⚠️ **Test gap:** the positive case uses a hand-built `DescribePayload`; the real-WIT-with-doc
  end-to-end path is only covered for the *no-doc* case. Add a real `///`-documented WIT fixture
  test asserting the doc reaches the manifest before merge (de-risks the
  `version.schema.functions[].docs` shape assumption).
- **S1 / Track A — greentic-runner-host: ⚠️ consumer code correct, ❌ does NOT compile yet.**
  `component_invoker.rs` now prefers `op.description` (clean, with empty-string guard), but the
  runner pins `greentic-types = "=1.2.0-research.1"` (published, lacks the field) → `cargo check
  -p greentic-runner-host` fails `E0609 unknown field description`. **BLOCKED on the release
  train:** publish greentic-types with `description` as a new research prerelease, then bump the
  runner pin (and replace greentic-component's path patch). See
  [[greentic-types-release-train-gate]].

**Merge ordering:** Track B can PR independently now (after full `local_check.sh`). Track A is a
coordinated release-train change: publish greentic-types prerelease → unpatch + bump
greentic-component → bump runner pin → runner-host goes green.

**Verification lesson:** background `cargo … | tail` masks cargo's exit code (pipeline returns
`tail`'s 0). Both "exit 0" notifications were false-green; always read the cargo text, never
trust the piped exit code.

## Decision — Path A (confirmed 2026-06-25)
**A — single dynamic admin-registered ComponentExtension adapter.** Register a component by
store/repo/OCI URL into a per-tenant catalog (`component_tools`); tools appear automatically,
mirroring the MCP-Adapter. `gtdx` (S5) is the registration UX (writes a catalog row), NOT a
wrapper generator. Path B is dropped. All sessions below follow Path A as written.
