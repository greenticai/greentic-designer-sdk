#!/usr/bin/env bash
set -euo pipefail

echo "==> cargo fmt"
cargo fmt --all -- --check

echo "==> cargo clippy"
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings

echo "==> cargo test"
cargo test --workspace --all-features --locked

echo "==> cargo build (release)"
cargo build --workspace --locked --release

echo "==> cargo publish --dry-run (leaf crates only)"
# Only contract+state can be dry-run before a release: testing/registry/cli
# depend on these via path+version, and `cargo publish` (even --no-verify)
# resolves those deps against crates.io, where the unreleased `-research`
# versions don't exist yet. Their publishability is verified at release time by
# the ordered publish in release.yml (audit cycle-2 N13).
cargo publish --dry-run --allow-dirty -p greentic-extension-sdk-contract
cargo publish --dry-run --allow-dirty -p greentic-extension-sdk-state

echo
echo "All checks passed."
