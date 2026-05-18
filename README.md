# greentic-designer-sdk

Public SDK for authoring [Greentic Designer](https://greentic.ai) extensions — Bundle, Design, Deploy, Provider, WASM-component, and LLM extension kinds.

## What this is

A workspace containing the public-facing tooling and types for building Greentic Designer extensions:

| Crate | Description |
|---|---|
| `greentic-extension-sdk-contract` | Type definitions, `describe.json` schema, signing/verification primitives |
| `greentic-extension-sdk-state` | Persistent enable/disable state for installed extensions |
| `greentic-extension-sdk-registry` | Registry client (HTTP + OCI) and install lifecycle |
| `greentic-extension-sdk-testing` | Test utilities: fixtures, gtxpack helpers |
| `greentic-extension-sdk-cli` | The `gtdx` command-line tool: scaffold, build, validate, sign, publish |

The runtime engine that *executes* WASM extensions is part of the commercial Greentic Designer platform and is not included here. This SDK gives developers everything they need to author, validate, sign, and publish extensions; execution happens on the Greentic platform.

## Quick start

### Install the CLI

Prebuilt binaries are published to GitHub Releases for every tagged
version (Linux / macOS / Windows, x86_64 + aarch64). Pick whichever
matches your setup:

**Recommended — `cargo binstall` (no compile, fetches the release binary):**

```bash
cargo binstall greentic-extension-sdk-cli \
  --version 1.2.2-research \
  --git https://github.com/greenticai/greentic-designer-sdk
```

`--git` is required because this crate is not published to crates.io —
binstall reads the `[package.metadata.binstall]` section directly from
the repo at the requested tag. Drop the `--version` flag once a stable
release is cut.

**Build from source (slowest, needs the full toolchain):**

```bash
cargo install --git https://github.com/greenticai/greentic-designer-sdk \
  --tag v1.2.2-research \
  greentic-extension-sdk-cli
```

**Manual download:**

```bash
# macOS Apple Silicon example — swap target for your platform
curl -L -o gtdx.tgz \
  https://github.com/greenticai/greentic-designer-sdk/releases/download/v1.2.2-research/gtdx-v1.2.2-research-aarch64-apple-darwin.tgz
tar -xzf gtdx.tgz
chmod +x gtdx && mv gtdx ~/.cargo/bin/
```

Available targets: `aarch64-apple-darwin`, `x86_64-apple-darwin`,
`aarch64-unknown-linux-gnu`, `x86_64-unknown-linux-gnu`,
`aarch64-pc-windows-msvc`, `x86_64-pc-windows-msvc` (Windows uses
`.zip` instead of `.tgz`).

Verify the install:

```bash
gtdx --version
```

### Scaffold and build an extension

```bash
gtdx new my-ext --kind design     # or: bundle | deploy | provider | wasm-component | llm
cd my-ext
gtdx dev --once
```

This rebuilds, packs, and produces `dist/<name>-<version>.gtxpack`. The
pack includes a `manifest.json` integrity ledger (sha256 of every entry)
since 1.2.0-research; runtime install verifies it.

### Lint before publish

```bash
gtdx lint --dir .
```

Catches cross-field invariants the JSON Schema can't (dangling
`runtime_ref`, capability self-cycles, invalid semver, breaking changes
without a version bump). Each rule has a stable code; see
`crates/greentic-extension-sdk-cli/src/commands/lint.rs` for the
catalogue.

### Quick dev-loop install

```bash
gtdx dev --mount ./path/to/built/ext
```

One-shot strict-parity install — build + pack + install the same way
production install would, but for an already-built source dir (no
watcher loop).

### Sign and publish

```bash
gtdx keygen > my-key.pem
gtdx sign --key my-key.pem ./
gtdx login                       # auth to Greentic Store
gtdx publish ./
```

## Audit + roadmap

The May 2026 extensions audit completion summary, design rationale, and
PR map for every shipped phase live under
[`docs/superpowers/`](./docs/superpowers/):

- [`specs/2026-05-13-extensions-1.0-cleanup.md`](./docs/superpowers/specs/2026-05-13-extensions-1.0-cleanup.md) — umbrella spec + full PR map
- [`plans/2026-05-13-contract-0.5.0-bump.md`](./docs/superpowers/plans/2026-05-13-contract-0.5.0-bump.md) — Phase A (contract → v2 + migration)
- [`plans/2026-05-13-security-hardening.md`](./docs/superpowers/plans/2026-05-13-security-hardening.md) — Phase D (D.5 trust root blocked on org decision)
- [`plans/2026-05-13-dx-cleanup.md`](./docs/superpowers/plans/2026-05-13-dx-cleanup.md) — Phase E (DX cleanup, all shipped)

## WIT specification

The canonical WebAssembly Component Model interface specifications for all extension kinds live under [`wit/`](./wit/):

- `extension-base.wit` — shared types
- `extension-host.wit` — host-side imports available to extensions
- `extension-bundle.wit` — `BundleExtension` world (packages designer sessions)
- `extension-design.wit` — `DesignExtension` world (authoring)
- `extension-deploy.wit` — `DeployExtension` world (deployment)
- `extension-provider.wit` — `ProviderExtension` world

The WIT package versions are declared as `@0.1.0` in each `wit/*.wit` file (this is the contract surface scaffolded extensions import against — see `CONTRACT_VERSION` in `crates/greentic-extension-sdk-cli/src/scaffold/embedded.rs`). The crate / workspace ships at the version declared in the root `Cargo.toml`. The `gtdx` binary embeds a copy of the current WIT package set under `crates/greentic-extension-sdk-cli/embedded-wit/$CARGO_PKG_VERSION/` (auto-populated from `wit/` by `build.rs`) so scaffolding works offline without network access to crates.io.

## Local development

```bash
bash ci/local_check.sh
```

Runs `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, `cargo build --release`, plus `cargo publish --dry-run` on leaf crates.

## Toolchain

- Rust 1.95.0 (pinned via `rust-toolchain.toml`)
- Edition 2024
- Targets: `wasm32-wasip2` for WASM components

## License

[MIT](./LICENSE)
