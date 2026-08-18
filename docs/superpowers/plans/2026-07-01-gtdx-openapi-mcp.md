# `gtdx` OpenAPI→MCP Generation Implementation Plan (SP-B)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `gtdx new --kind mcp --from-openapi <spec>` shells out to `greentic-mcp-gen`, then auto-authors a complete `describe.json` (network from the spec's servers + `secret_requirements`), producing a ready-to-publish MCP extension; plus an interactive wizard for bare `gtdx new`.

**Architecture:** `gtdx` stays thin — resolve the `greentic-mcp-gen` binary (env override → PATH via the `which` crate), run it into the target dir, read the `component-meta.json` sidecar (SP-A), then render the existing mcp `describe.json.tmpl` and JSON-patch `runtime.permissions.network` + `secret_requirements` from the sidecar. No OpenAPI parsing enters `gtdx`. The wizard uses the already-present `dialoguer` crate, gated on the existing (currently-unused) `--yes` flag.

**Tech Stack:** Rust 1.95, edition 2024; `which` (6) + `dialoguer` (0.11) — both already deps; `serde_json`; `anyhow`. No new crates.

## Global Constraints

- Rust 1.95, edition 2024. No `unwrap()`/`panic!()` on production paths — `anyhow` with context.
- **No i18n system** in gtdx — user-facing strings are plain `anyhow!`/`bail!`/`println!`/`eprintln!` (match existing style, e.g. `new.rs:104`).
- `which` (6) and `dialoguer` (0.11) are ALREADY workspace deps — do NOT add new crates. Env-var reads use the `non_empty_env(name) -> Option<String>` helper pattern (`publish/backend.rs:126`).
- The mcp `describe.json` intentionally does NOT match the `DescribeJson` contract type (`deny_unknown_fields`, `kind: wasix:mcp/router`, `secret_requirements` not `requiredSecrets`). Author it by rendering the existing template + patching `serde_json::Value` — NOT via `greentic_extension_sdk_contract::DescribeJson`.
- `--from-openapi` is valid only with `--kind mcp`.
- Clippy is workspace `-D warnings`; `--locked` (commit `Cargo.lock` if deps change — none expected).
- Tests use raw `std::process::Command` + `tempfile` (NO `assert_cmd`), under `crates/greentic-extension-sdk-cli/tests/cli_new/`, via helpers `gtdx_bin()` + `run()` in `tests/cli_new/fixtures.rs`. Real generator invocation is env-gated (mirror `GTDX_RUN_CARGO_CHECK`), and e2e uses a STUB `greentic-mcp-gen` pointed at by the resolver's env override.
- Gate before done: `bash ci/local_check.sh` (fmt + clippy `-D warnings` + test + build release + publish dry-run).
- Naming: the `--spec` generator path (SP-A) writes `<stem>.component.wasm` + `<stem>.component-meta.json`; derive the meta filename from the chosen wasm stem.

## File Structure

- `crates/greentic-extension-sdk-cli/src/commands/new.rs` — add `--from-openapi` field; branch `run()`; add the wizard. (Tasks 1, 4, 5)
- `crates/greentic-extension-sdk-cli/src/scaffold/openapi.rs` — NEW: resolve+run `greentic-mcp-gen`, author describe.json. (Tasks 2, 3)
- `crates/greentic-extension-sdk-cli/src/scaffold/mod.rs` — expose the new module. (Task 2)
- Tests: `crates/greentic-extension-sdk-cli/tests/cli_new/openapi.rs` (+ register in `tests/cli_new/main.rs`). (Task 6)
- `README.md` — document the flow. (Task 6)

---

### Task 1: `--from-openapi` flag + validation

**Files:**
- Modify: `crates/greentic-extension-sdk-cli/src/commands/new.rs` (`Args`, and a validation helper)
- Test: `crates/greentic-extension-sdk-cli/tests/cli_new/openapi.rs` (create; register `mod openapi;` in `tests/cli_new/main.rs`)

**Interfaces:**
- Produces: `Args.from_openapi: Option<PathBuf>`; `fn validate_from_openapi(kind: Kind, from_openapi: Option<&Path>) -> anyhow::Result<()>`.

- [ ] **Step 1: Write the failing test**

Create `crates/greentic-extension-sdk-cli/tests/cli_new/openapi.rs`:

```rust
use std::process::Command;
use super::fixtures::{gtdx_bin, run};

#[test]
fn from_openapi_requires_kind_mcp() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("demo");
    // --kind design + --from-openapi must be rejected.
    let (ok, _o, e) = run(Command::new(gtdx_bin())
        .arg("new").arg("demo")
        .arg("--kind").arg("design")
        .arg("--from-openapi").arg("api.yaml")
        .arg("--dir").arg(&proj)
        .arg("-y").arg("--no-git").arg("--force"));
    assert!(!ok, "expected failure");
    assert!(e.contains("--from-openapi"), "stderr should explain the flag constraint:\n{e}");
}
```

Register it: in `crates/greentic-extension-sdk-cli/tests/cli_new/main.rs` add `mod openapi;`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-cli --test cli_new from_openapi_requires_kind_mcp`
Expected: FAIL — unknown arg `--from-openapi`.

- [ ] **Step 3: Add the flag + validation**

In `new.rs` `Args` (after the `label` field, ~line 65):

```rust
    /// Seed a `--kind mcp` extension from an OpenAPI/Swagger spec (generates the
    /// router via greentic-mcp-gen instead of the empty echo skeleton).
    #[arg(long, value_name = "SPEC")]
    pub from_openapi: Option<PathBuf>,
```

Add near the other validators (e.g. after `validate_version`):

```rust
fn validate_from_openapi(kind: Kind, from_openapi: Option<&Path>) -> anyhow::Result<()> {
    if from_openapi.is_some() && kind != Kind::Mcp {
        anyhow::bail!("--from-openapi is only valid with --kind mcp");
    }
    Ok(())
}
```

Call it early in `run()` (right after `validate_version(&args.version)?;`):

```rust
    validate_from_openapi(args.kind, args.from_openapi.as_deref())?;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p greentic-extension-sdk-cli --test cli_new from_openapi_requires_kind_mcp`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/greentic-extension-sdk-cli/src/commands/new.rs crates/greentic-extension-sdk-cli/tests/cli_new/openapi.rs crates/greentic-extension-sdk-cli/tests/cli_new/main.rs
git commit -m "feat(new): add --from-openapi flag (mcp-only) with validation"
```

---

### Task 2: Resolve + run `greentic-mcp-gen`

**Files:**
- Create: `crates/greentic-extension-sdk-cli/src/scaffold/openapi.rs`
- Modify: `crates/greentic-extension-sdk-cli/src/scaffold/mod.rs` (add `pub mod openapi;`)
- Test: `crates/greentic-extension-sdk-cli/src/scaffold/openapi.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Produces:
  - `pub const MCP_GEN_BIN_ENV: &str = "GTDX_MCP_GEN_BIN";`
  - `pub fn resolve_mcp_gen() -> anyhow::Result<std::path::PathBuf>`
  - `pub struct GeneratedArtifacts { pub wasm: PathBuf, pub meta: Option<PathBuf> }`
  - `pub fn run_generator(bin: &Path, spec: &Path, out_dir: &Path) -> anyhow::Result<GeneratedArtifacts>`

- [ ] **Step 1: Write the failing test**

Create `crates/greentic-extension-sdk-cli/src/scaffold/openapi.rs` with only a test first (plus `pub mod openapi;` in `mod.rs`):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_mcp_gen_reports_guided_error_when_absent() {
        // Point the override at a non-existent path and ensure a helpful error.
        // SAFETY note: single-threaded test; uses a bogus path so `which` also misses.
        let err = resolve_mcp_gen_with(Some("/nonexistent/greentic-mcp-gen".into()), || None)
            .expect_err("must fail");
        let msg = err.to_string();
        assert!(msg.contains("greentic-mcp-gen"), "guided error: {msg}");
        assert!(msg.contains("cargo binstall"), "should suggest install: {msg}");
    }

    #[test]
    fn resolve_mcp_gen_prefers_env_override_that_exists() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("greentic-mcp-gen");
        std::fs::write(&bin, b"x").unwrap();
        let got = resolve_mcp_gen_with(Some(bin.clone()), || None).unwrap();
        assert_eq!(got, bin);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-cli resolve_mcp_gen`
Expected: FAIL — `resolve_mcp_gen_with` not found.

- [ ] **Step 3: Implement the module**

Write the module body (above the test) in `scaffold/openapi.rs`:

```rust
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Env override pointing at the `greentic-mcp-gen` binary (absolute path).
pub const MCP_GEN_BIN_ENV: &str = "GTDX_MCP_GEN_BIN";

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.is_empty())
}

/// Resolve the generator binary: `GTDX_MCP_GEN_BIN` (if it exists) then PATH.
pub fn resolve_mcp_gen() -> anyhow::Result<PathBuf> {
    resolve_mcp_gen_with(
        non_empty_env(MCP_GEN_BIN_ENV).map(PathBuf::from),
        || which::which("greentic-mcp-gen").ok(),
    )
}

/// Testable core: `override_path` wins if it exists on disk; else `on_path()`.
fn resolve_mcp_gen_with(
    override_path: Option<PathBuf>,
    on_path: impl Fn() -> Option<PathBuf>,
) -> anyhow::Result<PathBuf> {
    if let Some(p) = override_path {
        if p.exists() {
            return Ok(p);
        }
    }
    if let Some(p) = on_path() {
        return Ok(p);
    }
    anyhow::bail!(
        "greentic-mcp-gen was not found. Install it with \
         `cargo binstall greentic-mcp-generator` (set GITHUB_TOKEN for the private repo), \
         or set {MCP_GEN_BIN_ENV} to its path."
    )
}

/// Artifacts the generator emits into `out_dir` (single-spec path).
pub struct GeneratedArtifacts {
    pub wasm: PathBuf,
    pub meta: Option<PathBuf>,
}

/// Run `greentic-mcp-gen --spec <spec> --output-dir <out_dir>` and locate the
/// newest `*.component.wasm` + its paired `*.component-meta.json`.
pub fn run_generator(bin: &Path, spec: &Path, out_dir: &Path) -> anyhow::Result<GeneratedArtifacts> {
    std::fs::create_dir_all(out_dir)?;
    let status = Command::new(bin)
        .arg("--spec")
        .arg(spec)
        .arg("--output-dir")
        .arg(out_dir)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| anyhow::anyhow!("failed to run greentic-mcp-gen: {e}"))?;
    if !status.success() {
        anyhow::bail!("greentic-mcp-gen failed (exit {status})");
    }
    let wasm = newest_matching(out_dir, ".component.wasm")?
        .ok_or_else(|| anyhow::anyhow!("greentic-mcp-gen produced no *.component.wasm in {}", out_dir.display()))?;
    // Sidecar is <stem>.component-meta.json paired with <stem>.component.wasm.
    let meta = wasm
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| out_dir.join(n.replace(".component.wasm", ".component-meta.json")))
        .filter(|p| p.is_file());
    Ok(GeneratedArtifacts { wasm, meta })
}

fn newest_matching(dir: &Path, suffix: &str) -> anyhow::Result<Option<PathBuf>> {
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let is_match = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(suffix));
        if !is_match {
            continue;
        }
        let mtime = entry.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
        if best.as_ref().map(|(t, _)| mtime >= *t).unwrap_or(true) {
            best = Some((mtime, path));
        }
    }
    Ok(best.map(|(_, p)| p))
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p greentic-extension-sdk-cli resolve_mcp_gen`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/greentic-extension-sdk-cli/src/scaffold/openapi.rs crates/greentic-extension-sdk-cli/src/scaffold/mod.rs
git commit -m "feat(scaffold): resolve + run greentic-mcp-gen for OpenAPI generation"
```

---

### Task 3: Auto-author `describe.json` (network + secrets)

**Files:**
- Modify: `crates/greentic-extension-sdk-cli/src/scaffold/openapi.rs`
- Test: same file (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: rendered mcp `describe.json` string (from the existing template + `Context`), and an optional `component-meta.json` path (Task 2).
- Produces: `pub fn author_describe_json(rendered: &str, meta: Option<&Path>) -> anyhow::Result<String>` — returns the patched describe.json text.

- [ ] **Step 1: Write the failing test**

Add to `scaffold/openapi.rs` tests:

```rust
    #[test]
    fn author_describe_fills_network_and_secrets_from_meta() {
        let rendered = r#"{
  "kind": "wasix:mcp/router",
  "runtime": { "permissions": { "network": [], "secrets": [] } },
  "secret_requirements": []
}"#;
        let dir = tempfile::tempdir().unwrap();
        let meta = dir.path().join("m.json");
        std::fs::write(&meta, r#"{
  "servers": ["https://api.example.com"],
  "secret_requirements": [{"key":"EXAMPLE_KEY","required":true}],
  "oauth_scopes": []
}"#).unwrap();

        let out = author_describe_json(rendered, Some(&meta)).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["runtime"]["permissions"]["network"], serde_json::json!(["https://api.example.com"]));
        assert_eq!(v["secret_requirements"][0]["key"], "EXAMPLE_KEY");
    }

    #[test]
    fn author_describe_degrades_without_meta() {
        let rendered = r#"{"runtime":{"permissions":{"network":[]}},"secret_requirements":[]}"#;
        let out = author_describe_json(rendered, None).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["runtime"]["permissions"]["network"], serde_json::json!([]));
        assert_eq!(v["secret_requirements"], serde_json::json!([]));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-cli author_describe`
Expected: FAIL — `author_describe_json` not found.

- [ ] **Step 3: Implement the author**

Add to `scaffold/openapi.rs`:

```rust
/// Patch a rendered mcp `describe.json` with network hosts + secret requirements
/// taken from the generator's `component-meta.json`. Degrades to the rendered
/// values (empty) with a warning when `meta` is absent.
pub fn author_describe_json(rendered: &str, meta: Option<&Path>) -> anyhow::Result<String> {
    let mut doc: serde_json::Value =
        serde_json::from_str(rendered).map_err(|e| anyhow::anyhow!("rendered describe.json is not valid JSON: {e}"))?;

    let Some(meta_path) = meta else {
        eprintln!(
            "  ! component-meta.json not found — permissions.network and secret_requirements left empty. \
             Update greentic-mcp-generator to auto-fill them, or edit describe.json."
        );
        return serde_json::to_string_pretty(&doc).map_err(Into::into);
    };

    let meta: serde_json::Value = serde_json::from_slice(&std::fs::read(meta_path)?)
        .map_err(|e| anyhow::anyhow!("component-meta.json is not valid JSON: {e}"))?;

    // network <= servers (verbatim origins the component calls)
    if let Some(servers) = meta.get("servers").cloned() {
        doc["runtime"]["permissions"]["network"] = servers;
    }
    // secret_requirements <= meta.secret_requirements (same greentic-types shape)
    if let Some(secrets) = meta.get("secret_requirements").cloned() {
        doc["secret_requirements"] = secrets;
    }
    serde_json::to_string_pretty(&doc).map_err(Into::into)
}
```

> Design note: `permissions.network` is populated with the OpenAPI `servers` verbatim (origin URLs). If a future describe-schema for `wasix:mcp/router` requires bare hosts, normalize here — but do not add a URL-parsing crate for MVP.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p greentic-extension-sdk-cli author_describe`
Expected: PASS (both).

- [ ] **Step 5: Commit**

```bash
git add crates/greentic-extension-sdk-cli/src/scaffold/openapi.rs
git commit -m "feat(scaffold): author describe.json network+secrets from component-meta.json"
```

---

### Task 4: Wire the `--from-openapi` path into `run()`

**Files:**
- Modify: `crates/greentic-extension-sdk-cli/src/commands/new.rs`
- Test: `crates/greentic-extension-sdk-cli/tests/cli_new/openapi.rs` (e2e with a stub generator)

**Interfaces:**
- Consumes: `scaffold::openapi::{resolve_mcp_gen, run_generator, author_describe_json}` (Tasks 2/3); the existing `build_context`, `template::load_templates_kind`, `template::write_file`, `render_templates`.
- Produces: end-to-end behavior for `gtdx new --kind mcp --from-openapi <spec>`.

- [ ] **Step 1: Write the failing e2e test**

Add to `tests/cli_new/openapi.rs` (stub generator via the env override; write it unix-gated to keep the shell stub simple):

```rust
#[cfg(unix)]
#[test]
fn from_openapi_generates_and_authors_describe() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    // Stub greentic-mcp-gen: writes <stem>.component.wasm + <stem>.component-meta.json into --output-dir.
    let stub = tmp.path().join("greentic-mcp-gen");
    std::fs::write(&stub, r#"#!/bin/sh
# args: --spec <spec> --output-dir <dir>
OUT=""
while [ $# -gt 0 ]; do case "$1" in --output-dir) OUT="$2"; shift 2;; *) shift;; esac; done
printf '(module)' > "$OUT/petstore.component.wasm"
cat > "$OUT/petstore.component-meta.json" <<JSON
{"servers":["https://petstore.example.com"],"secret_requirements":[{"key":"PETSTORE_KEY","required":true}],"oauth_scopes":[]}
JSON
"#).unwrap();
    let mut perms = std::fs::metadata(&stub).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&stub, perms).unwrap();

    let spec = tmp.path().join("petstore.yaml");
    std::fs::write(&spec, "openapi: 3.0.0\n").unwrap();
    let proj = tmp.path().join("petstore-ext");

    let (ok, _o, e) = run(Command::new(gtdx_bin())
        .env("GTDX_MCP_GEN_BIN", &stub)
        .arg("new").arg("petstore-ext")
        .arg("--kind").arg("mcp")
        .arg("--from-openapi").arg(&spec)
        .arg("--dir").arg(&proj)
        .arg("-y").arg("--no-git").arg("--force"));
    assert!(ok, "stderr:\n{e}");

    // generated wasm present
    assert!(proj.join("petstore.component.wasm").exists());
    // describe.json authored with network + secrets from the meta sidecar
    let describe: serde_json::Value =
        serde_json::from_slice(&std::fs::read(proj.join("describe.json")).unwrap()).unwrap();
    assert_eq!(describe["kind"], "wasix:mcp/router");
    assert_eq!(describe["runtime"]["permissions"]["network"], serde_json::json!(["https://petstore.example.com"]));
    assert_eq!(describe["secret_requirements"][0]["key"], "PETSTORE_KEY");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p greentic-extension-sdk-cli --test cli_new from_openapi_generates_and_authors_describe`
Expected: FAIL — from-openapi path not wired (falls back to echo skeleton or errors).

- [ ] **Step 3: Branch `run()` for the from-openapi path**

In `new.rs` `run()`, replace the scaffold/render section (the `render_templates` + `write_wit_and_lock` block) with a branch. Keep the existing echo path when `from_openapi` is `None`:

```rust
    let ctx = build_context(args, &id, &author);

    let files_written = if let Some(spec) = args.from_openapi.as_deref() {
        scaffold_from_openapi(&ctx, spec, &target)?
    } else {
        let mut n = render_templates(&ctx, args.kind.as_str(), &target)?;
        n += write_wit_and_lock(args.kind.as_str(), &target)?;
        n
    };

    make_scripts_executable(&target)?;
    run_git_init(&target, args.no_git);
    print_summary(args.kind.as_str(), &target, files_written);
    Ok(())
```

Add the orchestrator (in `new.rs`, using the Task 2/3 module):

```rust
fn scaffold_from_openapi(ctx: &crate::scaffold::template::Context, spec: &Path, target: &Path) -> anyhow::Result<usize> {
    use crate::scaffold::openapi;

    let bin = openapi::resolve_mcp_gen()?;
    let artifacts = openapi::run_generator(&bin, spec, target)?;

    // Render the mcp describe.json template, then patch network + secrets.
    let mut files = 1usize; // the generated wasm
    let describe_tmpl = crate::scaffold::template::load_templates_kind("mcp")
        .into_iter()
        .find(|e| e.dst_rel.ends_with("describe.json"))
        .ok_or_else(|| anyhow::anyhow!("mcp describe.json template missing"))?;
    let rendered = ctx.render(std::str::from_utf8(describe_tmpl.src_bytes)?)?;
    let authored = openapi::author_describe_json(&rendered, artifacts.meta.as_deref())?;
    crate::scaffold::template::write_file(&target.join("describe.json"), authored.as_bytes())?;
    files += 1;

    // Minimal Cargo.toml anchor so `gtdx publish --manifest ./Cargo.toml` works.
    let cargo_anchor = format!(
        "# Anchor manifest for `gtdx publish --wasm`. The component is the\n\
         # pre-built wasm generated from the OpenAPI spec; there is no crate to build here.\n\
         [package]\nname = \"{}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n[lib]\npath = \"/dev/null\"\n",
        target.file_name().and_then(|n| n.to_str()).unwrap_or("mcp-ext")
    );
    crate::scaffold::template::write_file(&target.join("Cargo.toml"), cargo_anchor.as_bytes())?;
    files += 1;

    println!(
        "  Next: gtdx publish --wasm {} --manifest {} .",
        artifacts.wasm.display(),
        target.join("Cargo.toml").display()
    );
    Ok(files)
}
```

> Confirm `Context`, `load_templates_kind`, `write_file`, and `TemplateEntry.{src_bytes,dst_rel}` visibility from `new.rs` — `render_templates` already uses them, so they are reachable (make any needed items `pub(crate)` in `scaffold/template.rs` if the compiler complains; `render_templates` is the reference for what's already accessible). The `dst_rel.ends_with("describe.json")` match assumes the template's dest is `describe.json` (confirmed by the scaffold). The anchor `Cargo.toml` uses `path = "/dev/null"` only to satisfy `--manifest`; if `gtdx publish` rejects that, fall back to writing a `describe.json`-only project and pass `--manifest <dir>` (verify against `publish`'s `--manifest` handling and adjust the anchor minimally).

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p greentic-extension-sdk-cli --test cli_new from_openapi_generates_and_authors_describe`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/greentic-extension-sdk-cli/src/commands/new.rs crates/greentic-extension-sdk-cli/tests/cli_new/openapi.rs
git commit -m "feat(new): wire --from-openapi to generate + author a publish-ready mcp extension"
```

---

### Task 5: Interactive wizard for bare `gtdx new`

**Files:**
- Modify: `crates/greentic-extension-sdk-cli/src/commands/new.rs`
- Test: `crates/greentic-extension-sdk-cli/tests/cli_new/openapi.rs`

**Interfaces:**
- Consumes: `dialoguer` (already a dep); `Args`.
- Produces: `fn wizard_fill(args: &mut Args) -> anyhow::Result<()>` invoked in `run()` when interactive.

- [ ] **Step 1: Write the failing test**

The interactive path can't be driven from a non-TTY test; assert the NON-interactive gate instead — `--yes` (and full flags) must skip prompts and still succeed, so CI/e2e never blocks on a prompt. Add to `tests/cli_new/openapi.rs`:

```rust
#[test]
fn yes_flag_skips_wizard_and_scaffolds_noninteractively() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("q");
    // No stdin attached; -y must prevent any prompt and scaffold the echo mcp skeleton.
    let (ok, _o, e) = run(Command::new(gtdx_bin())
        .arg("new").arg("q")
        .arg("--kind").arg("mcp")
        .arg("--dir").arg(&proj)
        .arg("-y").arg("--no-git").arg("--force"));
    assert!(ok, "stderr:\n{e}");
    assert!(proj.join("describe.json").exists());
}
```

- [ ] **Step 2: Run test to verify current behavior**

Run: `cargo test -p greentic-extension-sdk-cli --test cli_new yes_flag_skips_wizard`
Expected: PASS today (run() is linear). This test is a REGRESSION GUARD — it must stay green after adding the wizard (i.e. `-y` must bypass prompts).

- [ ] **Step 3: Add the wizard, gated so `--yes`/non-TTY never prompts**

In `new.rs`, add (uses `std::io::IsTerminal`):

```rust
fn wizard_fill(args: &mut Args) -> anyhow::Result<()> {
    use dialoguer::{Confirm, Input};
    use std::io::IsTerminal;

    // Never prompt when the user opted out or stdin is not a TTY (CI, pipes).
    if args.yes || !std::io::stdin().is_terminal() {
        return Ok(());
    }

    // Only prompt for values the user did not already pass explicitly.
    if args.from_openapi.is_none() && args.kind == Kind::Mcp {
        let seed = Confirm::new()
            .with_prompt("Seed this MCP extension from an OpenAPI spec?")
            .default(false)
            .interact()?;
        if seed {
            let path: String = Input::new().with_prompt("OpenAPI spec path").interact_text()?;
            args.from_openapi = Some(PathBuf::from(path));
        }
    }
    Ok(())
}
```

Invoke it at the very top of `run()`. Because `run(args: &Args, ...)` takes `&Args`, change the signature to `run(args: Args, _home: &Path)` (or clone into a local `let mut args = args.clone();`) and call `wizard_fill(&mut args)?` first. Prefer taking `Args` by value if the caller allows; otherwise clone. Match the dispatcher's call site (update it if the signature changes).

> Keep the wizard minimal per YAGNI: only the mcp OpenAPI-seed prompt for MVP. The other fields already have sensible defaults (`id`, `author`, `version`), so do NOT add prompts for them unless a later task asks. Explicit flags always win (the `is_none()`/kind guards ensure that).

- [ ] **Step 4: Run the regression test**

Run: `cargo test -p greentic-extension-sdk-cli --test cli_new yes_flag_skips_wizard`
Expected: PASS (wizard bypassed under `-y`).

- [ ] **Step 5: Commit**

```bash
git add crates/greentic-extension-sdk-cli/src/commands/new.rs
git commit -m "feat(new): interactive wizard for bare gtdx new (mcp OpenAPI seed)"
```

---

### Task 6: Docs + final gate

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Document the flow in the README**

Add a section to `README.md` near the `gtdx new` / mcp docs:

````markdown
### Generate an MCP extension from OpenAPI

```bash
gtdx new --kind mcp --from-openapi ./api.yaml weatherapi
```

`gtdx` shells out to `greentic-mcp-gen` (from `greentic-mcp-generator`) to generate
the `wasix:mcp/router` component, then auto-authors `describe.json` — including
`runtime.permissions.network` (from the spec's servers) and `secret_requirements`.
Install the generator once with `cargo binstall greentic-mcp-generator` (set
`GITHUB_TOKEN` for the private repo) or point `GTDX_MCP_GEN_BIN` at the binary.

The result is publish-ready:

```bash
gtdx publish --wasm ./weatherapi/weatherapi.component.wasm --manifest ./weatherapi/Cargo.toml ./weatherapi
```

Running `gtdx new` with no flags starts an interactive wizard; for `--kind mcp`
it offers to seed from an OpenAPI spec. Pass `-y` for non-interactive defaults.
````

- [ ] **Step 2: Run the full local CI gate**

Run: `bash ci/local_check.sh`
Expected: fmt + clippy(`-D warnings`) + tests + release build + publish dry-run all green. Fix any in-scope fmt/clippy issues (`cargo fmt --all`; resolve lints) and re-run until green. If a failure is clearly outside this feature's scope, document it in the PR summary rather than hiding it.

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: document gtdx new --from-openapi MCP generation"
```

---

## Self-Review

**Spec coverage (SP-B):**
- `gtdx new --kind mcp --from-openapi <spec>` produces wasm + describe.json → Tasks 1, 2, 4. ✅
- Auto-author describe.json (network from servers + secret_requirements) → Task 3; wired in Task 4. ✅
- Interactive wizard for bare `gtdx new` → Task 5. ✅
- Thin (shell out, no OpenAPI parsing; reuse `which`/`dialoguer`) → Tasks 2, 5, Global Constraints. ✅
- Guided error when generator absent → Task 2. ✅
- Degrade when `component-meta.json` absent → Task 3. ✅
- `--from-openapi` only with `--kind mcp` → Task 1. ✅
- Docs + gate → Task 6. ✅

**Placeholder scan:** No TBD/TODO. The `>` notes are verification guards with concrete fallbacks (Context/template visibility; Cargo anchor `--manifest` acceptance; network-shape normalization) — each names a default action, not deferred work.

**Type consistency:** `resolve_mcp_gen`/`resolve_mcp_gen_with`, `run_generator -> GeneratedArtifacts{wasm, meta}`, `author_describe_json(&str, Option<&Path>) -> Result<String>`, `scaffold_from_openapi(&Context, &Path, &Path) -> Result<usize>`, `MCP_GEN_BIN_ENV = "GTDX_MCP_GEN_BIN"` — used identically across Tasks 2/3/4 and their tests. The stub e2e in Task 4 emits exactly the `<stem>.component.wasm` + `<stem>.component-meta.json` names that Task 2's `run_generator` looks for, and the `component-meta.json` shape (`servers`/`secret_requirements`/`oauth_scopes`) matches SP-A's `ComponentMeta`.
