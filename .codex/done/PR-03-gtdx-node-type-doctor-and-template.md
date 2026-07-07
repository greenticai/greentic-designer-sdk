# PR 03 — Add `gtdx` validation support and template for node-providing design extensions

## Repository

`greenticai/greentic-designer-sdk`

## Objective

Make `gtdx` help extension developers create and validate node-providing design extensions.

This stays generic. Do not add Sorla-specific scaffolding here.

## CLI improvements

Extend:

```bash
gtdx validate <extension-dir>
```

to validate `contributions.nodeTypes` using the typed contract/schema.

`gtdx doctor` is currently a global environment, registry, credentials, and installed-extension diagnostic command. It does not accept an extension directory today. Either:

- add an optional path argument to `doctor` and preserve the existing no-arg global behavior, or
- keep extension-source validation in `gtdx validate` and only add node type checks to `doctor` for installed extensions discovered under `GREENTIC_HOME`.

## New template

Add:

```bash
gtdx new my-node-extension --kind wasm-component --node-type-id my-node --label "My Node"
```

There is no `--template` flag today. The existing node-providing scaffold kind is `wasm-component`, wired through `scaffold::Kind` and `template::load_templates_kind`. Prefer updating that scaffold unless there is a strong reason to add another kind.

The scaffold should include:

- `describe.json` with `kind=DesignExtension`
- non-empty `contributions.nodeTypes`
- one example node type using the same shape validated by PR 01/PR 02
- example schemas if they are part of that node type contract
- tests or fixture output if current templates support that

If the scaffold includes descriptor-level `runtime.gtpack`, it must render a valid `.gtpack` metadata block and staged file before the acceptance test runs `gtdx validate`. The current `wasm-component` template uses `runtime/REPLACE_ME.gtpack`, `REPLACE_AT_BUILD`, and `pack_id`/`component_version` snake_case fields, so validation will fail until those placeholders are replaced or the template learns to produce a valid local fixture.

## Doctor checks

`gtdx doctor` should report:

```text
Node types:
  count: 1
  valid: true
  config schemas valid: true
  output ports valid: true
```

Warnings:

- descriptor runtime.gtpack still contains scaffold placeholders
- node type has no output ports
- node type config schema is missing
- node type config has inline secret-looking values

Only add component-ref warnings such as moving tags or missing digests if PR 01 introduces a real component binding field.

## Tests

Add tests for:

- new template validates
- doctor detects valid nodeTypes
- doctor rejects invalid nodeTypes
- warning for scaffold placeholder gtpack metadata, if `doctor` learns extension-dir mode
- no Sorla-specific assumptions

## Docs

Update:

```text
docs/node-types.md
README.md
```

## Acceptance criteria

```bash
cargo test --workspace --all-features
bash ci/local_check.sh
cargo run -p greentic-extension-sdk-cli -- new my-node-extension --kind wasm-component --node-type-id my-node --label "My Node" --dir /tmp/my-node-extension --yes --no-git
# This validate step requires the scaffold to have replaced runtime.gtpack placeholders with a valid local fixture.
cargo run -p greentic-extension-sdk-cli -- validate /tmp/my-node-extension
```
