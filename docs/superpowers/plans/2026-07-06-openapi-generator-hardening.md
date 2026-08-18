# `gtdx openapi` Generator Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Three robustness fixes to the `gtdx openapi` parser (EPIC-F v1 follow-ups flagged by the whole-branch review) so the generator behaves correctly on more real-world specs: (1) duplicate-`operationId` dedup+warn, (2) per-operation security picks the first *supported* scheme (not blindly the first), (3) a non-scalar query parameter warns instead of silently serializing oddly.

**Architecture:** All three are in the pure parser `crates/greentic-extension-sdk-cli/src/commands/openapi/model.rs` (`parse_openapi` + `build_security`). No codegen or command changes; additive robustness with warnings on the skip/degrade paths.

**Tech Stack:** Rust (edition 2024), `openapiv3`, `serde_json`.

## Global Constraints
- **Crate:** `greentic-extension-sdk-cli` only; file `src/commands/openapi/model.rs`. No change to `codegen.rs`/templates/command.
- **Behavior on degrade:** warn (`eprintln!`, matching the existing skip-warn style at model.rs:159) + skip/degrade gracefully — never panic, never silently mis-generate.
- **Conventional commits, NO Claude co-author.** Target `research`.
- **Build discipline (shared machine, disk ample ~103GB):** cargo `-j2` + `CARGO_BUILD_JOBS=2`, FOREGROUND; never pkill/kill or delete another worktree's target/.

---

### Task 1: three parser hardening fixes + tests

**Files:**
- Modify: `crates/greentic-extension-sdk-cli/src/commands/openapi/model.rs`
- Test: inline `#[cfg(test)]` (+ reuse/extend `fixtures/petstore-min.json` or add small inline specs)

**Interfaces:**
- Consumes/produces: same `parse_openapi(spec_bytes, name_override, base_url_override) -> anyhow::Result<ConnectorModel>` + `build_security(...)` — behavior hardened, signatures unchanged.

- [ ] **Step 1: Read** `parse_openapi` (the operation loop ~:155-260, where `operation_id`/`ToolModel` are built) + `build_security` (~:344, "first entry in `components.securitySchemes`") + `AuthScheme` (Bearer/ApiKey) + the existing warn style (:159).

- [ ] **Step 2: Write failing tests.**
```rust
#[test]
fn duplicate_operation_id_is_skipped_with_warning() {
    // spec with two operations sharing operationId "getThing"
    let model = parse_openapi(DUP_ID_SPEC, None, None).unwrap();
    // only the FIRST occurrence is kept; the duplicate is dropped (not two tools with the same name)
    assert_eq!(model.tools.iter().filter(|t| t.name == "getThing").count(), 1);
}
#[test]
fn security_picks_first_supported_scheme_skipping_oauth2() {
    // components.securitySchemes: [oauth2 (unsupported), bearerAuth (supported)]
    let model = parse_openapi(OAUTH2_THEN_BEARER_SPEC, None, None).unwrap();
    assert!(matches!(model.security, Some(AuthScheme::Bearer { .. }))); // not None
}
#[test]
fn non_scalar_query_param_warns_but_still_parses() {
    // a query param whose schema is type:array — kept? or skipped? assert it does NOT panic
    // and the operation still yields a tool (degrade, don't crash)
    let model = parse_openapi(ARRAY_QUERY_SPEC, None, None).unwrap();
    assert_eq!(model.tools.len(), 1);
}
```

- [ ] **Step 3: Run — expect FAIL** (`CARGO_BUILD_JOBS=2 cargo test -p greentic-extension-sdk-cli -j2 openapi::model`).

- [ ] **Step 4: Implement.**
  - **Dedup:** track a `HashSet<String>` of seen `operation_id`s in the loop; on a duplicate, `eprintln!("warning: skipping duplicate operationId '{operation_id}' ({method} {path})")` + `continue` (keep the first).
  - **Security first-supported:** in `build_security`, iterate `components.securitySchemes` in order and return the FIRST one that maps to a supported `AuthScheme` (http bearer / apiKey-in-header); skip unsupported (oauth2, openIdConnect, apiKey-in-cookie/query) instead of returning `None` when the first happens to be unsupported. Warn once if some were skipped.
  - **Non-scalar query warn:** where a query `Param` schema is built, if the param schema `type` is `array`/`object` (non-scalar), `eprintln!("warning: query parameter '{name}' on '{operation_id}' is non-scalar; it will be serialized as-is")` — keep the param (degrade), do not skip the whole operation.

- [ ] **Step 5: PASS + gate + commit.** `cargo fmt --all`; `CARGO_BUILD_JOBS=2 cargo clippy -p greentic-extension-sdk-cli -j2 --all-targets -- -D warnings`; `CARGO_BUILD_JOBS=2 cargo test -p greentic-extension-sdk-cli -j2`. Commit (`fix(gtdx): dedup operationId + first-supported security scheme + non-scalar query warn`). Then finishing-a-development-branch → PR to `research`.

## Self-Review
- **Coverage:** the 3 review follow-ups → the 3 sub-fixes in Task 1, each tested.
- **Placeholder scan:** the `*_SPEC` fixtures are named but must be written as real inline OpenAPI 3.0 JSON in Step 2 (small — 1-2 ops each). No TBD.
- **Scope:** one file, one crate, additive robustness; existing 4/4 model tests must still pass.
