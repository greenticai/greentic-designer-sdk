# EPIC-F v1 — `gtdx openapi` — OpenAPI→MCP Connector Generator — Design Spec

**Status:** Draft — 2026-07-06
**Initiative:** Agentic platform coverage PRD, EPIC-F "Business Connector Catalog". Turns an OpenAPI spec into a Greentic connector so agents get 1-click API integrations instead of hand-rolled HTTP.

## 1. Problem & goal

No OpenAPI→connector generator exists (verified: `gtdx` has `new`/`publish` but nothing that reads a spec; no OpenAPI parser crate in the workspace). Business connectors (Stripe, Shopify, …) are hand-written per API (`component-hubspot-ext`, `component-github-mcp-ext`) — exactly the artifacts a generator should emit.

**Goal:** a `gtdx openapi <spec>` command that parses an OpenAPI 3.0 spec and **scaffolds a `DesignExtension` connector** (Rust source, codegen) whose `list_tools` surfaces one tool per API operation and whose `invoke_tool` performs the HTTP call via the host `extension-host/http.fetch` import (secret injection + network allow-list, proven by `component-http-ext`). The generated connector then builds + publishes via the existing `gtdx publish --wasm`.

## 2. Why DesignExtension (Path B), not `wasix:mcp/router` (Path A)

Two rails surface per-operation tools. **DesignExtension** (`greentic:extension-design/tools@0.2.0`, imports `extension-host/http`) gives one-line HTTP + secret-injection + allow-list — the `component-http-ext` skeleton. **`wasix:mcp/router`** imports nothing and would need a modified WIT world (`import wasi:http`), a wasip2 HTTP-client crate, and network permissions — materially more codegen. So the generator emits a **DesignExtension** connector (like the hand-written `component-hubspot-ext`), just generated from the spec. (A config-only path for a generic executor is not viable — none exists that yields per-operation tools.)

## 3. Architecture

### 3.1 The command
`gtdx openapi <spec-path> [--name <id>] [--out <dir>] [--base-url <override>]` — a new clap variant (`src/main.rs` + `src/commands/openapi/mod.rs`, `pub mod openapi;` in `commands/mod.rs`). It: parses the spec → builds an intermediate `ConnectorModel` → renders the connector into `<out>/<name>/` reusing the scaffold's template + WIT-lock machinery. The result is a ready-to-`gtdx publish --wasm` connector.

### 3.2 Parse → `ConnectorModel` (Task 1, pure)
Add `openapiv3` to the CLI crate. Parse the spec (JSON or YAML) into a pure model:
```
ConnectorModel { name, version, base_url (from servers[0] or --base-url), security: Option<AuthScheme>, tools: Vec<ToolModel> }
ToolModel { name (operationId), description (summary/description), method, path_template, params: Vec<Param{name, location(Path|Query), required, schema}>, body: Option<JsonSchema>, input_schema: serde_json::Value (JSON Schema merging params + body) }
AuthScheme { kind (Bearer | ApiKey{header_name}), secret_ref }  // from the first securityScheme
```
Scope (v1): OpenAPI 3.0; `GET`+`POST` operations with an `operationId`; `application/json` request bodies; `path`+`query` params (default style/explode); inline or one-level `$ref` schemas. **Deferred:** `oneOf`/`anyOf`/`allOf`/discriminators, multipart/form-urlencoded, cookie/header param styles, callbacks/webhooks, `$ref` cycles, OAuth2 flows (bearer/apiKey only).

### 3.3 Codegen → the connector (Task 2)
Render a `DesignExtension` connector, mirroring `component-http-ext`'s skeleton:
- `templates/openapi-connector/` (new): `Cargo.toml.tmpl`, `describe.json.tmpl` (`kind: DesignExtension`; `runtime.permissions.network` derived from `servers[]`; `secrets`/`secret_requirements` from the securityScheme), `wit/world.wit.tmpl` (import `greentic:extension-host/http` + `secrets`, export `greentic:extension-design/tools`), `src/lib.rs.tmpl` (static WIT glue: `list_tools` returns `tool_meta::TOOLS`, `invoke_tool` dispatches to `dispatch::invoke`), `rust-toolchain.toml.tmpl`, `build.sh.tmpl`. Register the dir in `scaffold/template.rs`.
- **Generated (written directly from code, NOT the `{{}}` engine, which can't express N tools):**
  - `src/tool_meta.rs` — `pub const TOOLS: &[ToolDef]` (one per `ToolModel`: name, description, input_schema_json).
  - `src/dispatch.rs` — `invoke(name, args_json) -> Result<String, Error>`: match on tool name → substitute path params into the URL template → append query params → attach auth (bearer/apiKey secret via `secrets::get`) → `http::fetch` → return the body. Mirror `component-http-ext/src/lib.rs::invoke_tool`.
- The `describe.json` tool caching is dynamic (admin runs `list_tools` at registration — root CLAUDE.md), so `describe.json` needs `kind`/`metadata`/`runtime` valid; the tools live in the wasm's `list_tools`.

### 3.4 Reuse the scaffold plumbing
Reuse `scaffold::template::{Context, write_file}` + `new::write_wit_and_lock` (the `.gtdx-contract.lock` sha256 of WIT deps) so the generated connector is contract-locked like a `gtdx new` one. The `openapi` command shares `new`'s WIT-dep embedding for the DesignExtension world.

## 4. Testing (offline)
- **Parse (Task 1):** a fixture OpenAPI spec (a tiny 2-operation API: `GET /pets/{id}`, `POST /pets`) → `ConnectorModel` with the right tools/params/input_schema/auth. Edge cases: missing operationId (skip + warn), `$ref` param, required vs optional, apiKey vs bearer security.
- **Codegen (Task 2):** generate from the fixture spec → **insta snapshots** of `describe.json` + `tool_meta.rs` + `dispatch.rs` + `world.wit` (proves deterministic, correct codegen). Assert each tool's `input_schema` is valid JSON (mirror `component-http-ext/src/tool_meta.rs`'s host schema-validity test).
- **Generated connector compiles (Task 3):** generate a connector into a temp dir, then host-compile its pure modules (`tool_meta`/`dispatch`) — OR a golden-connector checked-in fixture that the SDK's own tests build. Proves the emitted Rust is valid.
- **Runtime HTTP:** the generated `invoke` uses the host `http::fetch` import → mockable in a host harness (greentic-mcp ships offline mock paths); a `wiremock`-style test exercises `invoke` against a local server. **Real third-party API = the one thing not offline-testable** → an opt-in/online test + the pre-enablement checklist.

## 5. Scope boundaries (YAGNI)
**In v1:** `gtdx openapi <spec>` → DesignExtension connector for GET+POST/JSON/path+query/bearer|apiKey; parse+codegen offline tests; the generated connector is `gtdx publish --wasm`-ready.
**Deferred:** the OpenAPI features in §3.2; OAuth2; response-schema→output-schema; pagination; rate-limit; a curated catalog of specific connectors (Stripe/Shopify/…) — that's content on top of the generator (E-F v2); publishing automation.

## 6. Pre-enablement (live — not CI)
Generating + building the connector is offline. Actually calling a real API needs: the API's base URL reachable, a valid secret (bearer/apiKey) for the target service, and the tenant registering the published connector. The generator + the mock-http test prove the mechanics; a real Stripe/Shopify call is a pre-enablement verification.

## 7. Rollout
Additive `gtdx` subcommand + a new template dir + the `openapiv3` dep; no change to existing `gtdx` commands. Target `research` (greentic-designer-sdk). Follow-ups: the deferred OpenAPI features; a curated first connector (e.g. Stripe) as an EPIC-F v2 content slice; response/output schemas.
