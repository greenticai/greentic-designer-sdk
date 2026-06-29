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

There are two ways to scaffold. Pick whichever you prefer:

```bash
# 1) Flag-driven (scriptable, CI-friendly)
gtdx new my-ext --kind design     # or: bundle | deploy | provider | wasm-component | llm | mcp

# 2) Interactive wizard — just run `gtdx new` with no name on a terminal
gtdx new                          # prompts for name, kind, id, version, author, license
gtdx new --wizard                 # force the wizard even when flags are given

cd my-ext
gtdx dev --once
```

The wizard uses any flags you pass as prompt defaults, so `gtdx new my-ext --wizard`
pre-fills the name. Pass `--yes` to skip the wizard and resolve everything from
flags/defaults (useful in scripts and CI, where there is no terminal).

This rebuilds, packs, and produces `dist/<name>-<version>.gtxpack`. The
pack includes a `manifest.json` integrity ledger (sha256 of every entry)
since 1.2.0-research; runtime install verifies it.

#### MCP kinds: `wasix:mcp/router` vs agent-only design-extension MCPs

`gtdx new --kind mcp` scaffolds a `wasix:mcp/router` WASM component. This is a
**flow-capable local MCP router**: addressable as a flow node (`dw.mcp.<id>`),
loaded by the Greentic MCP executor (`greentic-mcp`), and suitable for both
agentic workers and direct flow usage.

**Migration note for existing design-extension MCPs:** older MCP tool-sets
scaffolded as `--kind design` (with `kind: DesignExtension` in `describe.json`)
are agent-only toolsets that run inside the agentic worker loop. They are NOT
flow-capable local-wasm MCPs. To make a design-extension MCP flow-capable:

1. Re-scaffold with `gtdx new --kind mcp` to get the `wasix:mcp/router` world.
2. Port your tool handlers into the new scaffold.
3. Republish — the `kind` field in `describe.json` will be `wasix:mcp/router`
   and the SDK's `ExtensionKind::WasixMcpRouter` variant will be used throughout.

Old design-extension MCPs continue to work for agentic workers and do not need
to be migrated unless flow-node addressability is needed.

### Lint before publish

```bash
gtdx lint --dir .
```

Catches cross-field invariants the JSON Schema can't (dangling
`runtime_ref`, capability self-cycles, invalid semver, breaking changes
without a version bump). Each rule has a stable code; see
`crates/greentic-extension-sdk-cli/src/commands/lint/rules.rs` for the
catalogue.

#### Governance rules (2026-06)

These enforce cross-extension consistency. Run `gtdx lint --dir <ext>`
locally, or `gtdx lint --publish --dir <ext>` to also enforce
`E_SHA256_ZERO`.

| Code | Rule | Fix |
|------|------|-----|
| `E_SCHEMA_HOST` | `$schema` must be `https://store.greentic.cloud/schemas/describe-v2.json` | Replace any `store.greentic.ai` (or missing) `$schema` with the canonical URL. |
| `E_EXPORT_FORM` | `tools[].export` must be a fully-qualified `greentic:extension-design/<interface>.<member>` reference (e.g. `tools.invoke-tool`, `validation.validate-content`, `knowledge.get-entry`) | Replace bare names like `"invoke-tool"` with the fully-qualified form. |
| `E_ENGINE_DEPRECATED` | the `engine` block is forbidden | Move version constraints into `compat.min_designer_version` / `compat.min_runner_version` and delete `engine`. |
| `E_SHA256_ZERO` | (`--publish` only) no placeholder `0000…` hashes | Let the build/publish step fill real `sha256` values before publishing. |
| `E_ID_PATTERN` | `metadata.id` must match `^greentic\.[a-z0-9][a-z0-9-]*$` | Use a lowercase-kebab id under the `greentic.` namespace. |
| `E_TOOL_NAMING` | tool names must be `snake_case` with no near-duplicate prefixes | Rename camelCase tools; disambiguate pairs like `generate_gtpack` / `generate_gtpack_from_sorla_yaml`. |

### Quick dev-loop install

```bash
gtdx dev --mount ./path/to/built/ext
```

One-shot strict-parity install — build + pack + install the same way
production install would, but for an already-built source dir (no
watcher loop).

### Log in to the store

The default registry is the public Greentic store, `greentic-store`
(`https://store.greentic.cloud`) — it is built in, so you do **not** need to
`gtdx registries add` it first.

```bash
gtdx login                  # browser device login (OAuth 2.0 Device Grant)
gtdx login --no-browser     # print the URL + code instead of opening a browser
gtdx login --paste          # skip device login; paste a token manually
gtdx login --token <TOKEN>  # non-interactive (also reads $GTDX_TOKEN) — for CI
```

By default `login` runs the **OAuth 2.0 Device Authorization Grant** (RFC 8628):
it shows a short code, opens the store's `/device` page, and waits while you sign
in and approve in the browser — then a fresh token is minted and stored in
`~/.greentic/credentials.toml` (owner-only). No copy-pasting required. Against a
store that does not implement device login, it transparently falls back to the
manual token paste. To target a different store, configure it once and it
overrides the built-in:

```bash
gtdx registries add greentic-store https://staging.store.example  # override URL
gtdx login --registry <name>                                       # or a named registry
```

### Sign and publish

`keygen`, `sign`, and `publish --sign` all use one key format: **PKCS8 PEM**
ed25519.

```bash
gtdx keygen --out my-key.pem            # PKCS8 PEM private key (mode 0600)
gtdx login                              # auth to the Greentic store (see above)
gtdx publish --sign --key my-key.pem ./ # bind manifest, then sign — one step
```

Prefer not to memorise the flags? `gtdx publish --wizard` walks you through
registry, mode (real / dry-run / verify-only), signing + key source, trust
policy and overwrite — using any flags you pass as defaults. The wizard is
opt-in, so plain `gtdx publish` keeps its scripted, CI-friendly behaviour.

`publish --sign` binds the whole-archive manifest into `describe.json` and then
signs, so the embedded signature covers the entire pack. Key sources, in
precedence order: `--key <path>`, `--key-id <id>` (loads
`~/.greentic/keys/<id>.key`), or the `GREENTIC_EXT_SIGNING_KEY_PEM` env var (CI).

To sign a `describe.json` in isolation (outside a pack), use `gtdx sign --key
my-key.pem ./`.

To publish a component that was **built outside this CLI** (e.g. a generated MCP
component), pass `--wasm <path>` so `publish` packs that artifact instead of
running `cargo component build`. The `describe.json` in `--manifest`'s directory
still drives the pack, signing, and registry metadata — only the build step is
skipped:

```bash
gtdx publish --wasm ./out/my-mcp.component.wasm --manifest ./describe-dir/Cargo.toml ./
```

### Verify a pack

```bash
gtdx verify ./dist/my-ext-0.1.0.gtxpack                  # integrity + binding + ledger
gtdx verify ./dist/my-ext-0.1.0.gtxpack --trusted-key <b64-pubkey>  # + authenticity
```

For an archive, `verify` runs the full chain: the `describe.json` signature, the
manifest binding (`manifestSha256`), and the whole-archive integrity ledger
(`manifest.json`) — so a tampered `extension.wasm` or smuggled file is caught,
not just a tampered descriptor. Without `--trusted-key` the signature is only
checked for self-consistency (the describe is unmodified); pass `--trusted-key`
to additionally anchor *who* signed it.

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
