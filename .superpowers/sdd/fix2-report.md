# Fix 2: Hermetic greentic-mcp-gen Invocation

## Summary

`gtdx new --kind mcp --from-openapi <spec>` invoked `greentic-mcp-gen` without
isolation. The real generator creates bookkeeping dirs (`input/done/error/uploaded`)
in its working directory and **moves** the source spec file into `done/`. This
meant:

- Four unexpected directories appeared in the user's cwd.
- The user's original OpenAPI spec file was silently destroyed.

## Hermetic Approach

`run_generator` in `src/scaffold/openapi.rs` now:

1. Creates a private temp directory (`scratch`) via `tempfile::tempdir()`. The
   `TempDir` guard is held for the duration of the function and dropped at
   return, triggering automatic cleanup.
2. Pre-creates `scratch/input/` so the generator does not fail looking for it.
3. **Copies** the user's spec file into `scratch/<spec-filename>` and passes
   that copy path as `--spec`. The generator's `mv` now moves our throwaway
   copy, leaving the original untouched.
4. Passes explicit absolute dir flags so all generator bookkeeping stays inside
   scratch: `--input-dir`, `--done-dir`, `--error-dir`, `--uploaded-dir`.
5. Sets `.current_dir(scratch_path)` on the child process as belt-and-suspenders.
6. Passes `--output-dir <out_dir>` unchanged — artifacts still land in the
   caller's chosen project directory.
7. All existing behaviour preserved: non-zero exit → error; artifact location via
   `newest_matching(out_dir, ".component.wasm")` + sidecar pairing.

## tempfile in [dependencies]

`tempfile` was only in `[dev-dependencies]`. It is now also in `[dependencies]`
(workspace version `"3"`) because the production `run_generator` function needs
it at runtime.

## Strengthened Test Stub + Assertions

The `from_openapi_generates_and_authors_describe` integration test now:

### Stub behaviour (mimics the real generator)
- Parses both `--spec` and `--output-dir` from its argument list.
- Creates `input/done/error/uploaded` dirs in its **own cwd** (i.e. wherever
  gtdx sets the child's working directory).
- `mv`s the `--spec` file into `done/`.
- Writes `petstore.component.wasm` + `petstore.component-meta.json` to
  `--output-dir`.

### Test setup
- The user's spec is placed in a separate `user_cwd` tempdir to isolate it from
  stub artefacts.
- The test runs `gtdx` with `.current_dir(user_cwd.path())` to simulate the
  user being in their own directory.

### Hermetic assertions (new)
- The original spec file still exists at its original path after the command.
- No `input/done/error/uploaded` dirs in `user_cwd`.
- No `input/done/error/uploaded` dirs in the generated project dir.
- Existing assertions retained: wasm present, `describe.json` authored with
  network + secrets from meta sidecar.

## RED / GREEN Evidence

**RED** (before fix, test stub already mimics real generator side-effects):
```
test openapi::from_openapi_generates_and_authors_describe ... FAILED
thread panicked at tests/cli_new/openapi.rs:142:5:
original spec file was destroyed — generator polluted the user's workspace
```

**GREEN** (after fix):
```
test openapi::from_openapi_requires_kind_mcp ... ok
test openapi::yes_flag_skips_wizard_and_scaffolds_noninteractively ... ok
test openapi::from_openapi_generates_and_authors_describe ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 14 filtered out
```

## Full ci/local_check.sh Outcome

```
==> cargo fmt          PASS
==> cargo clippy       PASS  (1 doc-markdown lint in openapi.rs fixed: OpenAPI -> `OpenAPI`)
==> cargo test         PASS  (16 passed; 1 ignored [wasm32-wasip2]; 0 failed)
==> cargo publish --dry-run (per-crate)  PASS
All checks passed.
```

## Files Changed

- `crates/greentic-extension-sdk-cli/Cargo.toml` — added `tempfile` to
  `[dependencies]`
- `crates/greentic-extension-sdk-cli/src/scaffold/openapi.rs` — hermetic
  `run_generator` implementation
- `crates/greentic-extension-sdk-cli/tests/cli_new/openapi.rs` — strengthened
  stub + hermetic assertions
- `Cargo.lock` — updated by cargo (tempfile promoted from dev-only)
- `.superpowers/sdd/fix2-report.md` — this file

## Concerns

None blocking. One note: the stub parses `--input-dir/--done-dir/--error-dir/--uploaded-dir`
with a wildcard `*) shift;;` branch, so if the real generator's CLI does not
accept those flags it will silently ignore them and still create dirs in its cwd.
In that case only `--spec` copy + `.current_dir(scratch)` matter (which are
already in place). The explicit dir flags are opportunistic hardening.
