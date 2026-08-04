#!/usr/bin/env bash
set -euo pipefail

proof_root="$(cd "$(dirname "$0")/.." && pwd)"
repository_root="$(cd "$proof_root/../.." && pwd)"

cd "$repository_root"
cargo fmt --manifest-path "$proof_root/aws-handler/Cargo.toml" -- --check
cargo test --manifest-path "$proof_root/aws-handler/Cargo.toml" --locked
cargo clippy \
  --manifest-path "$proof_root/aws-handler/Cargo.toml" \
  --all-targets \
  --locked \
  -- -D warnings
npm ci --ignore-scripts --prefix "$proof_root/browser"
npm audit --audit-level=high --prefix "$proof_root/browser"
npm test --prefix "$proof_root/browser"
bash "$proof_root/scripts/check-aws-template.sh"
bash "$proof_root/scripts/test-live-authority.sh"
shellcheck "$proof_root/scripts/"*.sh

echo "Realtime AppSync local proof gates passed."
