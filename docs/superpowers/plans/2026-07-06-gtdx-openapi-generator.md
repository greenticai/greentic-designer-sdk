# `gtdx openapi` Generator Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `gtdx openapi <spec>` parses an OpenAPI 3.0 spec and scaffolds a `DesignExtension` connector (Rust codegen) — one tool per GET/POST operation, `invoke_tool` via the host `extension-host/http.fetch` — ready for `gtdx publish --wasm`.

**Architecture:** A pure parse layer (`openapiv3` → `ConnectorModel`), a codegen layer (model → the connector's `describe.json` + generated `tool_meta.rs`/`dispatch.rs` + a new `templates/openapi-connector/` static-glue template), and the `gtdx openapi` command wiring. Mirrors the hand-written `component-http-ext` skeleton.

**Tech Stack:** Rust (edition 2024), `openapiv3` (NEW dep), `serde_json`, `insta` (snapshot tests, already used), the `gtdx` scaffold machinery (`scaffold::template`, `new::write_wit_and_lock`).

## Global Constraints

- **Crate:** `greentic-designer-sdk/crates/greentic-extension-sdk-cli` (the `gtdx` CLI). Reference the hand-written `component-http-ext` (`src/lib.rs::invoke_tool`, `src/tool_meta.rs`, `src/input.rs`) as the exact skeleton the generated connector clones — READ it (it's a sibling repo `component-http-ext`).
- **Target = DesignExtension** (`export greentic:extension-design/tools@0.2.0`, `import greentic:extension-host/http` + `secrets`), NOT `wasix:mcp/router`. The generated `describe.json` is `kind: DesignExtension` with `runtime.permissions.network` from the spec `servers[]` + `secrets`/`secret_requirements` from the securityScheme.
- **Codegen for N tools is done in CODE** (build the Rust/JSON strings + write files directly), NOT via the scaffold's `{{key}}` string-replace engine (which errors on unsubstituted `{{}}` and can't express N variable tools). The static glue (Cargo.toml, world.wit, lib.rs, describe.json shell) CAN use the `{{}}` engine.
- **Scope (v1):** OpenAPI 3.0; `GET`+`POST` with an `operationId`; `application/json` bodies; `path`+`query` params (default style); inline / one-level `$ref` schemas; a single `http bearer` or `apiKey` securityScheme. Skip (warn) operations missing an `operationId` or using unsupported features. Defer: oneOf/anyOf/allOf, multipart, cookie/header params, OAuth2, callbacks, `$ref` cycles.
- **Offline only:** parse + codegen + "generated connector compiles" are offline-verifiable. Real third-party API calls are NOT tested (pre-enablement); the generated `invoke` is exercised against a mock host-`http` in a later test.
- **No change to existing `gtdx` commands.** **Conventional commits, NO Claude co-author.** Target `research`.
- **Build discipline (SHARED CONTENDED MACHINE, disk ~40GB dropping):** all cargo with `-j2` + `CARGO_BUILD_JOBS=2`; FOREGROUND; if OOM-killed retry `-j1` then STOP+report; NEVER pkill/kill or delete another worktree's `target/`.

---

### Task 1: OpenAPI parse → `ConnectorModel`

**Files:**
- Create: `crates/greentic-extension-sdk-cli/src/commands/openapi/model.rs` (+ `mod.rs` with `pub mod model;`) + `pub mod openapi;` in `src/commands/mod.rs`
- Modify: `crates/greentic-extension-sdk-cli/Cargo.toml` (add `openapiv3`)
- Test: inline `#[cfg(test)]` + a fixture spec under `src/commands/openapi/fixtures/petstore-min.json`

**Interfaces:**
- Produces:
  - `enum ParamLoc { Path, Query }`; `struct Param { name: String, location: ParamLoc, required: bool, schema: serde_json::Value }`
  - `enum AuthScheme { Bearer { secret_ref: String }, ApiKey { header_name: String, secret_ref: String } }`
  - `struct ToolModel { name: String, description: String, method: String, path_template: String, params: Vec<Param>, body: Option<serde_json::Value>, input_schema: serde_json::Value }`
  - `struct ConnectorModel { name: String, version: String, base_url: String, security: Option<AuthScheme>, tools: Vec<ToolModel> }`
  - `fn parse_openapi(spec_bytes: &[u8], name_override: Option<&str>, base_url_override: Option<&str>) -> anyhow::Result<ConnectorModel>` — parse (JSON or YAML) via `openapiv3::OpenAPI`; for each GET/POST op with an `operationId`, build a `ToolModel` (input_schema = a JSON Schema `{type:object, properties: {<each path+query param>, <requestBody json props>}, required: [...]}`); derive `security` from the first `securityScheme` (bearer/apiKey); `base_url` from `servers[0].url` (or the override). Skip+warn on missing operationId / unsupported method.

- [ ] **Step 1: Add the fixture** `petstore-min.json` — a 2-operation OpenAPI 3.0 spec: `GET /pets/{id}` (path param `id`, query param `verbose`), `POST /pets` (JSON body `{name, tag}`), a `bearerAuth` securityScheme, one `servers` entry.
- [ ] **Step 2: Write failing tests.**
```rust
#[test]
fn parses_petstore_into_two_tools() {
    let model = parse_openapi(include_bytes!("fixtures/petstore-min.json"), None, None).unwrap();
    assert_eq!(model.tools.len(), 2);
    let get = model.tools.iter().find(|t| t.method == "GET").unwrap();
    assert_eq!(get.path_template, "/pets/{id}");
    assert!(get.params.iter().any(|p| p.name == "id" && matches!(p.location, ParamLoc::Path) && p.required));
    // input_schema has `id` + `verbose` properties, `id` required
    assert_eq!(get.input_schema["properties"]["id"]["type"], "string");
    assert!(get.input_schema["required"].as_array().unwrap().iter().any(|v| v == "id"));
    let post = model.tools.iter().find(|t| t.method == "POST").unwrap();
    assert!(post.input_schema["properties"]["name"].is_object()); // from requestBody
    assert!(matches!(model.security, Some(AuthScheme::Bearer { .. })));
    assert!(model.base_url.starts_with("http"));
}
#[test]
fn skips_operation_without_operation_id() { /* a spec op missing operationId is skipped, not an error */ }
```
- [ ] **Step 3: Run — expect FAIL** (`CARGO_BUILD_JOBS=2 cargo test -p greentic-extension-sdk-cli -j2 openapi::model`).
- [ ] **Step 4: Implement** `parse_openapi` + the model. Resolve one-level `$ref`s for params/schemas; merge params + requestBody into `input_schema`. Never panic on a malformed spec (return `Err`).
- [ ] **Step 5: PASS + commit** (`feat(gtdx): openapi spec → ConnectorModel parser`).

---

### Task 2: codegen + the `gtdx openapi` command

**Files:**
- Create: `src/commands/openapi/codegen.rs` (model → generated file strings) + `src/commands/openapi/mod.rs` (the `run(args)` command)
- Create: `templates/openapi-connector/` (`Cargo.toml.tmpl`, `describe.json.tmpl`, `wit/world.wit.tmpl`, `src/lib.rs.tmpl`, `rust-toolchain.toml.tmpl`, `build.sh.tmpl`)
- Modify: `src/main.rs` (add `Openapi(commands::openapi::Args)` variant + match arm), `src/scaffold/template.rs` (register the `openapi-connector` template dir)
- Test: inline `#[cfg(test)]` with `insta` snapshots

**Interfaces:**
- Consumes: Task 1's `ConnectorModel`; `scaffold::template::{Context, write_file}`; `new::write_wit_and_lock` (the WIT-lock helper).
- Produces:
  - `fn gen_tool_meta(model: &ConnectorModel) -> String` — the `src/tool_meta.rs` source: `pub const TOOLS: &[ToolDef] = &[ ToolDef { name, description, input_schema } , ... ];` (input_schema embedded as a JSON string literal).
  - `fn gen_dispatch(model: &ConnectorModel) -> String` — `src/dispatch.rs`: `pub fn invoke(name: &str, args: &serde_json::Value) -> Result<String, String>` matching each tool → build URL from `base_url` + path-substitution + query → auth header from `secrets::get(<secret_ref>)` → `host_http::fetch(...)` → return body. Mirror `component-http-ext/src/lib.rs::invoke_tool`.
  - `fn run(args: Args) -> anyhow::Result<()>` — read spec → `parse_openapi` → render the static template (Context vars: name/id/version/base_url/network-allowlist/secret-name) → write `tool_meta.rs`/`dispatch.rs` directly → `write_wit_and_lock` → print the output path + the `gtdx publish --wasm` next step.

- [ ] **Step 1: Read `component-http-ext`** (`src/lib.rs` invoke_tool, `src/tool_meta.rs`, its `describe.json`, its `wit/world.wit`) — this is the exact shape the generated connector clones. Read `gtdx new`'s `render_templates`/`write_wit_and_lock` + `scaffold::template` API.
- [ ] **Step 2: Write failing snapshot tests** — `gen_tool_meta(&fixture_model)` + `gen_dispatch(&fixture_model)` produce the expected Rust (insta snapshots); the generated `tool_meta` TOOLS' `input_schema` strings each parse as valid JSON; `run` on the fixture spec into a tempdir writes `describe.json` (kind DesignExtension, network allow-list from servers, secret from securityScheme), `src/tool_meta.rs`, `src/dispatch.rs`, `wit/world.wit`, `Cargo.toml`, `.gtdx-contract.lock`.
- [ ] **Step 3: Run — expect FAIL.**
- [ ] **Step 4: Implement** `gen_tool_meta`/`gen_dispatch`/`run` + the `openapi-connector` templates + the `main.rs`/`template.rs` wiring.
- [ ] **Step 5: PASS + commit** (`feat(gtdx): openapi command generates a DesignExtension connector`).

---

### Task 3: prove the generated connector compiles + a golden connector

**Files:**
- Create: `tests/openapi_generated_compiles.rs` (or extend) — generate from the fixture spec, then compile-check the generated pure modules
- Test: a golden connector build

**Interfaces:**
- Consumes: Task 2's `run`.

- [ ] **Step 1: Read** how the SDK tests compile scaffolded output (grep for a `gtdx new` → `cargo build` integration test; mirror it). Determine the cheapest proof: (a) generate into a tempdir + `cargo check` the generated crate (host target — the pure `tool_meta`/`dispatch` compile without wasm), or (b) a checked-in golden connector built as a fixture.
- [ ] **Step 2: Failing test** — generate a connector from `petstore-min.json`, then `cargo check` its generated crate (or host-compile `tool_meta.rs`+`dispatch.rs`); assert it succeeds. (The generated `dispatch.rs`'s host-fn imports are `#[cfg(target_arch=wasm32)]`-gated like `component-http-ext`, so the pure logic host-compiles.)
- [ ] **Step 3: Run — expect FAIL** (if the generated code has a bug, this catches it).
- [ ] **Step 4: Fix** any codegen defects surfaced (this is the task that proves the emitted Rust is real).
- [ ] **Step 5: Gate + commit.** `cargo fmt --all`; `CARGO_BUILD_JOBS=2 cargo clippy -p greentic-extension-sdk-cli -j2 --all-targets -- -D warnings`; `CARGO_BUILD_JOBS=2 cargo test -p greentic-extension-sdk-cli -j2`. Commit (`test(gtdx): generated openapi connector compiles`). Then finishing-a-development-branch → PR to `research` with the §6 pre-enablement note (real-API calls need a live service + secret; generation + mock are offline-tested).

---

## Self-Review

- **Spec coverage:** §3.2 parse → Task 1; §3.3 codegen + §3.1 command → Task 2; §4 "generated compiles" → Task 3; §4 parse/codegen offline tests → Tasks 1-2; §6 pre-enablement (real API) → Task 3 PR note. §3.4 scaffold reuse → Task 2 (`write_wit_and_lock`).
- **Placeholder scan:** "read component-http-ext / gtdx new render_templates / the SDK's compile-test" are deliberate — the exact DesignExtension skeleton, the scaffold API, and the WIT-lock helper must be read from the repo. Codegen strings are built in code (not the `{{}}` engine) per the Global Constraints. No TBD as work-defining; real-API runtime intentionally not tested (offline).
- **Type consistency:** `ConnectorModel`/`ToolModel`/`AuthScheme`/`Param` (Task 1) consumed by `gen_tool_meta`/`gen_dispatch`/`run` (Task 2); the generated connector (Task 2) compiled by Task 3. `openapiv3` dep added once (Task 1).
- **Scope:** a new `gtdx` subcommand + parse + codegen + template + one dep; bounded OpenAPI subset; existing commands untouched; real-API verification deferred.
