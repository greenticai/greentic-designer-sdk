# greentic-designer-sdk

Public SDK for authoring [Greentic Designer](https://greentic.ai) extensions — Bundle, Design, Deploy, and Provider extension kinds.

## What this is

A workspace containing the public-facing tooling and types for building Greentic Designer extensions:

| Crate | Description |
|---|---|
| `greentic-ext-contract` | Type definitions, `describe.json` schema, signing/verification primitives |
| `greentic-ext-state` | Persistent enable/disable state for installed extensions |
| `greentic-ext-registry` | Registry client (HTTP + OCI) and install lifecycle |
| `greentic-ext-testing` | Test utilities: fixtures, gtxpack helpers |
| `greentic-ext-cli` | The `gtdx` command-line tool: scaffold, build, validate, sign, publish |

The runtime engine that *executes* WASM extensions is part of the commercial Greentic Designer platform and is not included here. This SDK gives developers everything they need to author, validate, sign, and publish extensions; execution happens on the Greentic platform.

## Quick start

### Install the CLI

```bash
cargo install greentic-ext-cli
# or, when binary release pipeline is up:
# cargo binstall gtdx
# or download from GitHub Releases
```

This installs the `gtdx` binary.

### Scaffold and build an extension

```bash
gtdx new my-bundle-ext --kind bundle
cd my-bundle-ext
gtdx dev --once
```

This rebuilds, packs, and produces `dist/<name>-<version>.gtxpack`.

### Sign and publish

```bash
gtdx keygen > my-key.pem
gtdx sign --key my-key.pem ./
gtdx login                       # auth to Greentic Store
gtdx publish ./
```

## WIT specification

The canonical WebAssembly Component Model interface specifications for all extension kinds live under [`wit/`](./wit/):

- `extension-base.wit` — shared types
- `extension-host.wit` — host-side imports available to extensions
- `extension-bundle.wit` — `BundleExtension` world (packages designer sessions)
- `extension-design.wit` — `DesignExtension` world (authoring)
- `extension-deploy.wit` — `DeployExtension` world (deployment)
- `extension-provider.wit` — `ProviderExtension` world

Versions are pinned at `0.4.0`. The `gtdx` binary embeds a copy under `crates/greentic-ext-cli/embedded-wit/` for offline scaffolding.

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
