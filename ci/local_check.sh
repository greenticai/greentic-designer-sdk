#!/usr/bin/env bash
set -euo pipefail

echo "==> cargo fmt"
cargo fmt --all -- --check

echo "==> cargo clippy"
cargo clippy --workspace --all-targets --locked -- -D warnings

echo "==> cargo test"
cargo test --workspace --locked

echo "==> cargo build (release)"
cargo build --workspace --locked --release

echo "==> cargo publish --dry-run (leaf crates only — internal-dep crates verified post-publish ordering)"
cargo publish --dry-run --allow-dirty -p greentic-ext-contract
cargo publish --dry-run --allow-dirty -p greentic-ext-state

echo
echo "All checks passed."
