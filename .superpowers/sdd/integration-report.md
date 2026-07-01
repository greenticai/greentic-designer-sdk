# SP-B OpenAPI→MCP Merge Integration Report

## What conflicted

Single file with a git conflict marker:
- `crates/greentic-extension-sdk-cli/src/commands/new/mod.rs` — our `wizard_fill` fn + the old `run(mut args: Args, ...)` signature vs research's `Resolved` struct + new `run(args: &Args, ...)` + `resolve()`/`resolve_from_flags()` dispatch. Auto-merge succeeded on the `Args` struct (our `from_openapi` field was already in HEAD, so git picked research's struct block which already had our field added by the 3-way merge).

Files that auto-merged cleanly: `README.md`, `src/main.rs`, `src/scaffold/template.rs`, `src/commands/login.rs`, `src/commands/publish.rs`, `src/publish/backend.rs`, and the new template files.

## Resolution per file

### `src/commands/new/mod.rs`
- Dropped our `wizard_fill` function entirely (research's `wizard::run` supersedes it).
- Adopted research's `Resolved` struct as the base, adding `from_openapi: Option<PathBuf>` field.
- Adopted research's `run(args: &Args, _home: &Path)` → `resolve(args)` → scaffold dispatch structure.
- In `run()`: after `resolve()`, added `validate_from_openapi(resolved.kind, resolved.from_openapi.as_deref())?` before preflight.
- In `run()`: the scaffold branch is now `if let Some(spec) = resolved.from_openapi.as_deref() { scaffold_from_openapi(...) } else { render_templates + write_wit_and_lock }`.
- In `resolve_from_flags()`: threaded `from_openapi: args.from_openapi.clone()` into the returned `Resolved`.
- Kept `scaffold_from_openapi`, `validate_from_openapi`, and all helper fns from our branch unchanged.
- Made `is_reverse_dns` `pub(super)` so `wizard.rs` can call it (matching research's import pattern).

### `src/commands/new/wizard.rs`
- Added `use std::path::PathBuf;`.
- Added `prompt_openapi_seed(args: &Args, kind: Kind) -> anyhow::Result<Option<PathBuf>>`: if `kind == Kind::Mcp` and `args.from_openapi.is_none()`, prompts `Confirm` "Seed this MCP extension from an OpenAPI spec?" then `Input` "OpenAPI spec path". Otherwise passes `args.from_openapi.clone()` through unchanged.
- Called `prompt_openapi_seed` after `prompt_kind` in `run()`, before `prompt_id`.
- Extended the `Resolved { ... }` construction to include `from_openapi`.
- All research wizard tests preserved unchanged.

### `src/main.rs`
- Auto-merged brought in research's `.await` on `run_login`. Our call `commands::new::run(args, &home)` was missing the `&`, fixed to `commands::new::run(&args, &home)` to match the new `&Args` signature.

### `src/scaffold/openapi.rs`
- No conflict; net-new on our branch. Kept as-is.

### `src/scaffold/mod.rs`
- Already had `pub mod openapi;` from our branch. Kept.

### Tests (`tests/cli_new/openapi.rs`)
- No changes needed. All three tests use named flags (`--kind mcp`, `-y`, `--no-git`, `--force`, `--from-openapi`) which are unchanged in research's `Args`. The `name` argument is still a positional `Option<String>` so passing `"petstore-ext"` as a positional arg still works. Tests pass as-is.

## How `from_openapi` threads through

```
CLI flag --from-openapi <SPEC>
  → Args.from_openapi: Option<PathBuf>
  → resolve(args)
      → if wizard: wizard::run(args)
            → prompt_openapi_seed(args, kind) → Resolved.from_openapi
        else: resolve_from_flags(args)
            → Resolved.from_openapi = args.from_openapi.clone()
  → validate_from_openapi(resolved.kind, resolved.from_openapi.as_deref())
        → bail if Some && kind != Mcp
  → if Some(spec) = resolved.from_openapi:
        scaffold_from_openapi(&ctx, spec, &target)
    else:
        render_templates + write_wit_and_lock
```

## `wizard_fill` dropped

Yes, our `wizard_fill` function is gone. Research's `wizard::run` handles the same logic (and more) via `prompt_openapi_seed`. No functionality was lost.

## Test adaptations

None required. The openapi tests already used the flag-driven path (`-y`, named positional `name` arg) which maps cleanly to the new `Args.name: Option<String>` (positional arg, still works with a bare string). All three tests assert real behavior:
1. `from_openapi_requires_kind_mcp` — rejects `--from-openapi` without `--kind mcp`.
2. `from_openapi_generates_and_authors_describe` — e2e stub generator, asserts `describe.json` network+secrets filled.
3. `yes_flag_skips_wizard_and_scaffolds_noninteractively` — asserts `-y` path works.

## ci/local_check.sh outcome

| Stage | Result |
|-------|--------|
| cargo fmt | PASS |
| cargo clippy | FAIL (2 `doc_markdown` warnings on `OpenAPI` without backticks in our new comments) → fixed in commit `16de66e` |
| cargo test | PASS (all tests ok) |
| cargo build --release | PASS |
| cargo publish --dry-run | PASS |

Final re-run: **All checks passed**.

## Commits

- `517f71b` — merge commit (origin/research → impl/gtdx-openapi-mcp)
- `16de66e` — fix: backtick `OpenAPI` in doc comments to satisfy clippy::doc_markdown

## Files changed

- `crates/greentic-extension-sdk-cli/src/commands/new/mod.rs` — conflict resolved, `Resolved.from_openapi` added, `scaffold_from_openapi` wired
- `crates/greentic-extension-sdk-cli/src/commands/new/wizard.rs` — `prompt_openapi_seed` added, `Resolved` extended
- `crates/greentic-extension-sdk-cli/src/main.rs` — `&args` reference fix
- `crates/greentic-extension-sdk-cli/src/scaffold/openapi.rs` — kept from our branch (no conflict)
- Plus all research auto-merged files (templates, login, publish, etc.)

## Concerns

None blocking. The `validate_from_openapi` check happens after `resolve()` (which includes the wizard), so a user who picks MCP in the wizard and enters an OpenAPI path that was incorrectly combined with a non-MCP kind would be caught at validation — but since `prompt_openapi_seed` only fires for `Kind::Mcp`, this path is unreachable in practice. The double-check is intentional defense-in-depth for the non-interactive (`-y`) path.

---

## Fix: double next-steps

### Problem

`scaffold_from_openapi` (line 281-285) printed its own authoritative next-step:
```
  Next: gtdx publish --wasm <wasm> --manifest ./Cargo.toml .
```
Then `run()` unconditionally called `print_summary(...)`, which printed a second block:
```
Next steps:
  cd <target>
  gtdx dev        # watch, rebuild, reinstall
  gtdx publish    # pack to dist/
```
For the OpenAPI path this is misleading (`gtdx dev` does not apply — the Cargo.toml has `[lib] path = "/dev/null"`) and conflicts with the first message.

### Solution

In `run()` (line 127), wrapped `print_summary` in a guard:

```rust
if resolved.from_openapi.is_none() {
    print_summary(resolved.kind.as_str(), &target, files_written);
}
```

The OpenAPI path now emits only the one line from `scaffold_from_openapi`. All other scaffold kinds still call `print_summary` unchanged.

### Test + Clippy

| Check | Result |
|-------|--------|
| `cargo test -p greentic-extension-sdk-cli --test cli_new` | 16 passed, 0 failed, 1 ignored |
| `cargo clippy -p greentic-extension-sdk-cli --all-targets -- -D warnings` | clean (no warnings) |

### Commit

`10ea4b7` — `fix(new): single next-steps message on the --from-openapi path`
