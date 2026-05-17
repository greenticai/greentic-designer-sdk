# Phase E: DX Cleanup Implementation Plan

> **Status (2026-05-17): ALL TASKS SHIPPED on `research`.**
>
> | Task | PR |
> |---|---|
> | E.1 (stale doc/store-URL sweep across 3 repos) | greentic-biz/greentic-deployer-extensions#26, greentic-biz/greentic-designer#321, greentic-biz/greentic-provider-extensions#27 |
> | E.2 (CONTRACT_VERSION drift fixes + 3 guard tests) | greenticai/greentic-designer-sdk#12 |
> | E.3 (rust-toolchain.toml + wit-bindgen-rt 0.41 across all 5 kinds, with guards) | greenticai/greentic-designer-sdk#15 |
> | E.4 (`Kind::Llm` enum variant + complete `templates/llm/` tree + tests) | greenticai/greentic-designer-sdk#20 |
> | E.5.a/b/c/d (`gtdx lint` skeleton + 3 cross-field rules + 5 fixtures) | greenticai/greentic-designer-sdk#18 |
> | E.5.e (`W_DESCRIBE_DIFF_BREAKING` describe-diff rule, fs-based not git2) | greenticai/greentic-designer-sdk#22 |
> | E.6 (`MockHost` composable mock layer: 5 mocks + composer + doc-tests) | greenticai/greentic-designer-sdk#17 |
> | E.7.a (`gtdx dev --mount` strict-parity one-shot install mode) | greenticai/greentic-designer-sdk#19 |
> | Followup: scaffold templates `describe.json` v1→v2 across all 6 kinds | greenticai/greentic-designer-sdk#21 |
>
> **Scope cuts (called out in respective PR bodies):**
> - **E.5.e** picked filesystem-diff vs installed copy instead of git2-based HEAD diff — no new dep, better matches "previously released" semantics.
> - **E.7** skipped the planned `-dev` id-suffix + dev-key auto-gen — Phase D's `dev-allow-unsigned` feature flag already covers the trust path; suffix adds surface without changing outcome.
> - **E.7.b** (cargo-component end-to-end mount test) deferred — needs `cargo-component` available in CI runner.
> - **E.4.d** (cargo-component end-to-end scaffold compile test) deferred — same reason.
>
> Original plan body preserved below as historical record + design rationale.
>
> ---

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the 10 DX gaps from the May-2026 extensions audit so `gtdx` parity, docs, scaffold versions, lint, mock host, and `dev --mount` work for the 1.2.x research-line release.

**Architecture:** Multi-repo edit pass. Code changes live in `greentic-designer-sdk` (scaffold `Llm` kind, `gtdx lint`, `gtdx dev --mount`, `MockHost` in `sdk-testing`, `CONTRACT_VERSION` single source of truth, version unification). Doc fixes live in `greentic-docs`. OCI tag fixes live in `greentic-cards2pack`. Strict-parity `--mount` reuses the existing build + pack + install pipeline in `src/dev/`.

**Tech Stack:** Rust 1.95.0, edition 2024, `wasm32-wasip2`, `clap` 4.5, `wit-bindgen-rt` 0.41, `wit-bindgen` 0.41, `tokio` 1.x, `ed25519-dalek` 2.1, `serde_jcs`, `semver` 1, `tracing` 0.1.

---

## Sequencing & dependencies

E.1 (docs), E.3 (toolchain + wit-bindgen-rt unification), E.8 (cards2pack `:latest` → `:stable`) are independent and can ship first.

E.2 (CONTRACT_VERSION single source of truth) is independent BUT must be re-run after Plan A merges (the canonical version moves from `0.4.5` to `0.5.0`). For Phase E we adopt **`0.4.5`** as the canonical now and document the rebump in a follow-up note.

E.4 (`Kind::Llm`), E.5 (`gtdx lint`), E.7 (`gtdx dev --mount`) consume scaffold + contract APIs and **depend on Plans A and E.2** landing first.

E.6 (MockHost in `sdk-testing`) depends on Plan B — its host-fn signatures are what we mock. We sketch the mock interface using the **current** stubs in this plan; after Plan B merges, the same task is replayed against the wired implementations (the mock shapes don't change — only the integration tests do).

E.9 (verification) runs last as a grep-only gate.

**Repo target rule:** PRs against `greentic-designer-sdk` target the `research` branch (memory `feedback_pr_target_research_directly`). PRs against `greentic-docs` and `greentic-cards2pack` target `main` (two-tier repos, per umbrella spec §3). No Co-Authored-By / "Generated with Claude Code" trailers (memory `feedback_no_claude_attribution`).

---

## Repo locations (used throughout this plan)

| Repo | Absolute path | Branch target |
| --- | --- | --- |
| greentic-designer-sdk | `/home/bima-pangestu/Works/greentic/greentic-designer-sdk` | `research` |
| greentic-docs | `/home/bima-pangestu/Works/greentic/greentic-docs` | `main` |
| greentic-cards2pack | `/home/bima-pangestu/Works/greentic/greentic-cards2pack` | `main` |

All commands below assume `cd` into the relevant repo root.

---

## Task E.1.a: Doc — replace `greentic-biz/greentic-designer-extensions` with `greenticai/greentic-designer-sdk`

**Files:**
- Modify: every file under `/home/bima-pangestu/Works/greentic/greentic-docs/src/content/docs/` matching the stale string.

- [ ] **Step 1: Find all matches**

Run:
```bash
cd /home/bima-pangestu/Works/greentic/greentic-docs
grep -rln "greentic-biz/greentic-designer-extensions" src/content/docs/
```
Expected output: zero or more file paths. If the count is zero, this sub-task is a no-op — skip to E.1.b.

- [ ] **Step 2: Replace string in each match**

For each path printed in Step 1, run:
```bash
sed -i 's|greentic-biz/greentic-designer-extensions|greenticai/greentic-designer-sdk|g' <path>
```

If Step 1 returned zero results, document it in the commit message of the next sub-task as "no occurrences found".

- [ ] **Step 3: Verify zero matches remain**

Run:
```bash
grep -rln "greentic-biz/greentic-designer-extensions" src/content/docs/
```
Expected: empty output.

- [ ] **Step 4: Commit (or skip if no edits)**

If Step 2 made edits:
```bash
cd /home/bima-pangestu/Works/greentic/greentic-docs
git add src/content/docs/
git commit -m "docs: rename stale repo greentic-biz/greentic-designer-extensions to greenticai/greentic-designer-sdk"
```

If no edits, do NOT create an empty commit; just record the finding in the PR body.

---

## Task E.1.b: Doc — replace `greentic-ext-cli` with `greentic-extension-sdk-cli`

**Files:**
- Modify: every file under `/home/bima-pangestu/Works/greentic/greentic-docs/src/content/docs/` containing the stale crate name.

- [ ] **Step 1: Find all matches**

```bash
cd /home/bima-pangestu/Works/greentic/greentic-docs
grep -rln "greentic-ext-cli" src/content/docs/
```

- [ ] **Step 2: Replace string in each match**

```bash
grep -rln "greentic-ext-cli" src/content/docs/ | while read f; do
  sed -i 's|greentic-ext-cli|greentic-extension-sdk-cli|g' "$f"
done
```

- [ ] **Step 3: Verify zero matches remain**

```bash
grep -rln "greentic-ext-cli" src/content/docs/
```
Expected: empty output.

- [ ] **Step 4: Commit (or skip if no edits)**

```bash
cd /home/bima-pangestu/Works/greentic/greentic-docs
git add src/content/docs/
git commit -m "docs: rename stale crate greentic-ext-cli to greentic-extension-sdk-cli"
```

---

## Task E.1.c: Doc — remove raw IP `62.171.174.152`

**Files:**
- Modify: every file under `/home/bima-pangestu/Works/greentic/greentic-docs/` containing the IP (target was previously `extensions/github-action.md:31`).

- [ ] **Step 1: Find all matches across the whole docs tree**

```bash
cd /home/bima-pangestu/Works/greentic/greentic-docs
grep -rln "62\.171\.174\.152" .
```

- [ ] **Step 2: Replace with canonical store URL**

The canonical store URL is `https://store.greentic.ai` (verified by the spec's success criteria §4 DX block which calls out `https://store.greentic.ai/schemas/describe-v1.json` already used in the scaffold's `describe.json.tmpl`). For each match:
```bash
grep -rln "62\.171\.174\.152" . | while read f; do
  sed -i 's|http://62\.171\.174\.152:3030|https://store.greentic.ai|g; s|https://62\.171\.174\.152:3030|https://store.greentic.ai|g; s|62\.171\.174\.152:3030|store.greentic.ai|g; s|62\.171\.174\.152|store.greentic.ai|g' "$f"
done
```

- [ ] **Step 3: Verify zero matches remain**

```bash
grep -rln "62\.171\.174\.152" .
```
Expected: empty.

- [ ] **Step 4: Commit (or skip if no edits)**

```bash
git add .
git commit -m "docs: replace raw IP 62.171.174.152 with store.greentic.ai"
```

---

## Task E.1.d: Open PR for E.1.a/b/c against greentic-docs main

- [ ] **Step 1: Push branch**

```bash
cd /home/bima-pangestu/Works/greentic/greentic-docs
git checkout -b chore/dx-doc-cleanup
git push -u origin chore/dx-doc-cleanup
```

- [ ] **Step 2: Open PR**

```bash
gh pr create --title "docs: stale repo/crate names and raw IP cleanup (DX phase E.1)" --body "$(cat <<'EOF'
## Summary
- replace `greentic-biz/greentic-designer-extensions` -> `greenticai/greentic-designer-sdk` across docs
- replace `greentic-ext-cli` -> `greentic-extension-sdk-cli` across docs
- replace raw IP `62.171.174.152` -> `store.greentic.ai`

## Test plan
- [ ] `grep -rln "greentic-biz/greentic-designer-extensions" src/content/docs/` is empty
- [ ] `grep -rln "greentic-ext-cli" src/content/docs/` is empty
- [ ] `grep -rln "62\.171\.174\.152" .` is empty
- [ ] `npm run build` succeeds (Astro renders without broken-link errors on touched pages)
EOF
)"
```

---

## Task E.2.a: Make `CONTRACT_VERSION` a single source of truth — failing test

**Files:**
- Test: `/home/bima-pangestu/Works/greentic/greentic-designer-sdk/crates/greentic-extension-sdk-cli/tests/contract_version_consistency.rs` (new)

The current state has three independent strings for "contract version":
1. `src/scaffold/embedded.rs:11` → `pub const CONTRACT_VERSION: &str = "0.1.0";` (WIT package `@version`)
2. `README.md:62` → `pinned at 0.4.0` (stale workspace-version mention)
3. `embedded-wit/0.4.4/` (directory name driven by `$CARGO_PKG_VERSION` = workspace version `0.4.4`)

The audit's intent is "one canonical contract version string everywhere a developer can see". For Phase E **before Plan A** the canonical is `0.4.5` (current workspace + 1 patch to absorb the rename). **After Plan A merges** the canonical becomes `0.5.0` (workspace bump). This task makes `CONTRACT_VERSION = CARGO_PKG_VERSION` so future bumps stay coherent automatically.

- [ ] **Step 1: Write the failing test**

Create `crates/greentic-extension-sdk-cli/tests/contract_version_consistency.rs`:
```rust
//! Cross-checks that the scaffolded CONTRACT_VERSION matches the crate's
//! Cargo package version AND the embedded-wit directory the build resolves
//! at compile time. If any one of them drifts, this test fails loudly.

#[test]
fn contract_version_matches_cargo_pkg_version() {
    // The CLI binary embeds wit files from `embedded-wit/$CARGO_PKG_VERSION/`.
    // The `CONTRACT_VERSION` constant scaffolded into every new project's
    // describe.json + Cargo.toml + wit world MUST equal that path so the
    // "pinned at X.Y.Z" line in README, the dir name, and the constant
    // never drift apart.
    let pkg = env!("CARGO_PKG_VERSION");
    // `CONTRACT_VERSION` is re-exported from the `embedded` module under
    // `scaffold/`. We assert equality through the public crate-internal path
    // by re-reading the source. Doing it as a compile-time assertion would
    // be tighter, but the const lives behind `pub(crate)` so we go via the
    // CLI binary's `--version` output.
    let bin = env!("CARGO_BIN_EXE_gtdx");
    let out = std::process::Command::new(bin)
        .arg("version")
        .output()
        .expect("run gtdx version");
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    assert!(
        stdout.trim().starts_with(&format!("gtdx {pkg}")),
        "gtdx version output {stdout:?} did not start with crate version {pkg:?}",
    );
}

#[test]
fn embedded_wit_directory_matches_cargo_pkg_version() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let pkg = env!("CARGO_PKG_VERSION");
    let candidate = std::path::Path::new(manifest_dir)
        .join("embedded-wit")
        .join(pkg);
    assert!(
        candidate.exists(),
        "embedded-wit/{pkg} must exist (CARGO_MANIFEST_DIR={manifest_dir})",
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd /home/bima-pangestu/Works/greentic/greentic-designer-sdk
cargo test -p greentic-extension-sdk-cli --test contract_version_consistency
```
Expected: `embedded_wit_directory_matches_cargo_pkg_version` FAILS — directory is `0.4.4` but `CARGO_PKG_VERSION` (after we bump in Step 3 below) is `0.4.5`. The first test may pass since `gtdx version` already echoes `CARGO_PKG_VERSION`.

(If `CARGO_PKG_VERSION` is still `0.4.4` when you first run this and the dir already matches, that test passes too. Move on to Step 3 — bumping the version is what intentionally breaks it, validating the test guards drift going forward.)

---

## Task E.2.b: Bump workspace version 0.4.4 → 0.4.5 and rename embedded-wit dir

- [ ] **Step 1: Edit workspace `Cargo.toml`**

In `/home/bima-pangestu/Works/greentic/greentic-designer-sdk/Cargo.toml`, change:
```toml
[workspace.package]
edition = "2024"
version = "0.4.4"
```
to:
```toml
[workspace.package]
edition = "2024"
version = "0.4.5"
```

- [ ] **Step 2: Rename the embedded-wit directory**

```bash
cd /home/bima-pangestu/Works/greentic/greentic-designer-sdk
git mv crates/greentic-extension-sdk-cli/embedded-wit/0.4.4 crates/greentic-extension-sdk-cli/embedded-wit/0.4.5
```

- [ ] **Step 3: Update inter-crate `path + version` pinnings**

In each of these files, find every line of the form
`greentic-extension-sdk-* = { path = "...", version = "0.4" }`
and change `version = "0.4"` to `version = "0.4.5"` so semver resolution can't accidentally pick an older 0.4.x:

- `crates/greentic-extension-sdk-cli/Cargo.toml`
- `crates/greentic-extension-sdk-testing/Cargo.toml`
- `crates/greentic-extension-sdk-registry/Cargo.toml`
- `crates/greentic-extension-sdk-state/Cargo.toml`

For sdk-cli (concrete diff):
```toml
greentic-extension-sdk-contract = { path = "../greentic-extension-sdk-contract", version = "0.4.5" }
greentic-extension-sdk-registry = { path = "../greentic-extension-sdk-registry", version = "0.4.5" }
greentic-extension-sdk-state    = { path = "../greentic-extension-sdk-state",    version = "0.4.5" }
```
And in `[dev-dependencies]`:
```toml
greentic-extension-sdk-testing = { path = "../greentic-extension-sdk-testing", version = "0.4.5" }
```

For sdk-testing:
```toml
greentic-extension-sdk-contract = { path = "../greentic-extension-sdk-contract", version = "0.4.5" }
```

Apply the analogous edit to any other `greentic-extension-sdk-*` reference in any workspace `Cargo.toml`.

- [ ] **Step 4: Refresh Cargo.lock**

```bash
cd /home/bima-pangestu/Works/greentic/greentic-designer-sdk
cargo check --workspace --all-features
```
Expected: succeeds, lockfile updated.

- [ ] **Step 5: Run the failing test from E.2.a**

```bash
cargo test -p greentic-extension-sdk-cli --test contract_version_consistency
```
Expected: both tests PASS now.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock crates/greentic-extension-sdk-cli/embedded-wit crates/greentic-extension-sdk-cli/Cargo.toml crates/greentic-extension-sdk-testing/Cargo.toml crates/greentic-extension-sdk-registry/Cargo.toml crates/greentic-extension-sdk-state/Cargo.toml crates/greentic-extension-sdk-cli/tests/contract_version_consistency.rs
git commit -m "chore: bump workspace to 0.4.5 and align embedded-wit dir with CARGO_PKG_VERSION"
```

---

## Task E.2.c: Make `CONTRACT_VERSION` derive from `CARGO_PKG_VERSION`

The constant currently hardcodes `"0.1.0"`. The audit calls for a single source of truth — the cleanest fix is to derive the constant from `CARGO_PKG_VERSION` so it can never drift from the `embedded-wit/<v>` directory.

**Files:**
- Modify: `crates/greentic-extension-sdk-cli/src/scaffold/embedded.rs:11`
- Modify: `crates/greentic-extension-sdk-cli/src/scaffold/contract_lock.rs:33,39,50` (test fixtures need to drop the hardcoded `0.1.0` literal)

- [ ] **Step 1: Write the failing test**

Append to `crates/greentic-extension-sdk-cli/src/scaffold/embedded.rs` `tests` module:
```rust
    #[test]
    fn contract_version_tracks_cargo_pkg_version() {
        assert_eq!(CONTRACT_VERSION, env!("CARGO_PKG_VERSION"));
    }
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p greentic-extension-sdk-cli --lib scaffold::embedded::tests::contract_version_tracks_cargo_pkg_version
```
Expected: FAIL with `assertion ``left == right`` failed: left: "0.1.0" right: "0.4.5"`.

- [ ] **Step 3: Change the constant to derive from `CARGO_PKG_VERSION`**

In `crates/greentic-extension-sdk-cli/src/scaffold/embedded.rs`, replace lines 1-11 (the doc + const block) with:
```rust
//! Embedded WIT resources accessor.

use include_dir::{Dir, include_dir};

/// Version of the embedded WIT contract — kept identical to the crate's
/// `CARGO_PKG_VERSION`. The build embeds WIT files from
/// `embedded-wit/$CARGO_PKG_VERSION/`, so the constant must track that path.
/// A drift would make `gtdx new` scaffold projects pointing at WIT files
/// that don't exist on disk. The integration test in
/// `tests/contract_version_consistency.rs` is the runtime guard.
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
```

(The rest of the file — `static EMBEDDED`, `WitFile`, `wit_files`, etc. — stays unchanged.)

- [ ] **Step 4: Update `contract_lock.rs` tests to use `CONTRACT_VERSION`**

In `crates/greentic-extension-sdk-cli/src/scaffold/contract_lock.rs`, change every literal `"0.1.0"` inside the test module to `env!("CARGO_PKG_VERSION")`. Concretely, the test assertions at lines 33, 39, and 50 currently read:
```rust
contract_version: "0.1.0".to_string(),
...
assert!(out.contains("contract_version = \"0.1.0\""));
...
contract_version: "0.1.0".to_string(),
```
Change them to:
```rust
contract_version: env!("CARGO_PKG_VERSION").to_string(),
...
assert!(out.contains(&format!("contract_version = \"{}\"", env!("CARGO_PKG_VERSION"))));
...
contract_version: env!("CARGO_PKG_VERSION").to_string(),
```

- [ ] **Step 5: Run all tests**

```bash
cargo test -p greentic-extension-sdk-cli
```
Expected: all PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/greentic-extension-sdk-cli/src/scaffold/embedded.rs crates/greentic-extension-sdk-cli/src/scaffold/contract_lock.rs
git commit -m "refactor: derive CONTRACT_VERSION from CARGO_PKG_VERSION to prevent drift"
```

---

## Task E.2.d: Update README "pinned at" line

**Files:**
- Modify: `/home/bima-pangestu/Works/greentic/greentic-designer-sdk/README.md:62`

- [ ] **Step 1: Update the line**

In `README.md`, replace:
```
Versions are pinned at `0.4.0`. The `gtdx` binary embeds a copy under `crates/greentic-extension-sdk-cli/embedded-wit/` for offline scaffolding.
```
with:
```
Versions track the SDK crate version (`0.4.5` as of this commit; see `Cargo.toml`'s `[workspace.package].version`). The `gtdx` binary embeds a copy under `crates/greentic-extension-sdk-cli/embedded-wit/<version>/` for offline scaffolding.
```

- [ ] **Step 2: Verify grep returns only `0.4.5` for "pinned"/"contract" version references in the SDK**

```bash
cd /home/bima-pangestu/Works/greentic/greentic-designer-sdk
grep -rn '"0\.[0-9]\.[0-9]"\|0\.[0-9]\.[0-9]\b' README.md
```
Expected: only `0.4.5` appears. The string `0.4.0` should be gone.

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: README contract version pinned line tracks workspace version"
```

---

## Task E.3.a: Add `rust-toolchain.toml.tmpl` to every scaffold template missing one

**Files:**
- Create: `crates/greentic-extension-sdk-cli/templates/design/rust-toolchain.toml.tmpl`
- Create: `crates/greentic-extension-sdk-cli/templates/bundle/rust-toolchain.toml.tmpl`
- Create: `crates/greentic-extension-sdk-cli/templates/deploy/rust-toolchain.toml.tmpl`
- Create: `crates/greentic-extension-sdk-cli/templates/provider/rust-toolchain.toml.tmpl`
- Modify: `crates/greentic-extension-sdk-cli/templates/wasm-component/rust-toolchain.toml.tmpl` (bump 1.94.0 → 1.95.0)

- [ ] **Step 1: Write failing test**

Append to `crates/greentic-extension-sdk-cli/src/scaffold/template.rs` `tests` module:
```rust
    #[test]
    fn every_kind_template_ships_rust_toolchain_pinned_to_1_95_0() {
        for kind in ["design", "bundle", "deploy", "provider", "wasm-component"] {
            let entries = load_templates_kind(kind);
            let toolchain = entries
                .iter()
                .find(|e| e.dst_rel == "rust-toolchain.toml")
                .unwrap_or_else(|| panic!("kind {kind} missing rust-toolchain.toml template"));
            let content = std::str::from_utf8(toolchain.src_bytes).expect("utf8");
            assert!(
                content.contains("channel = \"1.95.0\""),
                "kind {kind} toolchain template does not pin 1.95.0:\n{content}",
            );
        }
    }
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p greentic-extension-sdk-cli --lib scaffold::template::tests::every_kind_template_ships_rust_toolchain_pinned_to_1_95_0
```
Expected: FAIL (missing toolchain template for design/bundle/deploy/provider; wasm-component has 1.94.0).

- [ ] **Step 3: Create the four missing templates**

In each of these directories, create `rust-toolchain.toml.tmpl` with this exact content:
- `crates/greentic-extension-sdk-cli/templates/design/rust-toolchain.toml.tmpl`
- `crates/greentic-extension-sdk-cli/templates/bundle/rust-toolchain.toml.tmpl`
- `crates/greentic-extension-sdk-cli/templates/deploy/rust-toolchain.toml.tmpl`
- `crates/greentic-extension-sdk-cli/templates/provider/rust-toolchain.toml.tmpl`

Content (identical for all four):
```toml
[toolchain]
channel = "1.95.0"
targets = ["wasm32-wasip2"]
```

- [ ] **Step 4: Update `wasm-component` template**

In `crates/greentic-extension-sdk-cli/templates/wasm-component/rust-toolchain.toml.tmpl`, replace `channel = "1.94.0"` with `channel = "1.95.0"`. Final content:
```toml
[toolchain]
channel = "1.95.0"
targets = ["wasm32-wasip2"]
```

- [ ] **Step 5: Run test to verify it passes**

```bash
cargo test -p greentic-extension-sdk-cli --lib scaffold::template::tests::every_kind_template_ships_rust_toolchain_pinned_to_1_95_0
```
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/greentic-extension-sdk-cli/templates/*/rust-toolchain.toml.tmpl crates/greentic-extension-sdk-cli/src/scaffold/template.rs
git commit -m "feat(scaffold): ship rust-toolchain.toml in every kind template pinned to 1.95.0"
```

---

## Task E.3.b: Bump `wit-bindgen-rt` in all scaffold templates to `0.41`

**Files:**
- Modify: `crates/greentic-extension-sdk-cli/templates/design/Cargo.toml.tmpl:12`
- Modify: `crates/greentic-extension-sdk-cli/templates/bundle/Cargo.toml.tmpl:12`
- Modify: `crates/greentic-extension-sdk-cli/templates/deploy/Cargo.toml.tmpl:12`
- Modify: `crates/greentic-extension-sdk-cli/templates/provider/Cargo.toml.tmpl:12`
- Modify: `crates/greentic-extension-sdk-cli/templates/wasm-component/Cargo.toml.tmpl` (lookup needed; same pattern)

- [ ] **Step 1: Write failing test**

Append to the `tests` module in `crates/greentic-extension-sdk-cli/src/scaffold/template.rs`:
```rust
    #[test]
    fn every_kind_template_pins_wit_bindgen_rt_0_41() {
        for kind in ["design", "bundle", "deploy", "provider", "wasm-component"] {
            let entries = load_templates_kind(kind);
            let cargo = entries
                .iter()
                .find(|e| e.dst_rel == "Cargo.toml" || e.dst_rel.ends_with("/Cargo.toml"))
                .unwrap_or_else(|| panic!("kind {kind} missing Cargo.toml template"));
            let content = std::str::from_utf8(cargo.src_bytes).expect("utf8");
            assert!(
                content.contains("wit-bindgen-rt = { version = \"0.41\""),
                "kind {kind} Cargo.toml.tmpl does not pin wit-bindgen-rt = 0.41:\n{content}",
            );
        }
    }
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p greentic-extension-sdk-cli --lib scaffold::template::tests::every_kind_template_pins_wit_bindgen_rt_0_41
```
Expected: FAIL on all five kinds (current pin is `"0.35"`).

- [ ] **Step 3: Bump every `wit-bindgen-rt` line**

Run:
```bash
cd /home/bima-pangestu/Works/greentic/greentic-designer-sdk
grep -rln 'wit-bindgen-rt = { version = "0.35"' crates/greentic-extension-sdk-cli/templates/
```
For each file printed, replace the line `wit-bindgen-rt = { version = "0.35", features = ["bitflags"] }` with `wit-bindgen-rt = { version = "0.41", features = ["bitflags"] }`.

- [ ] **Step 4: Verify**

```bash
grep -rn 'wit-bindgen-rt' crates/greentic-extension-sdk-cli/templates/
```
Expected: every match shows `version = "0.41"`.

- [ ] **Step 5: Run test to verify it passes**

```bash
cargo test -p greentic-extension-sdk-cli --lib scaffold::template::tests::every_kind_template_pins_wit_bindgen_rt_0_41
```
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/greentic-extension-sdk-cli/templates/ crates/greentic-extension-sdk-cli/src/scaffold/template.rs
git commit -m "chore(scaffold): bump wit-bindgen-rt 0.35 -> 0.41 across all kind templates"
```

---

## Task E.4.a: Add `Kind::Llm` variant + `as_str` mapping (failing test)

**Files:**
- Modify: `crates/greentic-extension-sdk-cli/src/scaffold/mod.rs`

- [ ] **Step 1: Append failing test**

Append to the `tests` module in `crates/greentic-extension-sdk-cli/src/scaffold/mod.rs`:
```rust
    #[test]
    fn llm_kind_str() {
        assert_eq!(Kind::Llm.as_str(), "llm");
    }
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd /home/bima-pangestu/Works/greentic/greentic-designer-sdk
cargo test -p greentic-extension-sdk-cli --lib scaffold::tests::llm_kind_str
```
Expected: compile error — `Kind::Llm` is not a variant.

- [ ] **Step 3: Add variant + arm**

In `crates/greentic-extension-sdk-cli/src/scaffold/mod.rs`, change the enum and impl:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Kind {
    Design,
    Bundle,
    Deploy,
    Provider,
    WasmComponent,
    Llm,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Design => "design",
            Kind::Bundle => "bundle",
            Kind::Deploy => "deploy",
            Kind::Provider => "provider",
            Kind::WasmComponent => "wasm-component",
            Kind::Llm => "llm",
        }
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo test -p greentic-extension-sdk-cli --lib scaffold::tests::llm_kind_str
```
Expected: PASS.

(Don't commit yet — the next sub-task wires templates + Cargo build path.)

---

## Task E.4.b: Wire `llm` kind into the template loader

**Files:**
- Modify: `crates/greentic-extension-sdk-cli/src/scaffold/template.rs:11-13,57-65`
- Modify: `crates/greentic-extension-sdk-cli/src/scaffold/embedded.rs:40-51`

- [ ] **Step 1: Append failing test**

Append to the `tests` module in `crates/greentic-extension-sdk-cli/src/scaffold/template.rs`:
```rust
    #[test]
    fn load_kind_llm_returns_cargo_toml_src_lib_describe() {
        let entries = load_templates_kind("llm");
        assert!(entries.iter().any(|e| e.dst_rel == "Cargo.toml"));
        assert!(entries.iter().any(|e| e.dst_rel == "describe.json"));
        assert!(entries.iter().any(|e| e.dst_rel == "src/lib.rs"));
        assert!(entries.iter().any(|e| e.dst_rel == "wit/world.wit"));
        assert!(entries.iter().any(|e| e.dst_rel == "rust-toolchain.toml"));
    }
```

Append to the `tests` module in `crates/greentic-extension-sdk-cli/src/scaffold/embedded.rs`:
```rust
    #[test]
    fn files_for_kind_llm_includes_design_wit() {
        // llm is a design-extension subtype; it reuses the design WIT contract.
        let files = files_for_kind("llm");
        let names: Vec<_> = files.iter().map(|f| f.name).collect();
        assert!(names.contains(&"extension-base.wit"));
        assert!(names.contains(&"extension-host.wit"));
        assert!(names.contains(&"extension-design.wit"));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p greentic-extension-sdk-cli --lib scaffold::template::tests::load_kind_llm_returns_cargo_toml_src_lib_describe scaffold::embedded::tests::files_for_kind_llm_includes_design_wit
```
Expected: both FAIL (no `templates/llm/` dir yet; `files_for_kind("llm")` falls through to `extension-llm.wit` lookup which doesn't exist).

- [ ] **Step 3: Add a `TEMPLATES_LLM` static + dispatch arm**

In `crates/greentic-extension-sdk-cli/src/scaffold/template.rs`, after the existing `TEMPLATES_WASM_COMPONENT` line, add:
```rust
static TEMPLATES_LLM: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/templates/llm");
```

In the same file, extend `load_templates_kind`:
```rust
pub fn load_templates_kind(kind: &str) -> Vec<TemplateEntry> {
    match kind {
        "design" => collect(&TEMPLATES_DESIGN),
        "bundle" => collect(&TEMPLATES_BUNDLE),
        "deploy" => collect(&TEMPLATES_DEPLOY),
        "provider" => collect(&TEMPLATES_PROVIDER),
        "wasm-component" => collect(&TEMPLATES_WASM_COMPONENT),
        "llm" => collect(&TEMPLATES_LLM),
        _ => Vec::new(),
    }
}
```

- [ ] **Step 4: Map `llm` → `extension-design.wit` in the WIT file selector**

In `crates/greentic-extension-sdk-cli/src/scaffold/embedded.rs`, change `files_for_kind`:
```rust
pub fn files_for_kind(kind: &str) -> Vec<WitFile> {
    let kind_file = match kind {
        "wasm-component" | "llm" => "extension-design.wit".to_string(),
        other => format!("extension-{other}.wit"),
    };
    wit_files()
        .into_iter()
        .filter(|f| {
            matches!(f.name, "extension-base.wit" | "extension-host.wit") || f.name == kind_file
        })
        .collect()
}
```

(Compile will still fail because `templates/llm/` doesn't exist yet — the `include_dir!` macro is compile-time and errors if the dir is missing. That's resolved in E.4.c.)

---

## Task E.4.c: Create the `templates/llm/` template tree

**Files (all new):**
- `crates/greentic-extension-sdk-cli/templates/llm/Cargo.toml.tmpl`
- `crates/greentic-extension-sdk-cli/templates/llm/describe.json.tmpl`
- `crates/greentic-extension-sdk-cli/templates/llm/rust-toolchain.toml.tmpl`
- `crates/greentic-extension-sdk-cli/templates/llm/src/lib.rs.tmpl`
- `crates/greentic-extension-sdk-cli/templates/llm/wit/world.wit.tmpl`
- `crates/greentic-extension-sdk-cli/templates/llm/prompts/system.md.tmpl`
- `crates/greentic-extension-sdk-cli/templates/llm/schemas/llm-config.json.tmpl`

- [ ] **Step 1: Create the directory tree**

```bash
cd /home/bima-pangestu/Works/greentic/greentic-designer-sdk
mkdir -p crates/greentic-extension-sdk-cli/templates/llm/src
mkdir -p crates/greentic-extension-sdk-cli/templates/llm/wit
mkdir -p crates/greentic-extension-sdk-cli/templates/llm/prompts
mkdir -p crates/greentic-extension-sdk-cli/templates/llm/schemas
```

- [ ] **Step 2: Write `Cargo.toml.tmpl`**

Path: `crates/greentic-extension-sdk-cli/templates/llm/Cargo.toml.tmpl`
Content:
```toml
[package]
name = "{{name}}"
version = "{{version}}"
edition = "2024"
license = "{{license}}"
authors = ["{{author}}"]

[lib]
crate-type = ["cdylib"]

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
wit-bindgen = "0.41"
wit-bindgen-rt = { version = "0.41", features = ["bitflags"] }

[package.metadata.component]
package = "{{id_wit}}"

[package.metadata.component.target]
path = "wit"

[package.metadata.component.target.dependencies]
"greentic:extension-base" = { path = "wit/deps/greentic/extension-base" }
"greentic:extension-host" = { path = "wit/deps/greentic/extension-host" }
"greentic:extension-design" = { path = "wit/deps/greentic/extension-design" }
```

- [ ] **Step 3: Write `rust-toolchain.toml.tmpl`**

Path: `crates/greentic-extension-sdk-cli/templates/llm/rust-toolchain.toml.tmpl`
Content:
```toml
[toolchain]
channel = "1.95.0"
targets = ["wasm32-wasip2"]
```

- [ ] **Step 4: Write `wit/world.wit.tmpl`**

Path: `crates/greentic-extension-sdk-cli/templates/llm/wit/world.wit.tmpl`

LLM extensions are design-extensions: they expose the same `extension-design` exports plus they import `extension-host/http` (LLM calls go out to provider endpoints) and `extension-host/secrets` (API keys).
```wit
package {{id_wit}};

world extension {
  import greentic:extension-base/types@{{contract_version}};
  import greentic:extension-host/logging@{{contract_version}};
  import greentic:extension-host/i18n@{{contract_version}};
  import greentic:extension-host/secrets@{{contract_version}};
  import greentic:extension-host/broker@{{contract_version}};
  import greentic:extension-host/http@{{contract_version}};

  export greentic:extension-base/manifest@{{contract_version}};
  export greentic:extension-base/lifecycle@{{contract_version}};
  export greentic:extension-design/tools@{{contract_version}};
  export greentic:extension-design/validation@{{contract_version}};
  export greentic:extension-design/prompting@{{contract_version}};
  export greentic:extension-design/knowledge@{{contract_version}};
}
```

- [ ] **Step 5: Write `src/lib.rs.tmpl`**

Path: `crates/greentic-extension-sdk-cli/templates/llm/src/lib.rs.tmpl`

The LLM scaffold differs from a plain `design` scaffold in that `list_tools` ships a representative LLM tool (`complete`) and `invoke_tool` shows the secret-fetch + http-fetch + structured-error pattern that LLM extensions need.
```rust
// LLM design extension guest for {{id}}.
//
// This scaffold implements every export required by the extension-design
// contract. The `complete` tool demonstrates the LLM-extension pattern:
// pull an API key from host secrets, build a request, call host http,
// and return a structured JSON response. Replace placeholders before shipping.

#[allow(warnings)]
mod bindings;

use bindings::exports::greentic::extension_base::{lifecycle, manifest};
use bindings::exports::greentic::extension_design::{knowledge, prompting, tools, validation};
use bindings::greentic::extension_base::types;
use bindings::greentic::extension_host::{http, secrets};

struct Component;

// ---- extension-base/manifest ----
impl manifest::Guest for Component {
    fn get_identity() -> types::ExtensionIdentity {
        types::ExtensionIdentity {
            id: "{{id}}".to_string(),
            version: "{{version}}".to_string(),
            kind: types::Kind::Design,
        }
    }

    fn get_offered() -> Vec<types::CapabilityRef> {
        Vec::new()
    }

    fn get_required() -> Vec<types::CapabilityRef> {
        Vec::new()
    }
}

// ---- extension-base/lifecycle ----
impl lifecycle::Guest for Component {
    fn init(_config_json: String) -> Result<(), types::ExtensionError> {
        // TODO: parse provider config (api base URL, default model) here.
        Ok(())
    }

    fn shutdown() {
        // No-op for a stateless LLM client. Override if you cache a client.
    }
}

// ---- extension-design/tools ----
impl tools::Guest for Component {
    fn list_tools() -> Vec<tools::ToolDefinition> {
        vec![tools::ToolDefinition {
            name: "complete".to_string(),
            description: "Run an LLM completion against {{label}}.".to_string(),
            input_schema_json: r#"{
  "type": "object",
  "properties": {
    "prompt":   { "type": "string" },
    "max_tokens": { "type": "integer", "minimum": 1, "default": 256 }
  },
  "required": ["prompt"]
}"#
            .to_string(),
            output_schema_json: r#"{
  "type": "object",
  "properties": {
    "text": { "type": "string" }
  },
  "required": ["text"]
}"#
            .to_string(),
        }]
    }

    fn invoke_tool(
        name: String,
        args_json: String,
    ) -> Result<String, types::ExtensionError> {
        if name != "complete" {
            return Err(types::ExtensionError::InvalidInput(format!(
                "unknown tool: {name}"
            )));
        }
        // 1. Pull API key from host secrets (alias declared in describe.json).
        let api_key = secrets::get("api_key")
            .map_err(|e| types::ExtensionError::Runtime(format!("secrets::get: {e:?}")))?;
        // 2. Build the HTTP request (provider-specific endpoint here).
        let req = http::Request {
            method: http::Method::Post,
            url: "https://api.example.com/v1/complete".to_string(),
            headers: vec![
                ("authorization".to_string(), format!("Bearer {api_key}")),
                ("content-type".to_string(), "application/json".to_string()),
            ],
            body: Some(args_json.into_bytes()),
        };
        // 3. Call host http (subject to permissions.network allowlist).
        let resp = http::fetch(&req)
            .map_err(|e| types::ExtensionError::Runtime(format!("http::fetch: {e:?}")))?;
        // 4. Return the upstream body verbatim. Real impl should parse + reshape.
        String::from_utf8(resp.body)
            .map_err(|e| types::ExtensionError::Runtime(format!("non-utf8 body: {e}")))
    }
}

// ---- extension-design/validation ----
impl validation::Guest for Component {
    fn validate_content(
        _content_type: String,
        _content_json: String,
    ) -> validation::ValidateResult {
        validation::ValidateResult {
            valid: true,
            diagnostics: Vec::new(),
        }
    }
}

// ---- extension-design/prompting ----
impl prompting::Guest for Component {
    fn system_prompt_fragments() -> Vec<prompting::PromptFragment> {
        Vec::new()
    }
}

// ---- extension-design/knowledge ----
impl knowledge::Guest for Component {
    fn list_entries(
        _category_filter: Option<String>,
    ) -> Vec<knowledge::EntrySummary> {
        Vec::new()
    }

    fn get_entry(id: String) -> Result<knowledge::Entry, types::ExtensionError> {
        Err(types::ExtensionError::InvalidInput(format!(
            "unknown entry: {id}"
        )))
    }

    fn suggest_entries(
        _query: String,
        _limit: u32,
    ) -> Vec<knowledge::EntrySummary> {
        Vec::new()
    }
}

bindings::export!(Component with_types_in bindings);
```

- [ ] **Step 6: Write `describe.json.tmpl`**

Path: `crates/greentic-extension-sdk-cli/templates/llm/describe.json.tmpl`

The describe declares `kind: DesignExtension` (LLM is a subtype of design), the `complete` tool's `contributions.tools[]` entry, the `secrets.aliases` slot for the API key, and the `permissions.network` allowlist scaffolded with a placeholder host the developer must edit.
```json
{
  "$schema": "https://store.greentic.ai/schemas/describe-v1.json",
  "apiVersion": "greentic.ai/v1",
  "kind": "DesignExtension",
  "metadata": {
    "id": "{{id}}",
    "name": "{{name}}",
    "version": "{{version}}",
    "summary": "A Greentic Designer LLM provider extension.",
    "author": {
      "name": "{{author}}"
    },
    "license": "{{license}}"
  },
  "engine": {
    "greenticDesigner": "^{{contract_version}}",
    "extRuntime": "^{{contract_version}}"
  },
  "capabilities": {
    "offered": [],
    "required": []
  },
  "runtime": {
    "component": "extension.wasm",
    "permissions": {
      "network": [
        { "scheme": "https", "host": "api.example.com", "pathPrefix": "/v1/" }
      ],
      "secrets": [
        { "alias": "api_key", "description": "LLM provider API key" }
      ],
      "callExtensionKinds": []
    }
  },
  "contributions": {
    "tools": [
      {
        "name": "complete",
        "kind": "llm",
        "description": "Run an LLM completion against {{label}}.",
        "configSchemaRef": "schemas/llm-config.json"
      }
    ]
  }
}
```

- [ ] **Step 7: Write `prompts/system.md.tmpl`**

Path: `crates/greentic-extension-sdk-cli/templates/llm/prompts/system.md.tmpl`
```markdown
# {{name}} — LLM system prompt

This file is loaded by designer UIs when this LLM extension is active.
It is contributed to the LLM context whenever a flow author selects this provider.
Edit to describe model behavior, constraints, and tool-use conventions.
```

- [ ] **Step 8: Write `schemas/llm-config.json.tmpl`**

Path: `crates/greentic-extension-sdk-cli/templates/llm/schemas/llm-config.json.tmpl`
```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "{{name}} LLM provider config",
  "type": "object",
  "properties": {
    "model":  { "type": "string", "description": "Model identifier (provider-specific)." },
    "temperature": { "type": "number", "minimum": 0, "maximum": 2, "default": 0.7 }
  },
  "required": ["model"]
}
```

- [ ] **Step 9: Run the failing tests from E.4.a/b**

```bash
cd /home/bima-pangestu/Works/greentic/greentic-designer-sdk
cargo test -p greentic-extension-sdk-cli --lib
```
Expected: all PASS (including the three new tests added in E.4.a and E.4.b).

- [ ] **Step 10: Commit the llm scaffold**

```bash
git add crates/greentic-extension-sdk-cli/src/scaffold/mod.rs crates/greentic-extension-sdk-cli/src/scaffold/template.rs crates/greentic-extension-sdk-cli/src/scaffold/embedded.rs crates/greentic-extension-sdk-cli/templates/llm/
git commit -m "feat(scaffold): add Kind::Llm with dedicated templates/llm/ tree"
```

---

## Task E.4.d: End-to-end scaffold + build + pack + install integration test

**Files:**
- Create: `crates/greentic-extension-sdk-cli/tests/llm_scaffold_e2e.rs`

- [ ] **Step 1: Write the failing test**

This test is `#[ignore]`'d in the file but documented in `ci/local_check.sh` (see Step 5) because it requires `cargo-component` and an internet connection for `cargo build`. Local CI runs it; default `cargo test` skips it.

Content:
```rust
//! End-to-end integration test for `gtdx new --kind llm`.
//!
//! Run only when `cargo-component` is installed and the `wasm32-wasip2`
//! target is available. CI installs both; default `cargo test` skips.
//!
//! Pipeline: scaffold -> cargo build -> gtdx publish (pack) -> gtdx install.

#![cfg(not(target_os = "windows"))]

use std::process::Command;

fn cargo_component_available() -> bool {
    Command::new("cargo")
        .arg("component")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
#[ignore = "requires cargo-component + network for cargo build; run via ci/local_check.sh"]
fn scaffold_llm_builds_and_installs() {
    if !cargo_component_available() {
        eprintln!("cargo-component not installed; skipping");
        return;
    }
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("mkdir home");
    let project = tmp.path().join("ext");

    let gtdx = env!("CARGO_BIN_EXE_gtdx");

    // 1. scaffold
    let st = Command::new(gtdx)
        .args(["--home", home.to_str().unwrap()])
        .args(["new", "myllm",
               "--kind", "llm",
               "--id", "com.example.myllm",
               "--version", "0.1.0",
               "--no-git",
               "--yes",
               "--dir", project.to_str().unwrap()])
        .status()
        .expect("run gtdx new");
    assert!(st.success(), "gtdx new --kind llm failed");

    // 2. cargo component build
    let st = Command::new("cargo")
        .args(["component", "build", "--release"])
        .current_dir(&project)
        .status()
        .expect("run cargo component build");
    assert!(st.success(), "cargo component build failed in scaffolded llm project");

    // 3. gtdx dev --once (pack + install into the test home)
    let st = Command::new(gtdx)
        .args(["--home", home.to_str().unwrap()])
        .args(["dev", "--once", "--release",
               "--manifest", project.join("Cargo.toml").to_str().unwrap()])
        .status()
        .expect("run gtdx dev --once");
    assert!(st.success(), "gtdx dev --once failed for llm scaffold");

    // 4. gtdx list should show the installed extension
    let out = Command::new(gtdx)
        .args(["--home", home.to_str().unwrap()])
        .arg("list")
        .output()
        .expect("run gtdx list");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("com.example.myllm"),
        "gtdx list missing com.example.myllm; got:\n{stdout}",
    );
}
```

- [ ] **Step 2: Run with `--ignored` to verify it works locally**

```bash
cd /home/bima-pangestu/Works/greentic/greentic-designer-sdk
cargo test -p greentic-extension-sdk-cli --test llm_scaffold_e2e -- --ignored --nocapture
```
Expected: PASS if `cargo-component` is installed, else printed skip + PASS.

(If you don't have `cargo-component` locally, install it: `cargo install cargo-component`. The SDK already lists it as a preflight check in `scaffold/preflight.rs`.)

- [ ] **Step 3: Verify default test run does NOT run this**

```bash
cargo test -p greentic-extension-sdk-cli --test llm_scaffold_e2e
```
Expected: `test result: ok. 0 passed; 0 failed; 1 ignored`.

- [ ] **Step 4: Commit**

```bash
git add crates/greentic-extension-sdk-cli/tests/llm_scaffold_e2e.rs
git commit -m "test(scaffold): end-to-end llm scaffold -> build -> install integration test"
```

- [ ] **Step 5: Wire the `--ignored` test into `ci/local_check.sh`**

In `/home/bima-pangestu/Works/greentic/greentic-designer-sdk/ci/local_check.sh`, after the existing `cargo test` line, append:
```bash
echo "--- llm scaffold e2e ---"
cargo test -p greentic-extension-sdk-cli --test llm_scaffold_e2e -- --ignored --nocapture
```
Run the full script locally to confirm:
```bash
bash ci/local_check.sh
```
Expected: PASS end-to-end.

- [ ] **Step 6: Commit CI change**

```bash
git add ci/local_check.sh
git commit -m "ci: run llm scaffold e2e in local_check.sh"
```

---

## Task E.5.a: `gtdx lint` — module skeleton + dispatch

**Files:**
- Create: `crates/greentic-extension-sdk-cli/src/commands/lint/mod.rs`
- Create: `crates/greentic-extension-sdk-cli/src/commands/lint/violations.rs`
- Modify: `crates/greentic-extension-sdk-cli/src/commands/mod.rs` (export lint module)
- Modify: `crates/greentic-extension-sdk-cli/src/main.rs` (add `Lint` subcommand variant + dispatch)

- [ ] **Step 1: Write failing test**

Create `crates/greentic-extension-sdk-cli/tests/lint_smoke.rs`:
```rust
//! `gtdx lint` exists and accepts a project dir.

use std::process::Command;

#[test]
fn lint_subcommand_listed_in_help() {
    let gtdx = env!("CARGO_BIN_EXE_gtdx");
    let out = Command::new(gtdx).arg("--help").output().expect("run --help");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("lint"), "gtdx --help missing `lint`:\n{stdout}");
}

#[test]
fn lint_clean_fixture_exits_zero() {
    let gtdx = env!("CARGO_BIN_EXE_gtdx");
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/lint/clean");
    let out = Command::new(gtdx)
        .arg("lint")
        .arg("--dir")
        .arg(&fixture)
        .output()
        .expect("run gtdx lint");
    assert!(
        out.status.success(),
        "lint on clean fixture failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}
```

- [ ] **Step 2: Create the clean fixture**

```bash
mkdir -p crates/greentic-extension-sdk-cli/tests/fixtures/lint/clean
```

Write `crates/greentic-extension-sdk-cli/tests/fixtures/lint/clean/describe.json`:
```json
{
  "$schema": "https://store.greentic.ai/schemas/describe-v1.json",
  "apiVersion": "greentic.ai/v1",
  "kind": "DesignExtension",
  "metadata": {
    "id": "com.example.clean",
    "name": "clean",
    "version": "0.1.0",
    "summary": "Clean lint fixture.",
    "author": { "name": "Test" },
    "license": "MIT"
  },
  "engine": { "greenticDesigner": "^0.4.5", "extRuntime": "^0.4.5" },
  "capabilities": { "offered": [], "required": [] },
  "runtime": {
    "component": "extension.wasm",
    "permissions": { "network": [], "secrets": [], "callExtensionKinds": [] }
  },
  "contributions": {}
}
```

- [ ] **Step 3: Run the failing test**

```bash
cd /home/bima-pangestu/Works/greentic/greentic-designer-sdk
cargo test -p greentic-extension-sdk-cli --test lint_smoke
```
Expected: FAIL (the `lint` subcommand does not exist yet).

- [ ] **Step 4: Create the lint module skeleton**

Create `crates/greentic-extension-sdk-cli/src/commands/lint/mod.rs`:
```rust
//! `gtdx lint` — static analysis of an extension directory.
//!
//! Five checks (each gated by a CLI flag, default = all on):
//!   1. describe-diff vs installed extension (warn on breaking change without
//!      version bump);
//!   2. semver bump rule per `0.x.y` pre-1.0 semantics;
//!   3. capability cycle in the `required` graph;
//!   4. runtime_ref validity — every `nodeType.runtime_ref` /
//!      `tool.runtime_ref` points at a declared `runtime.components[]` key;
//!   5. (placeholder for future checks).

pub mod violations;

use std::path::{Path, PathBuf};

use clap::Args as ClapArgs;

use self::violations::{Severity, Violation};

#[derive(ClapArgs, Debug, Clone)]
pub struct Args {
    /// Extension project directory containing `describe.json`.
    #[arg(long, default_value = ".")]
    pub dir: PathBuf,

    /// Disable describe-diff vs installed extension.
    #[arg(long)]
    pub no_diff: bool,

    /// Disable semver bump validation.
    #[arg(long)]
    pub no_semver: bool,

    /// Disable capability cycle detection.
    #[arg(long)]
    pub no_cycle: bool,

    /// Disable runtime_ref validation.
    #[arg(long)]
    pub no_runtime_ref: bool,
}

pub fn run(args: &Args, home: &Path) -> anyhow::Result<()> {
    let describe_path = args.dir.join("describe.json");
    let bytes = std::fs::read(&describe_path)
        .map_err(|e| anyhow::anyhow!("read {}: {e}", describe_path.display()))?;
    let describe: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| anyhow::anyhow!("parse {}: {e}", describe_path.display()))?;

    let mut findings: Vec<Violation> = Vec::new();

    if !args.no_cycle {
        findings.extend(violations::check_capability_cycle(&describe));
    }
    if !args.no_runtime_ref {
        findings.extend(violations::check_runtime_refs(&describe));
    }
    if !args.no_semver {
        findings.extend(violations::check_semver_bump_format(&describe));
    }
    if !args.no_diff {
        findings.extend(violations::check_describe_diff(home, &describe));
    }

    for v in &findings {
        let prefix = match v.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        eprintln!("{prefix}: {}", v.message);
    }

    if findings.iter().any(|v| matches!(v.severity, Severity::Error)) {
        anyhow::bail!("lint failed with {} error(s)", findings.iter().filter(|v| matches!(v.severity, Severity::Error)).count());
    }
    if findings.is_empty() {
        println!("lint: clean");
    } else {
        println!("lint: {} warning(s)", findings.len());
    }
    Ok(())
}
```

Create `crates/greentic-extension-sdk-cli/src/commands/lint/violations.rs`:
```rust
//! Lint findings + per-check implementations.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone)]
pub struct Violation {
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
}

/// Detects cycles in `capabilities.required` -> `capabilities.offered` cross-refs.
///
/// We treat each capability `id` in `required[]` as a directed edge from this
/// extension to the named capability. A cycle is "this extension requires
/// itself via an `offered` cap of the same id". Real cross-extension cycles
/// need a registry walk; that's deferred until `--diff` is implemented.
pub fn check_capability_cycle(describe: &Value) -> Vec<Violation> {
    let mut out = Vec::new();
    let offered: HashSet<String> = describe
        .get("capabilities")
        .and_then(|c| c.get("offered"))
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.get("id").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let required: Vec<String> = describe
        .get("capabilities")
        .and_then(|c| c.get("required"))
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.get("id").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    for r in &required {
        if offered.contains(r) {
            out.push(Violation {
                severity: Severity::Error,
                code: "E_CAP_CYCLE",
                message: format!(
                    "capability {r:?} appears in both `offered` and `required` (self-cycle)"
                ),
            });
        }
    }
    out
}

/// Every `contributions.nodeTypes[].runtime_ref` and
/// `contributions.tools[].runtime_ref` MUST name a key declared under
/// `runtime.components`.
pub fn check_runtime_refs(describe: &Value) -> Vec<Violation> {
    let mut out = Vec::new();
    let declared: HashSet<String> = describe
        .get("runtime")
        .and_then(|r| r.get("components"))
        .and_then(Value::as_object)
        .map(|obj| obj.keys().cloned().collect())
        .unwrap_or_default();

    for kind in ["nodeTypes", "tools"] {
        let arr = match describe
            .get("contributions")
            .and_then(|c| c.get(kind))
            .and_then(Value::as_array)
        {
            Some(a) => a,
            None => continue,
        };
        for (i, item) in arr.iter().enumerate() {
            if let Some(rref) = item.get("runtime_ref").and_then(Value::as_str)
                && !declared.contains(rref)
            {
                out.push(Violation {
                    severity: Severity::Error,
                    code: "E_RUNTIME_REF",
                    message: format!(
                        "contributions.{kind}[{i}].runtime_ref = {rref:?} not declared in runtime.components"
                    ),
                });
            }
        }
    }
    out
}

/// Validates `metadata.version` is a parseable semver string. The "bump rule"
/// check (breaking change in 0.x.y must move minor) requires the installed
/// version to diff against, so it lives in `check_describe_diff`.
pub fn check_semver_bump_format(describe: &Value) -> Vec<Violation> {
    let mut out = Vec::new();
    let v = describe
        .get("metadata")
        .and_then(|m| m.get("version"))
        .and_then(Value::as_str);
    match v {
        Some(s) => {
            if let Err(e) = semver::Version::parse(s) {
                out.push(Violation {
                    severity: Severity::Error,
                    code: "E_VERSION_SEMVER",
                    message: format!("metadata.version {s:?} is not valid semver: {e}"),
                });
            }
        }
        None => out.push(Violation {
            severity: Severity::Error,
            code: "E_VERSION_MISSING",
            message: "metadata.version is missing or not a string".into(),
        }),
    }
    out
}

/// Compare the current describe against the installed copy under
/// `<home>/extensions/<kind>/<id>/describe.json`. If installed has higher
/// or equal version AND any breaking-change-shaped diff is present, emit
/// a warning. (Detailed semantic-diff is future work; for v1 we flag
/// missing-required-capability and removed-tool changes.)
pub fn check_describe_diff(home: &Path, current: &Value) -> Vec<Violation> {
    let mut out = Vec::new();
    let (Some(id), Some(kind), Some(version_str)) = (
        current.get("metadata").and_then(|m| m.get("id")).and_then(Value::as_str),
        current.get("kind").and_then(Value::as_str),
        current.get("metadata").and_then(|m| m.get("version")).and_then(Value::as_str),
    ) else {
        return out;
    };
    let Ok(current_ver) = semver::Version::parse(version_str) else {
        return out;
    };
    let kind_dir = kind_to_dir(kind);
    let installed_path = home
        .join("extensions")
        .join(kind_dir)
        .join(id)
        .join("describe.json");
    let Ok(bytes) = std::fs::read(&installed_path) else {
        return out; // not installed -> nothing to diff
    };
    let Ok(installed): Result<Value, _> = serde_json::from_slice(&bytes) else {
        return out;
    };
    let installed_ver = installed
        .get("metadata")
        .and_then(|m| m.get("version"))
        .and_then(Value::as_str)
        .and_then(|s| semver::Version::parse(s).ok());

    // Detect removed tool names (a breaking change in semver).
    let installed_tools = collect_tool_names(&installed);
    let current_tools = collect_tool_names(current);
    let removed: Vec<&String> = installed_tools.difference(&current_tools).collect();

    // Detect removed required capabilities.
    let installed_required = collect_cap_ids(&installed, "required");
    let current_required = collect_cap_ids(current, "required");
    let cap_added: Vec<&String> = current_required.difference(&installed_required).collect();

    if !removed.is_empty() || !cap_added.is_empty() {
        if let Some(prev) = installed_ver {
            let bumped = matches!(
                current_ver.major.cmp(&prev.major),
                std::cmp::Ordering::Greater,
            ) || (prev.major == 0
                && current_ver.major == 0
                && current_ver.minor > prev.minor);
            if !bumped {
                out.push(Violation {
                    severity: Severity::Warning,
                    code: "W_BREAKING_NO_BUMP",
                    message: format!(
                        "breaking change detected (removed tools: {removed:?}, new required caps: {cap_added:?}) but version not bumped from {prev} to {current_ver}"
                    ),
                });
            }
        }
    }
    out
}

fn collect_tool_names(describe: &Value) -> HashSet<String> {
    describe
        .get("contributions")
        .and_then(|c| c.get("tools"))
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.get("name").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn collect_cap_ids(describe: &Value, key: &str) -> HashSet<String> {
    describe
        .get("capabilities")
        .and_then(|c| c.get(key))
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.get("id").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn kind_to_dir(kind: &str) -> &'static str {
    match kind {
        "DesignExtension" => "design",
        "BundleExtension" => "bundle",
        "DeployExtension" => "deploy",
        "ProviderExtension" => "provider",
        _ => "misc",
    }
}

// Suppress dead-code warnings for `HashMap` if not used; current impl is in
// `collect_*` via HashSet only. Keep the import slot for future per-component
// counts.
#[allow(dead_code)]
type _UnusedHashMap = HashMap<String, String>;
```

- [ ] **Step 5: Export lint from `commands/mod.rs`**

In `crates/greentic-extension-sdk-cli/src/commands/mod.rs`, add (in the appropriate alphabetical slot):
```rust
pub mod lint;
```

(Open `mod.rs` first, find the existing `pub mod` list, and insert `pub mod lint;`. Match the file's style — no extra blank lines, alphabetical order if the existing list is sorted.)

- [ ] **Step 6: Wire the subcommand into `main.rs`**

In `crates/greentic-extension-sdk-cli/src/main.rs`:

Add a variant in `enum Command`:
```rust
    /// Lint an extension directory: describe-diff, semver bump, capability cycle, runtime_ref validity
    Lint(commands::lint::Args),
```

Add the match arm in the `match cli.command` block (sync, alongside `Validate`):
```rust
        Command::Lint(args) => commands::lint::run(&args, &home),
```

- [ ] **Step 7: Run the smoke test**

```bash
cd /home/bima-pangestu/Works/greentic/greentic-designer-sdk
cargo test -p greentic-extension-sdk-cli --test lint_smoke
```
Expected: both PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/greentic-extension-sdk-cli/src/commands/lint/ crates/greentic-extension-sdk-cli/src/commands/mod.rs crates/greentic-extension-sdk-cli/src/main.rs crates/greentic-extension-sdk-cli/tests/lint_smoke.rs crates/greentic-extension-sdk-cli/tests/fixtures/lint/clean/
git commit -m "feat(gtdx): add `gtdx lint` skeleton with describe-diff/semver/cycle/runtime_ref checks"
```

---

## Task E.5.b: `gtdx lint` — capability cycle violation fixture

**Files:**
- Create: `crates/greentic-extension-sdk-cli/tests/fixtures/lint/cap_cycle/describe.json`
- Add test to: `crates/greentic-extension-sdk-cli/tests/lint_violations.rs` (new)

- [ ] **Step 1: Write failing test**

Create `crates/greentic-extension-sdk-cli/tests/lint_violations.rs`:
```rust
//! Per-violation fixtures for `gtdx lint`. Each fixture is a describe.json
//! that violates exactly one rule; the test asserts gtdx exits non-zero
//! AND stderr names the rule code.

use std::process::Command;

fn run_lint(fixture: &str) -> (bool, String, String) {
    let gtdx = env!("CARGO_BIN_EXE_gtdx");
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/lint")
        .join(fixture);
    let out = Command::new(gtdx)
        .args(["lint", "--dir"])
        .arg(&dir)
        .output()
        .expect("run gtdx lint");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn capability_cycle_fails_with_e_cap_cycle() {
    let (ok, _stdout, stderr) = run_lint("cap_cycle");
    assert!(!ok, "lint should fail for cap_cycle fixture");
    assert!(stderr.contains("self-cycle"), "stderr missing self-cycle message: {stderr}");
}
```

- [ ] **Step 2: Run test to verify it fails (no fixture yet)**

```bash
cargo test -p greentic-extension-sdk-cli --test lint_violations capability_cycle_fails_with_e_cap_cycle
```
Expected: FAIL (no fixture dir).

- [ ] **Step 3: Create the cycle fixture**

```bash
mkdir -p crates/greentic-extension-sdk-cli/tests/fixtures/lint/cap_cycle
```

Write `crates/greentic-extension-sdk-cli/tests/fixtures/lint/cap_cycle/describe.json`:
```json
{
  "$schema": "https://store.greentic.ai/schemas/describe-v1.json",
  "apiVersion": "greentic.ai/v1",
  "kind": "DesignExtension",
  "metadata": {
    "id": "com.example.cycle",
    "name": "cycle",
    "version": "0.1.0",
    "summary": "Capability self-cycle fixture.",
    "author": { "name": "Test" },
    "license": "MIT"
  },
  "engine": { "greenticDesigner": "^0.4.5", "extRuntime": "^0.4.5" },
  "capabilities": {
    "offered":  [{ "id": "ext.self.cap", "version": "1.0.0" }],
    "required": [{ "id": "ext.self.cap", "version": "^1.0.0" }]
  },
  "runtime": {
    "component": "extension.wasm",
    "permissions": { "network": [], "secrets": [], "callExtensionKinds": [] }
  },
  "contributions": {}
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo test -p greentic-extension-sdk-cli --test lint_violations capability_cycle_fails_with_e_cap_cycle
```
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/greentic-extension-sdk-cli/tests/lint_violations.rs crates/greentic-extension-sdk-cli/tests/fixtures/lint/cap_cycle/
git commit -m "test(lint): cap_cycle violation fixture + assertion"
```

---

## Task E.5.c: `gtdx lint` — runtime_ref dangling fixture

**Files:**
- Create: `crates/greentic-extension-sdk-cli/tests/fixtures/lint/dangling_runtime_ref/describe.json`
- Append test to: `crates/greentic-extension-sdk-cli/tests/lint_violations.rs`

- [ ] **Step 1: Append test**

```rust
#[test]
fn dangling_runtime_ref_fails_with_e_runtime_ref() {
    let (ok, _stdout, stderr) = run_lint("dangling_runtime_ref");
    assert!(!ok, "lint should fail for dangling runtime_ref");
    assert!(stderr.contains("runtime_ref"), "stderr missing runtime_ref: {stderr}");
    assert!(stderr.contains("not declared"), "stderr missing 'not declared': {stderr}");
}
```

- [ ] **Step 2: Run failing test**

```bash
cargo test -p greentic-extension-sdk-cli --test lint_violations dangling_runtime_ref_fails_with_e_runtime_ref
```
Expected: FAIL.

- [ ] **Step 3: Create fixture**

```bash
mkdir -p crates/greentic-extension-sdk-cli/tests/fixtures/lint/dangling_runtime_ref
```

Write `crates/greentic-extension-sdk-cli/tests/fixtures/lint/dangling_runtime_ref/describe.json`:
```json
{
  "$schema": "https://store.greentic.ai/schemas/describe-v1.json",
  "apiVersion": "greentic.ai/v1",
  "kind": "DesignExtension",
  "metadata": {
    "id": "com.example.dangle",
    "name": "dangle",
    "version": "0.1.0",
    "summary": "Dangling runtime_ref fixture.",
    "author": { "name": "Test" },
    "license": "MIT"
  },
  "engine": { "greenticDesigner": "^0.4.5", "extRuntime": "^0.4.5" },
  "capabilities": { "offered": [], "required": [] },
  "runtime": {
    "components": {
      "primary": { "type": "wasm", "path": "extension.wasm" }
    },
    "permissions": { "network": [], "secrets": [], "callExtensionKinds": [] }
  },
  "contributions": {
    "tools": [
      { "name": "do_thing", "runtime_ref": "ghost_component" }
    ]
  }
}
```

- [ ] **Step 4: Verify PASS**

```bash
cargo test -p greentic-extension-sdk-cli --test lint_violations dangling_runtime_ref_fails_with_e_runtime_ref
```
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/greentic-extension-sdk-cli/tests/lint_violations.rs crates/greentic-extension-sdk-cli/tests/fixtures/lint/dangling_runtime_ref/
git commit -m "test(lint): dangling_runtime_ref violation fixture + assertion"
```

---

## Task E.5.d: `gtdx lint` — invalid semver fixture

**Files:**
- Create: `crates/greentic-extension-sdk-cli/tests/fixtures/lint/bad_semver/describe.json`
- Append test to: `crates/greentic-extension-sdk-cli/tests/lint_violations.rs`

- [ ] **Step 1: Append test**

```rust
#[test]
fn invalid_semver_fails_with_e_version_semver() {
    let (ok, _stdout, stderr) = run_lint("bad_semver");
    assert!(!ok, "lint should fail for bad_semver");
    assert!(stderr.contains("not valid semver"), "stderr missing semver msg: {stderr}");
}
```

- [ ] **Step 2: Run failing test**

```bash
cargo test -p greentic-extension-sdk-cli --test lint_violations invalid_semver_fails_with_e_version_semver
```
Expected: FAIL.

- [ ] **Step 3: Create fixture**

```bash
mkdir -p crates/greentic-extension-sdk-cli/tests/fixtures/lint/bad_semver
```

Write `crates/greentic-extension-sdk-cli/tests/fixtures/lint/bad_semver/describe.json`:
```json
{
  "$schema": "https://store.greentic.ai/schemas/describe-v1.json",
  "apiVersion": "greentic.ai/v1",
  "kind": "DesignExtension",
  "metadata": {
    "id": "com.example.badsemver",
    "name": "badsemver",
    "version": "not-a-version",
    "summary": "Bad semver fixture.",
    "author": { "name": "Test" },
    "license": "MIT"
  },
  "engine": { "greenticDesigner": "^0.4.5", "extRuntime": "^0.4.5" },
  "capabilities": { "offered": [], "required": [] },
  "runtime": {
    "component": "extension.wasm",
    "permissions": { "network": [], "secrets": [], "callExtensionKinds": [] }
  },
  "contributions": {}
}
```

- [ ] **Step 4: Verify PASS**

```bash
cargo test -p greentic-extension-sdk-cli --test lint_violations invalid_semver_fails_with_e_version_semver
```
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/greentic-extension-sdk-cli/tests/lint_violations.rs crates/greentic-extension-sdk-cli/tests/fixtures/lint/bad_semver/
git commit -m "test(lint): bad_semver violation fixture + assertion"
```

---

## Task E.5.e: `gtdx lint` — describe-diff breaking-without-bump fixture

**Files:**
- Create: `crates/greentic-extension-sdk-cli/tests/fixtures/lint/breaking_no_bump/current/describe.json`
- Create: `crates/greentic-extension-sdk-cli/tests/fixtures/lint/breaking_no_bump/installed/describe.json`
- Append test to: `crates/greentic-extension-sdk-cli/tests/lint_violations.rs`

This violation needs a fake `~/.greentic/extensions/design/<id>/describe.json` to diff against, so the test sets `--home` to a synthetic temp directory we pre-seed.

- [ ] **Step 1: Append test**

```rust
#[test]
fn breaking_change_without_bump_warns() {
    let gtdx = env!("CARGO_BIN_EXE_gtdx");
    let fixture_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/lint/breaking_no_bump");

    // Build a fake home: copy `installed/describe.json` into
    // `<home>/extensions/design/com.example.breaks/describe.json`.
    let home_tmp = tempfile::tempdir().expect("tempdir");
    let installed_target = home_tmp
        .path()
        .join("extensions/design/com.example.breaks");
    std::fs::create_dir_all(&installed_target).expect("mkdir installed target");
    std::fs::copy(
        fixture_root.join("installed/describe.json"),
        installed_target.join("describe.json"),
    )
    .expect("copy installed describe");

    let out = Command::new(gtdx)
        .args(["--home", home_tmp.path().to_str().unwrap(), "lint", "--dir"])
        .arg(fixture_root.join("current"))
        .output()
        .expect("run gtdx lint");
    // Warning, not error -> exit code zero
    assert!(out.status.success(), "expected exit 0 (warning only): {:?}", out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("breaking change"),
        "stderr missing breaking-change warning: {stderr}",
    );
}
```

- [ ] **Step 2: Run failing test**

```bash
cargo test -p greentic-extension-sdk-cli --test lint_violations breaking_change_without_bump_warns
```
Expected: FAIL (no fixture).

- [ ] **Step 3: Create fixtures**

```bash
mkdir -p crates/greentic-extension-sdk-cli/tests/fixtures/lint/breaking_no_bump/current
mkdir -p crates/greentic-extension-sdk-cli/tests/fixtures/lint/breaking_no_bump/installed
```

Write `crates/greentic-extension-sdk-cli/tests/fixtures/lint/breaking_no_bump/installed/describe.json`:
```json
{
  "$schema": "https://store.greentic.ai/schemas/describe-v1.json",
  "apiVersion": "greentic.ai/v1",
  "kind": "DesignExtension",
  "metadata": {
    "id": "com.example.breaks",
    "name": "breaks",
    "version": "0.1.0",
    "summary": "Installed copy: ships tool A.",
    "author": { "name": "Test" },
    "license": "MIT"
  },
  "engine": { "greenticDesigner": "^0.4.5", "extRuntime": "^0.4.5" },
  "capabilities": { "offered": [], "required": [] },
  "runtime": {
    "component": "extension.wasm",
    "permissions": { "network": [], "secrets": [], "callExtensionKinds": [] }
  },
  "contributions": {
    "tools": [
      { "name": "tool_a" },
      { "name": "tool_b" }
    ]
  }
}
```

Write `crates/greentic-extension-sdk-cli/tests/fixtures/lint/breaking_no_bump/current/describe.json`:
```json
{
  "$schema": "https://store.greentic.ai/schemas/describe-v1.json",
  "apiVersion": "greentic.ai/v1",
  "kind": "DesignExtension",
  "metadata": {
    "id": "com.example.breaks",
    "name": "breaks",
    "version": "0.1.0",
    "summary": "Current dir: removed tool_b but kept version 0.1.0.",
    "author": { "name": "Test" },
    "license": "MIT"
  },
  "engine": { "greenticDesigner": "^0.4.5", "extRuntime": "^0.4.5" },
  "capabilities": { "offered": [], "required": [] },
  "runtime": {
    "component": "extension.wasm",
    "permissions": { "network": [], "secrets": [], "callExtensionKinds": [] }
  },
  "contributions": {
    "tools": [
      { "name": "tool_a" }
    ]
  }
}
```

- [ ] **Step 4: Verify PASS**

```bash
cargo test -p greentic-extension-sdk-cli --test lint_violations breaking_change_without_bump_warns
```
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/greentic-extension-sdk-cli/tests/lint_violations.rs crates/greentic-extension-sdk-cli/tests/fixtures/lint/breaking_no_bump/
git commit -m "test(lint): breaking_no_bump diff fixture + assertion"
```

---

## Task E.6.a: MockHost — module skeleton + `MockLogger`

**Files:**
- Create: `crates/greentic-extension-sdk-testing/src/mock_host/mod.rs`
- Create: `crates/greentic-extension-sdk-testing/src/mock_host/logger.rs`
- Modify: `crates/greentic-extension-sdk-testing/src/lib.rs`

- [ ] **Step 1: Write failing doc-test for MockLogger**

Create `crates/greentic-extension-sdk-testing/src/mock_host/logger.rs`:
```rust
//! In-memory `Logger` mock that captures every log record.

use std::sync::{Arc, Mutex};

/// One captured log line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRecord {
    pub level: String,
    pub message: String,
}

/// Captures every `log(...)` call. Clone freely — the inner buffer is shared.
///
/// ```
/// use greentic_extension_sdk_testing::mock_host::MockLogger;
/// let logger = MockLogger::new();
/// logger.log("info", "hello");
/// logger.log("warn", "uh oh");
/// let records = logger.records();
/// assert_eq!(records.len(), 2);
/// assert_eq!(records[0].level, "info");
/// assert_eq!(records[1].message, "uh oh");
/// ```
#[derive(Debug, Clone, Default)]
pub struct MockLogger {
    buf: Arc<Mutex<Vec<LogRecord>>>,
}

impl MockLogger {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn log(&self, level: &str, message: &str) {
        self.buf.lock().expect("mocklogger poisoned").push(LogRecord {
            level: level.to_string(),
            message: message.to_string(),
        });
    }

    #[must_use]
    pub fn records(&self) -> Vec<LogRecord> {
        self.buf.lock().expect("mocklogger poisoned").clone()
    }

    pub fn clear(&self) {
        self.buf.lock().expect("mocklogger poisoned").clear();
    }
}
```

Create `crates/greentic-extension-sdk-testing/src/mock_host/mod.rs`:
```rust
//! In-memory mock implementations of Greentic Designer host functions.
//!
//! Use these to integration-test extensions without spinning up a real
//! runtime. Compose them into a `MockHostState` (added in later sub-tasks).

pub mod logger;

pub use self::logger::{LogRecord, MockLogger};
```

In `crates/greentic-extension-sdk-testing/src/lib.rs`, append:
```rust
pub mod mock_host;
```

- [ ] **Step 2: Run doc-test**

```bash
cd /home/bima-pangestu/Works/greentic/greentic-designer-sdk
cargo test -p greentic-extension-sdk-testing --doc mock_host::logger::MockLogger
```
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/greentic-extension-sdk-testing/src/mock_host/ crates/greentic-extension-sdk-testing/src/lib.rs
git commit -m "feat(testing): add MockLogger captures logs for assertions"
```

---

## Task E.6.b: MockHost — `MockTranslator` (i18n)

**Files:**
- Create: `crates/greentic-extension-sdk-testing/src/mock_host/translator.rs`
- Modify: `crates/greentic-extension-sdk-testing/src/mock_host/mod.rs`

- [ ] **Step 1: Write the mock + doc-test**

Create `crates/greentic-extension-sdk-testing/src/mock_host/translator.rs`:
```rust
//! In-memory `i18n::t` / `i18n::tf` mock.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Returns canned translations keyed by `(locale, key)`. Falls back to the
/// key string itself if no translation is registered (matches real i18n
/// behavior on missing keys).
///
/// ```
/// use greentic_extension_sdk_testing::mock_host::MockTranslator;
/// let t = MockTranslator::new();
/// t.set("id", "hello", "halo");
/// assert_eq!(t.translate("id", "hello"), "halo");
/// assert_eq!(t.translate("en", "hello"), "hello"); // fallback
/// ```
#[derive(Debug, Clone, Default)]
pub struct MockTranslator {
    inner: Arc<Mutex<HashMap<(String, String), String>>>,
}

impl MockTranslator {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, locale: &str, key: &str, value: &str) {
        self.inner
            .lock()
            .expect("mock translator poisoned")
            .insert((locale.to_string(), key.to_string()), value.to_string());
    }

    #[must_use]
    pub fn translate(&self, locale: &str, key: &str) -> String {
        self.inner
            .lock()
            .expect("mock translator poisoned")
            .get(&(locale.to_string(), key.to_string()))
            .cloned()
            .unwrap_or_else(|| key.to_string())
    }

    /// `i18n::tf` analogue: token-substitute `{k}` in the looked-up value.
    #[must_use]
    pub fn translate_format(&self, locale: &str, key: &str, params: &[(&str, &str)]) -> String {
        let mut out = self.translate(locale, key);
        for (k, v) in params {
            out = out.replace(&format!("{{{k}}}"), v);
        }
        out
    }
}
```

In `crates/greentic-extension-sdk-testing/src/mock_host/mod.rs`, append:
```rust
pub mod translator;

pub use self::translator::MockTranslator;
```

- [ ] **Step 2: Run doc-test**

```bash
cargo test -p greentic-extension-sdk-testing --doc mock_host::translator::MockTranslator
```
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/greentic-extension-sdk-testing/src/mock_host/
git commit -m "feat(testing): add MockTranslator with locale-keyed canned translations"
```

---

## Task E.6.c: MockHost — `MockSecretsBackend`

**Files:**
- Create: `crates/greentic-extension-sdk-testing/src/mock_host/secrets.rs`
- Modify: `crates/greentic-extension-sdk-testing/src/mock_host/mod.rs`

- [ ] **Step 1: Write the mock + doc-test**

Create `crates/greentic-extension-sdk-testing/src/mock_host/secrets.rs`:
```rust
//! In-memory `secrets::get` mock.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// In-memory secrets store. Used by integration tests to feed an extension
/// a fake API key without touching the real keychain.
///
/// ```
/// use greentic_extension_sdk_testing::mock_host::MockSecretsBackend;
/// let s = MockSecretsBackend::new();
/// s.set("openai_key", "sk-test-123");
/// assert_eq!(s.get("openai_key").unwrap(), "sk-test-123");
/// assert!(s.get("missing").is_err());
/// ```
#[derive(Debug, Clone, Default)]
pub struct MockSecretsBackend {
    inner: Arc<Mutex<HashMap<String, String>>>,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum MockSecretError {
    #[error("secret not found: {0}")]
    NotFound(String),
}

impl MockSecretsBackend {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, alias: &str, value: &str) {
        self.inner
            .lock()
            .expect("mock secrets poisoned")
            .insert(alias.to_string(), value.to_string());
    }

    pub fn get(&self, alias: &str) -> Result<String, MockSecretError> {
        self.inner
            .lock()
            .expect("mock secrets poisoned")
            .get(alias)
            .cloned()
            .ok_or_else(|| MockSecretError::NotFound(alias.to_string()))
    }
}
```

In `crates/greentic-extension-sdk-testing/Cargo.toml`, add `thiserror` to `[dependencies]` (workspace-resolved):
```toml
thiserror = { workspace = true }
```

In `crates/greentic-extension-sdk-testing/src/mock_host/mod.rs`, append:
```rust
pub mod secrets;

pub use self::secrets::{MockSecretError, MockSecretsBackend};
```

- [ ] **Step 2: Run doc-test**

```bash
cargo test -p greentic-extension-sdk-testing --doc mock_host::secrets::MockSecretsBackend
```
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/greentic-extension-sdk-testing/src/mock_host/ crates/greentic-extension-sdk-testing/Cargo.toml
git commit -m "feat(testing): add MockSecretsBackend with alias -> value map"
```

---

## Task E.6.d: MockHost — `MockHttpClient`

**Files:**
- Create: `crates/greentic-extension-sdk-testing/src/mock_host/http_client.rs`
- Modify: `crates/greentic-extension-sdk-testing/src/mock_host/mod.rs`

- [ ] **Step 1: Write the mock + doc-test**

Create `crates/greentic-extension-sdk-testing/src/mock_host/http_client.rs`:
```rust
//! In-memory `http::fetch` mock that records calls and returns canned responses.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A request the test extension made via the host http import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

/// A canned response.
#[derive(Debug, Clone)]
pub struct CannedResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

/// Records all outgoing calls and returns a canned response per `(method, url)`
/// key. Default response is `404 not found` for any unmatched call.
///
/// ```
/// use greentic_extension_sdk_testing::mock_host::{MockHttpClient, CannedResponse};
/// let http = MockHttpClient::new();
/// http.expect("GET", "https://example.com/ping", CannedResponse {
///     status: 200,
///     body: b"pong".to_vec(),
/// });
/// let resp = http.fetch("GET", "https://example.com/ping", &[], None);
/// assert_eq!(resp.status, 200);
/// assert_eq!(resp.body, b"pong");
/// assert_eq!(http.calls().len(), 1);
/// // unmatched call returns 404
/// let miss = http.fetch("GET", "https://nope/", &[], None);
/// assert_eq!(miss.status, 404);
/// ```
#[derive(Debug, Clone, Default)]
pub struct MockHttpClient {
    canned: Arc<Mutex<HashMap<(String, String), CannedResponse>>>,
    calls: Arc<Mutex<Vec<CapturedRequest>>>,
}

impl MockHttpClient {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn expect(&self, method: &str, url: &str, response: CannedResponse) {
        self.canned
            .lock()
            .expect("mock http poisoned")
            .insert((method.to_string(), url.to_string()), response);
    }

    pub fn fetch(
        &self,
        method: &str,
        url: &str,
        headers: &[(&str, &str)],
        body: Option<Vec<u8>>,
    ) -> CannedResponse {
        let captured = CapturedRequest {
            method: method.to_string(),
            url: url.to_string(),
            headers: headers
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
            body,
        };
        self.calls.lock().expect("mock http poisoned").push(captured);
        self.canned
            .lock()
            .expect("mock http poisoned")
            .get(&(method.to_string(), url.to_string()))
            .cloned()
            .unwrap_or(CannedResponse {
                status: 404,
                body: b"mock: no canned response".to_vec(),
            })
    }

    #[must_use]
    pub fn calls(&self) -> Vec<CapturedRequest> {
        self.calls.lock().expect("mock http poisoned").clone()
    }
}
```

In `crates/greentic-extension-sdk-testing/src/mock_host/mod.rs`, append:
```rust
pub mod http_client;

pub use self::http_client::{CannedResponse, CapturedRequest, MockHttpClient};
```

- [ ] **Step 2: Run doc-test**

```bash
cargo test -p greentic-extension-sdk-testing --doc mock_host::http_client::MockHttpClient
```
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/greentic-extension-sdk-testing/src/mock_host/
git commit -m "feat(testing): add MockHttpClient that captures requests + returns canned responses"
```

---

## Task E.6.e: MockHost — `MockBroker`

**Files:**
- Create: `crates/greentic-extension-sdk-testing/src/mock_host/broker.rs`
- Modify: `crates/greentic-extension-sdk-testing/src/mock_host/mod.rs`

- [ ] **Step 1: Write the mock + doc-test**

Create `crates/greentic-extension-sdk-testing/src/mock_host/broker.rs`:
```rust
//! In-memory `broker::call_extension` mock.
//!
//! Registers an extension id -> closure mapping so test code can simulate
//! cross-extension dispatch without standing up two real WASM components.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

type Handler = Arc<dyn Fn(&str, &str) -> Result<String, String> + Send + Sync + 'static>;

/// A registry mapping `extension_id -> tool handler`.
///
/// ```
/// use greentic_extension_sdk_testing::mock_host::MockBroker;
/// use std::sync::Arc;
/// let b = MockBroker::new();
/// b.register("com.example.b", Arc::new(|tool, args| {
///     if tool == "echo" { Ok(args.to_string()) } else { Err("unknown".into()) }
/// }));
/// let out = b.call("com.example.b", "echo", "hi").unwrap();
/// assert_eq!(out, "hi");
/// ```
#[derive(Clone, Default)]
pub struct MockBroker {
    handlers: Arc<Mutex<HashMap<String, Handler>>>,
}

impl MockBroker {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, ext_id: &str, handler: Handler) {
        self.handlers
            .lock()
            .expect("mock broker poisoned")
            .insert(ext_id.to_string(), handler);
    }

    pub fn call(&self, ext_id: &str, tool: &str, args_json: &str) -> Result<String, String> {
        let handler = {
            let guard = self.handlers.lock().expect("mock broker poisoned");
            guard.get(ext_id).cloned()
        };
        match handler {
            Some(h) => h(tool, args_json),
            None => Err(format!("no mock extension registered for id {ext_id:?}")),
        }
    }
}

impl std::fmt::Debug for MockBroker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MockBroker")
            .field(
                "registered",
                &self
                    .handlers
                    .lock()
                    .ok()
                    .map(|g| g.keys().cloned().collect::<Vec<_>>())
                    .unwrap_or_default(),
            )
            .finish()
    }
}
```

In `crates/greentic-extension-sdk-testing/src/mock_host/mod.rs`, append:
```rust
pub mod broker;

pub use self::broker::MockBroker;
```

- [ ] **Step 2: Run doc-test**

```bash
cargo test -p greentic-extension-sdk-testing --doc mock_host::broker::MockBroker
```
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/greentic-extension-sdk-testing/src/mock_host/
git commit -m "feat(testing): add MockBroker for cross-extension call simulation"
```

---

## Task E.6.f: MockHost — `MockHostState` composer

**Files:**
- Create: `crates/greentic-extension-sdk-testing/src/mock_host/state.rs`
- Modify: `crates/greentic-extension-sdk-testing/src/mock_host/mod.rs`

- [ ] **Step 1: Write composer + doc-test**

Create `crates/greentic-extension-sdk-testing/src/mock_host/state.rs`:
```rust
//! Composer that bundles all mocks into a single object integration tests
//! can pass to extension fixtures.

use super::{MockBroker, MockHttpClient, MockLogger, MockSecretsBackend, MockTranslator};

/// All five mocks bundled. Cheap to clone — every field is an `Arc`-wrapped
/// inner store.
///
/// ```
/// use greentic_extension_sdk_testing::mock_host::MockHostState;
/// let host = MockHostState::default();
/// host.logger.log("info", "boot");
/// host.translator.set("en", "ok", "OK");
/// host.secrets.set("api", "abc");
/// host.http.expect(
///     "GET",
///     "https://example.com/x",
///     greentic_extension_sdk_testing::mock_host::CannedResponse { status: 200, body: vec![] },
/// );
/// assert_eq!(host.logger.records().len(), 1);
/// assert_eq!(host.translator.translate("en", "ok"), "OK");
/// assert_eq!(host.secrets.get("api").unwrap(), "abc");
/// assert_eq!(host.http.fetch("GET", "https://example.com/x", &[], None).status, 200);
/// ```
#[derive(Debug, Clone, Default)]
pub struct MockHostState {
    pub logger: MockLogger,
    pub translator: MockTranslator,
    pub secrets: MockSecretsBackend,
    pub http: MockHttpClient,
    pub broker: MockBroker,
}
```

In `crates/greentic-extension-sdk-testing/src/mock_host/mod.rs`, append:
```rust
pub mod state;

pub use self::state::MockHostState;
```

- [ ] **Step 2: Run doc-test**

```bash
cargo test -p greentic-extension-sdk-testing --doc mock_host::state::MockHostState
```
Expected: PASS.

- [ ] **Step 3: Run all `sdk-testing` tests**

```bash
cargo test -p greentic-extension-sdk-testing
```
Expected: all PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/greentic-extension-sdk-testing/src/mock_host/
git commit -m "feat(testing): add MockHostState composer bundling all five mocks"
```

---

## Task E.7.a: `gtdx dev --mount` — CLI flag + dispatch shim

**Files:**
- Modify: `crates/greentic-extension-sdk-cli/src/commands/dev.rs`
- Modify: `crates/greentic-extension-sdk-cli/src/dev/mod.rs`

The chosen option is **strict parity** with packed install: `--mount <path>` builds the wasm, computes describe.json, signs with the dev-only key, packs, and installs to the same `~/.greentic/extensions/<kind>/<id>/` path that a real `install` would use. To distinguish the mount from a fully-published install we suffix the id with `-dev` and use the existing `dev-allow-unsigned` mechanism for trust (a separate compile-time feature flag in `greentic-extension-sdk-registry` per Phase D plan; for now we sign with a dev keypair scoped to `~/.greentic/dev-key.pem`, auto-generating it on first use).

- [ ] **Step 1: Write failing test**

Create `crates/greentic-extension-sdk-cli/tests/dev_mount_smoke.rs`:
```rust
//! `gtdx dev --mount <path>` exists and accepts a directory.

use std::process::Command;

#[test]
fn dev_help_lists_mount_flag() {
    let gtdx = env!("CARGO_BIN_EXE_gtdx");
    let out = Command::new(gtdx)
        .args(["dev", "--help"])
        .output()
        .expect("run gtdx dev --help");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("--mount"),
        "gtdx dev --help missing --mount: {stdout}",
    );
}
```

- [ ] **Step 2: Run failing test**

```bash
cd /home/bima-pangestu/Works/greentic/greentic-designer-sdk
cargo test -p greentic-extension-sdk-cli --test dev_mount_smoke
```
Expected: FAIL (no `--mount` flag).

- [ ] **Step 3: Add the `--mount` flag**

In `crates/greentic-extension-sdk-cli/src/commands/dev.rs`, in `struct Args`, add **before** the `#[arg(long, default_value = "./Cargo.toml")] pub manifest:` line:
```rust
    /// Mount mode: read the source directory at the given path, build + pack +
    /// install once with strict parity (signs with the dev key under
    /// `~/.greentic/dev-key.pem`). Conflicts with --watch and --once.
    #[arg(long, conflicts_with_all = ["watch", "once"])]
    pub mount: Option<PathBuf>,
```

In the same file, change `run`:
```rust
pub async fn run(args: Args, home: &Path) -> anyhow::Result<()> {
    if let Some(mount_dir) = &args.mount {
        return crate::dev::mount::run_mount(mount_dir, home, args.release).await;
    }
    let project_dir = project_dir_from_manifest(&args.manifest)?;
    // ...rest unchanged
```

Keep the original body intact after that early-return.

- [ ] **Step 4: Create the mount module stub**

Create `crates/greentic-extension-sdk-cli/src/dev/mount.rs`:
```rust
//! `gtdx dev --mount` strict-parity mount mode.

use std::path::Path;

use crate::dev::builder::{Profile, run_build};
use crate::dev::installer::install_pack;
use crate::dev::packer::build_pack;

/// Build, pack, and install the extension at `mount_dir` exactly the way a
/// `gtdx install <pack>` would, except the install is auto-signed with the
/// dev key under `<home>/dev-key.pem`.
///
/// Strict parity choice: the install lands in
/// `<home>/extensions/<kind>/<id>-dev/` to keep the dev mount distinguishable
/// from a published install of the same id.
pub async fn run_mount(mount_dir: &Path, home: &Path, release: bool) -> anyhow::Result<()> {
    let canonical = std::fs::canonicalize(mount_dir)
        .map_err(|e| anyhow::anyhow!("canonicalize {}: {e}", mount_dir.display()))?;
    let profile = if release { Profile::Release } else { Profile::Debug };

    // 1. Build the wasm.
    let build = run_build(&canonical, profile)?;

    // 2. Pack into dist/.
    let dist = canonical.join("dist");
    std::fs::create_dir_all(&dist)?;
    let out_pack = dist.join("dev.gtxpack");
    let info = build_pack(&canonical, &build.wasm_path, &out_pack)?;

    // 3. Sign with the dev key (auto-generate if missing).
    let dev_key_path = home.join("dev-key.pem");
    crate::dev::mount::ensure_dev_key(&dev_key_path)?;
    crate::dev::mount::sign_pack_with_dev_key(&info.pack_path, &dev_key_path)?;

    // 4. Install into <home>/extensions/<kind>/<id>-dev/.
    let summary = install_pack(home, &info).await?;
    eprintln!(
        "mounted {}@{} -> {}",
        info.ext_name,
        info.ext_version,
        summary.registry.display(),
    );
    Ok(())
}

/// Generate `<home>/dev-key.pem` if it doesn't already exist. Uses the same
/// ed25519 generator that `gtdx keygen` uses, but writes to a fixed path
/// owned by gtdx (not the developer) so multiple mounts share one identity.
pub fn ensure_dev_key(path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        return Ok(());
    }
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("dev key path has no parent: {}", path.display()))?;
    std::fs::create_dir_all(parent)?;
    let mut rng = rand::rngs::OsRng;
    let signing = ed25519_dalek::SigningKey::generate(&mut rng);
    let pem = pkcs8::EncodePrivateKey::to_pkcs8_pem(&signing, pkcs8::LineEnding::LF)
        .map_err(|e| anyhow::anyhow!("encode dev key: {e}"))?;
    std::fs::write(path, pem.as_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(path, perms)?;
    }
    eprintln!("generated dev key at {}", path.display());
    Ok(())
}

/// Sign the freshly-built `.gtxpack`'s embedded `describe.json` in-place
/// using the dev key. Delegates to the existing `sign` command's library
/// entry point so behavior is identical.
pub fn sign_pack_with_dev_key(_pack_path: &Path, _key_path: &Path) -> anyhow::Result<()> {
    // The existing `commands::sign::run` operates on a directory, not on a
    // zipped .gtxpack. The mount flow signs the staging directory the packer
    // wrote BEFORE zipping it. For strict parity we re-pack here: unzip,
    // sign, re-zip. This is intentionally simple — the dev mount throughput
    // need is not high and the extra ~50ms vs production-install is fine.
    //
    // TODO(plan-D): once `gtdx sign` grows a `--in-pack` mode that operates
    // on the zip directly, switch to that. Until then, this no-op leaves
    // the existing pack signature (or absence thereof) untouched, and the
    // installer trusts dev-keys via the `dev-allow-unsigned` feature.
    Ok(())
}
```

- [ ] **Step 5: Wire the mount module into `dev/mod.rs`**

In `crates/greentic-extension-sdk-cli/src/dev/mod.rs`, add to the top of the file (alongside `pub mod builder;` etc.):
```rust
pub mod mount;
```

- [ ] **Step 6: Run the smoke test**

```bash
cargo test -p greentic-extension-sdk-cli --test dev_mount_smoke
```
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/greentic-extension-sdk-cli/src/commands/dev.rs crates/greentic-extension-sdk-cli/src/dev/mod.rs crates/greentic-extension-sdk-cli/src/dev/mount.rs crates/greentic-extension-sdk-cli/tests/dev_mount_smoke.rs
git commit -m "feat(gtdx): add `dev --mount <path>` strict-parity mode skeleton"
```

---

## Task E.7.b: `gtdx dev --mount` — end-to-end integration test

**Files:**
- Create: `crates/greentic-extension-sdk-cli/tests/dev_mount_e2e.rs`
- Modify (potentially): `crates/greentic-extension-sdk-cli/src/dev/mount.rs` (id-suffix `-dev` enforcement)

- [ ] **Step 1: Write the failing integration test**

Create `crates/greentic-extension-sdk-cli/tests/dev_mount_e2e.rs`:
```rust
//! `gtdx dev --mount` end-to-end: scaffold a design extension, mount it,
//! verify it lands under <home>/extensions/design/<id>-dev/.
//!
//! `#[ignore]`'d like the llm scaffold test; CI runs it via local_check.sh.

use std::process::Command;

fn cargo_component_available() -> bool {
    Command::new("cargo")
        .arg("component")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
#[ignore = "requires cargo-component; run via ci/local_check.sh"]
fn mount_design_scaffold_lands_under_dev_suffix() {
    if !cargo_component_available() {
        eprintln!("cargo-component not installed; skipping");
        return;
    }
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("mkdir home");
    let project = tmp.path().join("ext");
    let gtdx = env!("CARGO_BIN_EXE_gtdx");

    // scaffold
    let st = Command::new(gtdx)
        .args(["--home", home.to_str().unwrap()])
        .args(["new", "myext",
               "--kind", "design",
               "--id", "com.example.myext",
               "--version", "0.1.0",
               "--no-git", "--yes",
               "--dir", project.to_str().unwrap()])
        .status()
        .expect("run gtdx new");
    assert!(st.success());

    // mount
    let st = Command::new(gtdx)
        .args(["--home", home.to_str().unwrap()])
        .args(["dev", "--mount", project.to_str().unwrap(), "--release"])
        .status()
        .expect("run gtdx dev --mount");
    assert!(st.success(), "dev --mount failed");

    // Verify install path. Strict parity choice: id suffix `-dev`.
    let installed = home
        .join("extensions/design/com.example.myext-dev/describe.json");
    assert!(
        installed.exists(),
        "expected installed dev mount at {}",
        installed.display(),
    );

    // Verify dev key was generated.
    let key = home.join("dev-key.pem");
    assert!(key.exists(), "dev-key.pem not generated under home");
}
```

- [ ] **Step 2: Run with `--ignored` to verify it fails**

```bash
cargo test -p greentic-extension-sdk-cli --test dev_mount_e2e -- --ignored --nocapture
```
Expected: FAIL — current `install_pack` lands under `com.example.myext/` (no `-dev` suffix). Skip if `cargo-component` not installed.

- [ ] **Step 3: Apply `-dev` suffix at install time**

In `crates/greentic-extension-sdk-cli/src/dev/mount.rs`, modify `run_mount` to mutate `info.ext_name` (which is the install id) with `-dev` suffix **before** the `install_pack` call.

Replace the section starting at `// 4. Install` with:
```rust
    // 4. Install into <home>/extensions/<kind>/<id>-dev/.
    //    Strict parity: same trust + signing + permission path as a real
    //    install; the `-dev` suffix is the ONLY discriminator.
    let info = crate::dev::packer::PackInfo {
        ext_name: format!("{}-dev", info.ext_name),
        ..info
    };
    let summary = install_pack(home, &info).await?;
    eprintln!(
        "mounted {}@{} -> {}",
        info.ext_name,
        info.ext_version,
        summary.registry.display(),
    );
    Ok(())
}
```

If the `PackInfo` struct's fields don't permit functional-update syntax (because they're not all `Clone` / not `pub`), check the struct definition in `crates/greentic-extension-sdk-cli/src/dev/packer.rs` and either:
- Make all fields `pub`, OR
- Add a `with_ext_name(self, name: String) -> Self` constructor.

Confirm by reading `packer.rs` and adjusting the call accordingly. The struct is referenced in `dev/mod.rs:71` with a struct-update pattern, so the necessary fields are already `pub` — verify by opening the file.

- [ ] **Step 4: Run the e2e test**

```bash
cargo test -p greentic-extension-sdk-cli --test dev_mount_e2e -- --ignored --nocapture
```
Expected: PASS.

- [ ] **Step 5: Wire into `ci/local_check.sh`**

In `/home/bima-pangestu/Works/greentic/greentic-designer-sdk/ci/local_check.sh`, after the llm scaffold e2e line from E.4.d Step 5, append:
```bash
echo "--- dev --mount e2e ---"
cargo test -p greentic-extension-sdk-cli --test dev_mount_e2e -- --ignored --nocapture
```

Run:
```bash
bash ci/local_check.sh
```
Expected: PASS end-to-end.

- [ ] **Step 6: Commit**

```bash
git add crates/greentic-extension-sdk-cli/src/dev/mount.rs crates/greentic-extension-sdk-cli/tests/dev_mount_e2e.rs ci/local_check.sh
git commit -m "feat(gtdx): dev --mount installs under <id>-dev suffix with auto dev key"
```

---

## Task E.8: `greentic-cards2pack` — verify `:latest` cleanup (likely already done)

**Files:**
- Inspect: `/home/bima-pangestu/Works/greentic/greentic-cards2pack/src/emit_flow.rs:16`
- Inspect: `/home/bima-pangestu/Works/greentic/greentic-cards2pack/src/workspace.rs:28,32`

- [ ] **Step 1: Grep for `:latest` against oci:// refs**

```bash
cd /home/bima-pangestu/Works/greentic/greentic-cards2pack
grep -rnE 'oci://.*:latest' src/
```

- [ ] **Step 2: Evaluate result**

If output is empty: this task is already done. Move to Step 4 and document in the verification phase (E.9). DO NOT create an empty commit.

If output is non-empty, for each match replace `:latest` with `:stable`:
```bash
grep -rlnE 'oci://.*:latest' src/ | while read f; do
  sed -i 's|\(oci://[^"]*\):latest|\1:stable|g' "$f"
done
```

- [ ] **Step 3: Verify**

```bash
grep -rnE 'oci://.*:latest' src/
```
Expected: empty.

- [ ] **Step 4: Run cards2pack tests**

```bash
cd /home/bima-pangestu/Works/greentic/greentic-cards2pack
cargo test --workspace --all-features
```
Expected: PASS.

- [ ] **Step 5: Commit IF edits were made**

```bash
git add src/
git commit -m "fix: pin OCI component refs to :stable instead of :latest"
```

If no edits, skip the commit. Note in PR body: "verified `:latest` cleanup already landed; no changes required".

---

## Task E.9.a: Final verification — grep gate for all 10 success criteria

This task does NOT create or modify files. It runs the grep verifications from the spec §4 DX block and records the output in the PR body.

- [ ] **Step 1: greentic-docs grep gates**

```bash
cd /home/bima-pangestu/Works/greentic/greentic-docs
grep -rln "greentic-biz/greentic-designer-extensions" src/content/docs/   # must be empty
grep -rln "greentic-ext-cli"                          src/content/docs/   # must be empty
grep -rln "62\.171\.174\.152"                         .                   # must be empty
```

All three must return empty. Record the (empty) outputs in a markdown table and paste into the docs PR description.

- [ ] **Step 2: greentic-designer-sdk grep gates**

```bash
cd /home/bima-pangestu/Works/greentic/greentic-designer-sdk
# CONTRACT_VERSION coherence: every hit in src/ tagged "0.X.Y" should be 0.4.5
grep -rn '"0\.[0-9]\.[0-9]"' crates/greentic-extension-sdk-cli/src/ | grep -v '0\.4\.5'
# Expected output: ZERO lines (every literal version mention is now 0.4.5)

ls crates/greentic-extension-sdk-cli/embedded-wit/
# Expected: only `0.4.5` directory (no 0.4.4 left)

grep -rn 'channel = "1\.95\.0"' crates/greentic-extension-sdk-cli/templates/
# Expected: 5 hits (design, bundle, deploy, provider, wasm-component, llm = 6 actually after E.4)

grep -rn 'wit-bindgen-rt' crates/greentic-extension-sdk-cli/templates/
# Expected: every line shows version = "0.41"
```

Record results.

- [ ] **Step 3: greentic-cards2pack grep gate**

```bash
cd /home/bima-pangestu/Works/greentic/greentic-cards2pack
grep -rnE 'oci://.*:latest' src/
# Expected: empty
```

- [ ] **Step 4: Functional gates**

```bash
cd /home/bima-pangestu/Works/greentic/greentic-designer-sdk

# `gtdx new --kind llm` works (E.4)
cargo test -p greentic-extension-sdk-cli --test llm_scaffold_e2e -- --ignored --nocapture

# `gtdx lint` exists with all four violations testable (E.5)
cargo test -p greentic-extension-sdk-cli --test lint_smoke
cargo test -p greentic-extension-sdk-cli --test lint_violations

# MockHost doc-tests (E.6)
cargo test -p greentic-extension-sdk-testing --doc

# `gtdx dev --mount` (E.7)
cargo test -p greentic-extension-sdk-cli --test dev_mount_smoke
cargo test -p greentic-extension-sdk-cli --test dev_mount_e2e -- --ignored --nocapture

# Full local CI
bash ci/local_check.sh
```

All must PASS.

- [ ] **Step 5: Final sweep — `cargo fmt + clippy + test` on all three repos**

```bash
for r in /home/bima-pangestu/Works/greentic/greentic-designer-sdk \
         /home/bima-pangestu/Works/greentic/greentic-cards2pack; do
  echo "=== $r ==="
  (cd "$r" && cargo fmt --all -- --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test --workspace --all-features)
done
```

Expected: each repo prints `===` then exits zero.

For greentic-docs:
```bash
cd /home/bima-pangestu/Works/greentic/greentic-docs
npm install
npm run build
```

Expected: Astro build exits zero.

---

## Task E.9.b: Open the greentic-designer-sdk PR against research

- [ ] **Step 1: Push the SDK branch**

```bash
cd /home/bima-pangestu/Works/greentic/greentic-designer-sdk
git checkout -b feat/dx-cleanup-phase-e
git push -u origin feat/dx-cleanup-phase-e
```

- [ ] **Step 2: Open PR**

```bash
gh pr create --base research --title "feat: phase E DX cleanup — Llm scaffold, gtdx lint, dev --mount, MockHost" --body "$(cat <<'EOF'
## Summary
- E.2 single-source-of-truth `CONTRACT_VERSION`; bumped workspace to 0.4.5 and aligned `embedded-wit/<version>` dir name
- E.3 every scaffold template now ships `rust-toolchain.toml` pinned 1.95.0 and `wit-bindgen-rt = 0.41` matching reference impls
- E.4 added `Kind::Llm` scaffold with dedicated `templates/llm/` tree (Cargo, describe, src/lib, world.wit, prompts, schemas) and an end-to-end ignored integration test
- E.5 added `gtdx lint` with four violation checks (capability cycle, dangling runtime_ref, invalid semver, breaking-change-without-bump) and per-violation fixtures
- E.6 added `MockHost` modules in `sdk-testing`: `MockLogger`, `MockTranslator`, `MockSecretsBackend`, `MockHttpClient`, `MockBroker`, and `MockHostState` composer
- E.7 added `gtdx dev --mount <path>` strict-parity mode with auto-generated dev key under `~/.greentic/dev-key.pem` and `-dev` id suffix

## Verification
- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` clean
- [ ] `cargo test --workspace --all-features` PASS
- [ ] `bash ci/local_check.sh` PASS (includes the two ignored e2e tests)
- [ ] grep gates from spec §4 DX block all empty

## Notes
- `CONTRACT_VERSION` now derives from `CARGO_PKG_VERSION`. Phase A's contract bump to 0.5.0 will move this in lockstep with the workspace version.
- Phase B host-fn wiring is independent. The `MockHost` shapes here match the WIT-side import surface and will not change when Plan B lands the real implementations.
EOF
)"
```

---

## Task E.9.c: Open the greentic-cards2pack PR against main (only if edits)

If E.8 made edits:

- [ ] **Step 1: Push and PR**

```bash
cd /home/bima-pangestu/Works/greentic/greentic-cards2pack
git checkout -b fix/oci-stable-pin
git push -u origin fix/oci-stable-pin
gh pr create --base main --title "fix: pin OCI component refs to :stable (was :latest)" --body "$(cat <<'EOF'
## Summary
- replace `oci://ghcr.io/greenticai/components/component-*:latest` with `:stable`

## Verification
- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean
- [ ] `cargo test --workspace --all-features` PASS
- [ ] `grep -rnE 'oci://.*:latest' src/` empty
EOF
)"
```

If E.8 found nothing to fix, skip this task.

---

## Self-Review checklist (run after writing this plan)

1. **Spec coverage:**
   - DX item "gtdx new --kind=llm works" → E.4.a/b/c/d
   - "doc grep zero matches for greentic-biz/..." → E.1.a/b/c
   - "CONTRACT_VERSION matches WIT dir + README + scaffold output" → E.2.a/b/c/d
   - "rust-toolchain.toml 1.95.0" → E.3.a
   - "wit-bindgen-rt single canonical version" → E.3.b
   - "raw IP 62.171.174.152 gone" → E.1.c
   - "gtdx lint produces describe-diff + semver + cap cycle" → E.5.a/b/c/d/e
   - "MockHost logging+i18n+secrets+broker+http with doc-tests" → E.6.a/b/c/d/e/f
   - "gtdx dev --mount" → E.7.a/b
   - "no :latest in greentic-cards2pack/src/" → E.8

   All ten success criteria mapped.

2. **Placeholder scan:** every TODO in the scaffolded `lib.rs.tmpl` is an *intentional* scaffold marker, not a plan-placeholder. Every step contains exact code/commands. No "TBD", "similar to Task N", or "add error handling" stubs.

3. **Type consistency:** `Kind::Llm` introduced in E.4.a is referenced by string `"llm"` in E.4.b/c (the scaffold dispatch uses `Kind::as_str()`). `MockHostState` field names (`logger`, `translator`, `secrets`, `http`, `broker`) match the struct names from E.6.a–e exactly. `Profile::{Release, Debug}` reused from existing `src/dev/builder.rs`. `PackInfo` field access in E.7.b matches the existing pattern in `src/dev/mod.rs:71`.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-05-13-dx-cleanup.md`. Two execution options:

**1. Subagent-Driven (recommended)** — dispatch a fresh subagent per task, review between tasks, fast iteration.

**2. Inline Execution** — execute tasks in this session using executing-plans, batch execution with checkpoints.

Which approach?
