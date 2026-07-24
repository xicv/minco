#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
uv run --locked python scripts/validate_static.py
uv run --locked python scripts/validate_publish.py
uv run --locked python scripts/deep_review.py
uv run --locked python scripts/test/deep_review_exclusions.py
uv run --locked python scripts/test/feedback_contract.py
node --check plugins/minco-plugin-feedback/assets/widget.js
cargo fmt --all -- --check
cargo check -p minco --no-default-features --locked
cargo check -p minco --locked
cargo check -p minco --all-features --locked
cargo check -p cargo-minco --locked
cargo test -p minco --no-default-features --locked
cargo test -p minco --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
cargo rustdoc -p cargo-minco --lib --all-features --locked
cargo doc --workspace --all-features --no-deps --locked
