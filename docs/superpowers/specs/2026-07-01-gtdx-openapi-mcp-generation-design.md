# Design: `gtdx` generates an MCP extension from an OpenAPI spec

- **Date:** 2026-07-01
- **Status:** Approved (design), pending implementation plan
- **Primary repo:** `greentic-designer-sdk` (`gtdx`)
- **Secondary repo:** `greentic-mcp-generator` (small metadata tweak)

## Problem

`gtdx` can scaffold an empty `wasix:mcp/router` skeleton (`gtdx new --kind mcp`,
one placeholder `<name>_echo` tool) and package a pre-built component
(`gtdx publish --wasm`), but it cannot **generate** a router from an OpenAPI /
Swagger spec. That capability lives only in `greentic-mcp-generator` (binary
`greentic-mcp-gen`). Authors who want an MCP extension from an API spec must run
two disjoint tools and hand-author `describe.json`.

We want `gtdx` to produce a ready-to-publish MCP extension **from an OpenAPI
spec** in one step, and to gain a real interactive wizard for `gtdx new`.

## Goals

- `gtdx new --kind mcp --from-openapi <spec>` produces a ready-to-publish MCP
  extension: generated router `.wasm` + an auto-authored `describe.json`.
- Auto-author `describe.json` completely: metadata + `runtime` wasm ref +
  `permissions.network` (from the spec's servers) + `secret_requirements` (from
  the generator's emitted tool/secret metadata).
- Add a genuine interactive wizard to bare `gtdx new` (prompts through the
  fields; for `kind == mcp`, offers OpenAPI seeding).
- Keep `gtdx` thin: shell out to `greentic-mcp-gen`; do NOT pull OpenAPI parsing
  (`openapiv3`) or the generator core into `gtdx`.

## Non-goals (YAGNI)

- Not a source-rebuildable (macro-based) project — MVP wraps the pre-built wasm.
- No automatic re-generate/diff when the spec changes (follow-up; `--force` re-run
  is the manual path).
- No fancy TUI — sequential prompts only.

## Decisions (locked during brainstorming)

| Decision | Choice |
| --- | --- |
| Command surface | `gtdx new --kind mcp --from-openapi <spec>` **and** an interactive wizard for bare `gtdx new` |
| Generation mechanism | Shell out to `greentic-mcp-gen` (managed external tool), consume its output |
| `describe.json` authoring | Full, including `permissions.network` — requires a small `greentic-mcp-generator` tweak to emit `servers` |

## Architecture — two repo-scoped parts

```
OpenAPI spec ──► gtdx new --kind mcp --from-openapi spec.yaml
                      │
                      │ shell out (resolve env→PATH, guided error if absent)
                      ▼
                greentic-mcp-gen --spec spec.yaml --output-dir <proj>
                      │  emits:
                      │   • <stem>.component.wasm
                      │   • tools.json            (MCP tools/list — per-tool schema + secret hints)
                      │   • component-meta.json   (NEW, SP-A: { servers, secret_requirements, oauth_scopes })
                      ▼
                gtdx auto-authors describe.json (kind: wasix:mcp/router)
                   metadata + runtime.world + runtime.components(wasm ref)
                   + permissions.network (from servers)
                   + secret_requirements (from meta)
                      ▼
                ready-to-publish project → gtdx publish --wasm <stem>.component.wasm --manifest ./Cargo.toml .
```

## SP-A — `greentic-mcp-generator`: emit `component-meta.json`

**Change:** alongside the existing `tools.json` + wasm, write a sidecar
`component-meta.json` in the output dir:

```json
{
  "servers": ["https://api.example.com"],
  "secret_requirements": [ /* consolidated from the per-tool secret_requirements */ ],
  "oauth_scopes": ["..."]
}
```

- `servers` already exists on the IR (`greentic-mcp-generator-core/src/ir.rs:11`,
  `pub servers: Vec<String>`); it is not currently written to disk.
- `secret_requirements` / `oauth_scopes` are already computed and baked per-tool
  (`greentic-mcp-generator-cli/src/gen/from_ir.rs:474-489`); consolidate them at
  the top level here.
- Backward-compatible: a new file, no change to `tools.json` or the wasm.
- Emitted on both the real build path and `--dry-run`.

**Boundary:** this is the only change in `greentic-mcp-generator`. It is a
standalone PR that can merge independently; `gtdx` degrades gracefully if the
file is absent (see Error handling).

## SP-B — `greentic-designer-sdk` (`gtdx`)

### B1. `--from-openapi` on `gtdx new`
- Add flag `--from-openapi <PATH>` (spec file) to `gtdx new`. Valid only with
  `--kind mcp`; error clearly otherwise.
- When set: skip the echo-skeleton templates. Instead:
  1. Resolve `greentic-mcp-gen` (env override then PATH). If absent, fail with a
     guided install message (`cargo binstall greentic-mcp-generator` +
     `GITHUB_TOKEN`, or install via the toolchain).
  2. Run it: `greentic-mcp-gen --spec <spec> --output-dir <target>`.
  3. Read `tools.json` + `component-meta.json` from the output dir.
  4. Auto-author `describe.json` (see B2).
  5. Copy/keep the spec in the project for reference; write a minimal
     `Cargo.toml` anchor so `--manifest` works for `gtdx publish`.
- Result: `gtdx publish --wasm <stem>.component.wasm --manifest ./Cargo.toml .`
  works immediately.

### B2. `describe.json` auto-authoring
Fill the `wasix:mcp/router` describe (same shape as the scaffold template):
- `metadata.id/name/version`: from `gtdx new` args (`--id/--name/--version`);
  `summary`/`description` seeded from the spec title if available.
- `runtime.world`: `{id_wit}/mcp-router@1.0.0` (unchanged pattern).
- `runtime.components.<key>`: point `gtpack.file` at the generated wasm
  (renamed to `extension.wasm` at pack time); sha256 left as the
  placeholder and bound by `gtdx publish` (unchanged behavior).
- `permissions.network`: hosts derived from `component-meta.json.servers`.
- `secret_requirements`: from `component-meta.json.secret_requirements`.
- `capabilities`, `permissions.secrets/callExtensionKinds`: unchanged defaults.

### B3. Interactive wizard for bare `gtdx new`
- Bare `gtdx new` (no positional/flags fully specifying the project) prompts
  sequentially using a prompt crate (**new dependency** — `dialoguer`, unless the
  workspace already vendors one). Prompts: kind (select), then name, id (default
  `com.example.<name>`), version, author (default from git), license.
- If `kind == mcp`: additionally prompt "Seed from an OpenAPI spec? [path]"
  (empty → the existing echo skeleton).
- Non-interactive escape hatches (both bypass all prompts): `--yes` (use
  defaults; makes the currently-vestigial flag real) **or** enough explicit flags
  to fully specify the project. Explicit flags always win over prompts.
- Not a TUI; simple sequential prompts.

## Data flow (from-openapi path)

1. `gtdx new --kind mcp --from-openapi ./api.yaml weatherapi --version 0.1.0`
2. Resolve `greentic-mcp-gen` → run `--spec ./api.yaml --output-dir ./weatherapi`.
3. Generator writes `weatherapi_*.component.wasm`, `tools.json`, `component-meta.json`.
4. gtdx reads the two JSON files, authors `./weatherapi/describe.json` with
   network + secrets filled.
5. gtdx writes the anchor `Cargo.toml` + keeps `api.yaml`; prints next-step:
   `gtdx publish --wasm … --manifest … .`

## Error handling
- `greentic-mcp-gen` not resolved → non-zero exit + guided install message (i18n
  if gtdx uses i18n; else a clear stderr message — match the repo's convention).
- Generator exits non-zero → surface its stderr and propagate failure; do not
  swallow. Clean up the partially-created target dir per the existing
  `run_preflight`/`prepare_target` conventions in `new.rs`.
- `--from-openapi` without `--kind mcp` → argument error.
- `component-meta.json` absent (older generator) → degrade: author
  `describe.json` from `tools.json` only (secrets if present), leave
  `permissions.network` empty, print a warning to update `greentic-mcp-generator`.
- No `unwrap()`/`panic!()` on production paths; `anyhow` context.

## Testing
- **SP-A:** unit — `component-meta.json` contains the IR servers + consolidated
  secret_requirements/oauth_scopes; one small-spec snapshot; emitted under
  `--dry-run` too. Gate: `bash ci/local_check.sh`.
- **SP-B:**
  - unit — `describe.json` authoring from `tools.json` + `component-meta.json`
    fixtures: `permissions.network` and `secret_requirements` populated; graceful
    degrade when meta absent (network empty + warning).
  - unit — wizard argument resolution is deterministic under `--yes` / full flags
    (no prompt path exercised in tests).
  - e2e — `gtdx new --kind mcp --from-openapi <fixture-spec>` with a **stub**
    `greentic-mcp-gen` (installed via the env-override the resolver honors) that
    writes canned `*.component.wasm` + `tools.json` + `component-meta.json`;
    assert the project layout + a schema-valid `describe.json` with network +
    secrets. Avoid `fs::hard_link` (cross-fs EXDEV flake); unix-gated stub OK.
  - e2e — guided error when the generator binary is absent.
  - Gate: `bash ci/local_check.sh`.

## Rollout / PRs
- **PR 1 (`greentic-mcp-generator`):** SP-A `component-meta.json`. Merge first so
  the generator in the wild emits it; gtdx degrades if missing, so ordering is
  soft.
- **PR 2 (`greentic-designer-sdk`):** SP-B `--from-openapi` + auto-author +
  wizard. Targets `research` (repo's working branch), no Claude attribution
  trailers (designer-family convention).

## Open questions
- Prompt crate choice (`dialoguer` vs `inquire`) — settle in the plan; prefer
  whatever the wider Greentic tree already uses, else `dialoguer`.
- Exact `component-meta.json` field names — finalize in SP-A's plan; `gtdx`
  consumes them as defined here.
