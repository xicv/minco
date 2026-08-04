#!/usr/bin/env bash
set -euo pipefail

proof_root="$(cd "$(dirname "$0")/.." && pwd)"

cargo fmt --manifest-path "$proof_root/rust/Cargo.toml" --check
cargo test --locked --manifest-path "$proof_root/rust/Cargo.toml"
cargo clippy --locked --manifest-path "$proof_root/rust/Cargo.toml" --all-targets -- -D warnings

cargo fmt --manifest-path "$proof_root/aws-handler/Cargo.toml" --check
cargo test --locked --manifest-path "$proof_root/aws-handler/Cargo.toml"
cargo clippy --locked --manifest-path "$proof_root/aws-handler/Cargo.toml" --all-targets -- -D warnings

bash "$proof_root/scripts/check-aws-template.sh"
bash "$proof_root/scripts/test-live-authority.sh"

npm --prefix "$proof_root/browser" ci --ignore-scripts
npm --prefix "$proof_root/browser" test
npm --prefix "$proof_root/browser" audit --audit-level=high
