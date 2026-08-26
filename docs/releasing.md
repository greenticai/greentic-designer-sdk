# Cutting a release

The checklist lived only in release commit messages, so each release re-derived
it by reading the last one. This is that checklist, written down.

Releases are cut from `main`. Pushing a version bump to `main` triggers
`tag-on-version-bump`, which tags `v<version>`, which triggers `release.yml`:
six platform binaries plus a **permanent** crates.io publish. crates.io
versions cannot be deleted, only yanked — so everything below happens before
the merge, not after.

## 1. Land the change on `research` first

Promotion is `research → develop → main` and **research wins every conflicting
hunk**. A fix committed only to `main` disappears at the next promotion.

Land on `research`, then bring it to `main` by cherry-pick — not by merging
`research`, which would promote the whole line including work that is not
ready. Verify every file you ported is byte-identical:

```bash
for f in $(git diff --name-only origin/main HEAD); do
  git diff --quiet origin/research HEAD -- "$f" || echo "DIFFERS: $f"
done
```

## 2. Bump four things, not one

The last two are silent if missed:

| What | Where |
|---|---|
| `version` | workspace `Cargo.toml` |
| eight `=X.Y.Z` pins | four crate manifests |
| `embedded-wit/<version>/` | rename the directory |
| `Cargo.lock` | `cargo check --workspace` regenerates it |

```bash
sed -i '13s/^version = "OLD"$/version = "NEW"/' Cargo.toml
grep -rl '"=OLD"' --include=Cargo.toml crates/ | xargs sed -i 's/"=OLD"/"=NEW"/g'
git mv crates/greentic-extension-sdk-cli/embedded-wit/{OLD,NEW}
cargo check --workspace
```

`tests/contract_version_consistency.rs` covers the `embedded-wit` rename from
both sides — that the directory for the current version exists, and that no
legacy one lingers for someone to copy the wrong contract out of.

## 3. Update the README's version floor

Say which release the floor moved to **and why** — what a reader pinning an
older version is opting into. Also bump the `TAG=` in the manual-download
example and any "outranks the X stable" line.

## 4. Run the gate

```bash
cargo fmt --all -- --check
bash ci/local_check.sh          # fmt + clippy -D warnings + test + packaging dry-run
```

## 5. After merging: sync the README back to `research`

**This is the step that keeps getting missed.** The floor is only ever
corrected in a release commit, and release commits are cut from `main` — so
`research` keeps the old text and the next promotion overwrites `main`'s
corrected README with it.

It has drifted three times. Once the tag is cut:

```bash
git checkout -B docs/sync-readme origin/research
git checkout origin/main -- README.md
git diff --quiet origin/main -- README.md && echo "identical"
```

Open that as a PR to `research`. Both branches byte-identical means the next
promotion has nothing to reconcile.

## 6. Verify the published artifact, not the local build

A release is not done because CI went green. Install what was actually
published and exercise it:

```bash
cargo binstall --root ./check greentic-extension-sdk-cli   # expect: no "Compiling" lines
./check/bin/gtdx --version
```

Then scaffold each kind and run the lifecycle — `new`, `dev --once`,
`lint --publish`, `publish --dry-run`, `cargo test`. Crate metadata on
crates.io is immutable, so a defect that reaches a published version stays in
that version forever; only a new one fixes it.
