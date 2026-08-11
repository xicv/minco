#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

required_commands=(
  aws
  cargo
  cargo-audit
  cargo-deny
  cargo-lambda
  cargo-llvm-cov
  cargo-mutants
  cargo-nextest
  cargo-semver-checks
  curl
  docker
  gitleaks
  jj
  node
  npm
  python3
  rg
  sam
  uv
  zig
)
missing_commands=()
for command in "${required_commands[@]}"; do
  command -v "$command" >/dev/null || missing_commands+=("$command")
done
if ((${#missing_commands[@]})); then
  printf 'local release qualification requires: %s\n' "${missing_commands[*]}" >&2
  exit 1
fi

./scripts/quality.sh
scripts/ci/local-assurance.sh --ephemeral
proofs/realtime-appsync/scripts/test-local.sh
scripts/release/candidate-recovery.sh
scripts/release/candidate-load.sh
scripts/release/publish.sh --skip-quality
scripts/aws/plan.sh
scripts/aws/validate.sh
scripts/aws/build-lambda.sh
scripts/aws/build-worker-lambda.sh
scripts/ci/local-runtime.sh
scripts/dev/rustack-smoke.sh
scripts/test/e2e.sh

printf 'Local release qualification passed; no provider, publication, or deployment claim was made.\n'
