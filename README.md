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

There are two channels. **Stable** is cut from `main` and published to
crates.io. **Research** is the active integration line: it ships tagged
binaries to GitHub Releases but is deliberately never published to
crates.io (`release.yml` skips any tag containing `research`).

**Use 1.2.10 or newer.** Two things landed in it.

It is the first release where an extension can contribute a page, not just
parts of one: `contributions.views[]` ships an author's own HTML, JS and CSS
inside the pack, and `gtdx new --with-view` scaffolds a working example. No
host renders views yet — the Designer and the Admin console land separately —
so a view-bearing extension declares, lints, validates and packs today, and
displays once the hosts catch up. Read
[docs/authoring-views.md](docs/authoring-views.md) before publishing one:
`Contributions` is `deny_unknown_fields`, so a `views` key makes the whole
describe unparseable to every designer released so far, and
`min_designer_version` cannot yet say so.

That is the same mechanism behind the release's second change. It is also the
first release where a decode failure says what actually went wrong. `gtdx
install` against a store serving a newer describe used to fail with reqwest's
`error decoding response body` — no field, no position, no endpoint — which
reads as a network fault. The real
cause is a version skew: `DescribeJson`'s nested types are
`deny_unknown_fields`, so one unrecognised field fails the whole parse. The
message now names the endpoint, the field, and the fix.

1.2.9 before it gave every kind example tests, not just four — `ci/local_check.sh`
has always run `cargo test`, and on `llm` and `mcp` there was still nothing
for it to run.

1.2.8 before it gave `design`, `bundle`, `deploy` and `provider` their tests,
and added a Testing section to the scaffolded `AGENTS.md` — including the one
prerequisite that otherwise reads as a broken project: `cargo test` needs the
generated `src/bindings.rs`, so a fresh clone must build once first.

1.2.7 before it gave `design`, `bundle`, `deploy` and `provider` a working
example. Before that they implemented every export as an empty stub, so a
fresh extension built, packed and installed cleanly — and contributed
nothing.

1.2.6 before it settled `gtdx dev` watch mode: a `cargo component build`
re-touched the very paths the watcher watches — `src/bindings.rs`,
`wit/deps/`, `Cargo.toml` — so every build queued the next, roughly three
rebuilds a second on an untouched scaffold, forever. `gtdx dev --once` was
never affected.

1.2.5 before it made `gtdx doctor` see the whole machine: the diagnostic had
skipped `provider` extensions entirely, so a whole kind was invisible to the
command meant to find broken ones. It also stopped `doctor` printing a line
per installed extension — failures group by reason and passing extensions
collapse to a count, with `--verbose` for the full listing.

1.2.4 before it made the whole lifecycle work for every extension kind:
`gtdx uninstall` could not remove a `provider` extension and reported success
anyway; `gtdx enable` / `disable` could not see an `mcp` extension at all; a
`--kind wasm-component` scaffold failed `gtdx lint --publish` even with a
digest-pinned `--component-ref`; `gtdx install` failed on any
`GREENTIC_HOME` containing `..`, blaming the pack; and `gtdx outdated`
reported the built-in store as "not configured".

1.2.3 before it fixed the two defects that could only reach authors as a
release: `cargo binstall` pointed at the wrong path inside the release
archive and silently fell back to a full source build, and `gtdx publish`
shipped all-zero `sha256` placeholders so `gtdx lint --publish` failed on a
freshly scaffolded project and kept failing after a successful publish.

Below 1.2.1 a fresh scaffold does not build at all: `wit/world.wit` rendered
a single contract version for packages that are versioned independently, so
the first `cargo component build` failed for every kind except `mcp` with
`package 'greentic:extension-host@0.2.0' not found`. 1.2.1 also fixed the
`provider` and `--kind llm` stubs and made `--kind wasm-component` emit a
node the runner can execute.

**Recommended — `cargo binstall` (no compile, fetches the release binary):**

```bash
cargo binstall greentic-extension-sdk-cli
```

Resolves to the latest stable release from crates.io; binstall reads
`[package.metadata.binstall]` to find the matching GitHub Release asset.
Add `--version <x.y.z>` to pin — but pinning `1.2.2` or older re-enters the
broken binstall metadata, so it compiles from source instead of downloading.

**Build from source (slowest, needs the full toolchain):**

```bash
cargo install greentic-extension-sdk-cli
```

**Manual download:**

Grab the asset for your platform from the
[latest release](https://github.com/greenticai/greentic-designer-sdk/releases/latest)
— the tag is embedded in the filename:

```bash
# macOS Apple Silicon example — swap TAG and TARGET for your platform
TAG=v1.2.10
TARGET=aarch64-apple-darwin
curl -L -o gtdx.tgz \
  "https://github.com/greenticai/greentic-designer-sdk/releases/download/$TAG/gtdx-$TAG-$TARGET.tgz"
# every archive holds the binary inside a directory named after the asset
tar -xzf gtdx.tgz
chmod +x "gtdx-$TAG-$TARGET/gtdx"
mv "gtdx-$TAG-$TARGET/gtdx" ~/.cargo/bin/
```

**Research line** — not on crates.io, so install it from the repo at a
research tag. Pick the newest `v*-research*` tag from
[Releases](https://github.com/greenticai/greentic-designer-sdk/releases):

```bash
cargo install --git https://github.com/greenticai/greentic-designer-sdk \
  --tag <v-research-tag> \
  greentic-extension-sdk-cli
```

Older `-research` versions do sit on crates.io from before the skip rule
landed. Avoid opting into pre-releases there: a pre-release of a *higher*
version sorts above the current stable while carrying older code, so
`1.3.0-research.1` outranks the 1.2.10 stable. (A pre-release of the same
version is not a trap — `1.2.10-research` would sort *below* `1.2.10`.)

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

# wasm-component wraps an already-published component as a palette node, so it
# needs that component's OCI reference — digest-pinned. Omit it and the
# scaffold writes a placeholder that `gtdx lint --publish` refuses.
gtdx new my-node --kind wasm-component \
    --component-ref oci://ghcr.io/greenticai/component/component-my-node@sha256:461c6a68…

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

#### Contribute a view (a UI page in the Designer or Admin console)

> **Phase 1 (SDK-only):** you can declare, scaffold, lint, validate and pack a
> view today — no host renders one yet, and a view-bearing `describe.json` is
> unloadable on every designer released so far (see
> [`docs/authoring-views.md`](./docs/authoring-views.md#authoring-views) for
> why). Treat `--with-view` as local development, not something to publish.

```bash
gtdx new my-ext --kind design --with-view   # adds a working example page
```

Scaffolds an example `assets/views/hello/` page (HTML/JS/CSS) wired through
the host's `postMessage` bridge, plus the matching `contributions.views[]`
and `runtime.permissions.ui` entries in `describe.json`. See
[`docs/authoring-views.md`](./docs/authoring-views.md) for the full authoring
guide — what ships, what the sandboxed page can and can't reach, and the
`E_VIEW_*` / `W_VIEW_SLOT_UNKNOWN` lint codes.

#### Contribute an addon (a managed service like Qdrant, Redis, Postgres)

> **Phase 1 (SDK-only):** you can declare, validate, lint and pack an addon
> today — nothing in the platform reconciles it yet, and an addon-bearing
> `describe.json` is unloadable on every designer released so far (see
> [`docs/authoring-addons.md`](./docs/authoring-addons.md#authoring-addons)
> for why). Treat `contributions.addons[]` as something you develop and
> validate locally, not something you ship.

```json
"contributions": {
  "addons": [{
    "id": "qdrant",
    "family": "vector-db",
    "display_name": "Qdrant",
    "description": "Vector database for embeddings and similarity search.",
    "config_schema": "{\"type\":\"object\"}",
    "desired_state_schema": "{\"type\":\"object\"}"
  }]
}
```

There's no `gtdx new --with-addon` scaffold in phase 1; you hand-write
`contributions.addons[]` and validate it with `gtdx validate` and
`gtdx lint`. See [`docs/authoring-addons.md`](./docs/authoring-addons.md)
for the full authoring guide — every field, why secrets never belong in
`desired_state_schema`, and the `E_ADDON_*` / `W_ADDON_FAMILY_UNKNOWN`
lint codes.

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
gtdx publish --wasm ./weatherapi/weatherapi.component.wasm --manifest ./weatherapi/Cargo.toml
```

Running `gtdx new` with no flags starts an interactive wizard; for `--kind mcp`
it offers to seed from an OpenAPI spec. Pass `-y` for non-interactive defaults.

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
| `E_ENGINE_DEPRECATED` | the `engine` block is forbidden | Move version constraints into `compat.min_designer_version` / `compat.min_runner_version` and delete `engine`. Templates stopped emitting it in 1.2.1, so this no longer fires on a fresh scaffold. |
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

### Designer compatibility

`gtdx` scaffolds against the current describe contract, `greentic.ai/v2`. A
designer older than **1.2.0** does not understand that contract: it screens the
extension out at boot with a terse `built for a newer designer` line, so the
extension never shows up in `/api/extensions` and there is nothing in the logs
pointing at the version. If a freshly built extension is simply invisible in
Designer, this is almost always why.

| Designer | `greentic.ai/v1` | `greentic.ai/v2` | SDK / `gtdx` |
|----------|------------------|------------------|--------------|
| `< 1.2.0` | loads | **skipped at boot** | `0.4.x` |
| `>= 1.2.0` | loads (migrated on read) | loads | `1.2.x` and newer |

`gtdx doctor` checks this for you against the designer you actually have
installed, and names the fix:

```console
$ gtdx doctor
designer compatibility
  ✓ greentic-designer 1.1.7  /usr/local/bin/greentic-designer
  ✗ greentic.telco-x-designer 0.1.0: declares greentic.ai/v2, which designer
    1.1.7 cannot load (it is skipped at boot as "built for a newer designer")
    — upgrade greentic-designer to >=1.2.0
```

The version comes from `greentic-designer --version`, which every lineage back
to 1.1.x supports. Running Designer from a checkout instead of `PATH`? Point
doctor at that build:

```bash
GREENTIC_DESIGNER_BIN=../greentic-designer/target/release/greentic-designer gtdx doctor
```

On top of the contract gate, doctor also honours the range the extension itself
declares — `compat.min_designer_version` on a v2 describe, or the equivalent
`engine.greenticDesigner` on a v1 one.

You do not have to remember to run doctor: `gtdx dev` and `gtdx install` run the
same check right after installing, so a designer that will not load what you just
built says so where you are already looking:

```console
$ gtdx dev --once
✓ installed my-ext@0.1.0
⚠ my-ext installed, but this designer cannot load it: declares greentic.ai/v2, …
```

Note what a scaffold declares: `min_designer_version` is the **contract floor**
(`>=1.2.0`), not the version of the SDK that generated it. Those are different
axes — an extension built by SDK 1.3.x still loads on any designer that speaks
v2. `contract_version` is the one that tracks the SDK.

There is deliberately no flag to emit a v1 describe from a v2 SDK. Downgrading
the contract would mean maintaining two describe shapes forever while still
lying about extensions that genuinely need v2 features; upgrading Designer is
the supported path.

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
gtdx publish --wasm ./out/my-mcp.component.wasm --manifest ./describe-dir/Cargo.toml
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

## End-to-end: scaffold → Designer

The full first-time loop, from an empty directory to a tool the agent can call.

```bash
# 0. Confirm the machine is ready — and that your Designer can load what
#    gtdx is about to build. Do this FIRST; it is the step that saves the day.
gtdx doctor

# 1. Scaffold. --kind design is the right pick for authoring tools the
#    Designer/agent calls; see the kind table under "Scaffold and build".
gtdx new my-ext --kind design
cd my-ext

# 2. Implement your tools, then build + pack + install in one shot.
gtdx dev --once
#    → dist/my-ext-0.1.0.gtxpack
#    → installed into ~/.greentic/extensions/design/my-ext-<version>/

# 3. Re-run doctor now that it is installed: this is where a designer/contract
#    mismatch shows up by name instead of as a silently missing extension.
gtdx doctor

# 4. Start Designer and confirm it loaded.
greentic-designer                                    # binds :8080 by default
curl -s localhost:8080/api/extensions | jq '.[].id'

#    From a Designer checkout instead, the dev loop binds :4000:
#      make dev-rust  →  curl -s localhost:4000/api/extensions | jq '.[].id'
```

Your extension id should appear in step 4. If it does not:

| Symptom | Cause | Fix |
|---------|-------|-----|
| Not in `/api/extensions`, boot log says `built for a newer designer` | Designer `< 1.2.0` cannot read a `greentic.ai/v2` describe | Upgrade Designer — see [Designer compatibility](#designer-compatibility) |
| Not in `/api/extensions`, no boot log line at all | Not installed where Designer looks | Check `~/.greentic/extensions/design/`; re-run `gtdx dev --once` |
| Boot log says `incomplete install` / `runtime artifact unavailable` | `extension.wasm` missing or not a WASM component | Rebuild: `gtdx dev --once --release` |
| Loads, but the agent never calls it | Tool descriptions too vague to route on | Sharpen `description` on each tool in `describe.json` |

`gtdx dev` without `--once` keeps watching and reinstalling on every save,
which is the loop to stay in once step 4 has worked once.

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
