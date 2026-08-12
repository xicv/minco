#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

mode="${1:-execute}"
if (($# > 1)) || [[ "$mode" != "execute" && "$mode" != "--check" && "$mode" != "--ephemeral" ]]; then
  printf 'usage: scripts/ci/local-assurance.sh [--check|--ephemeral]\n' >&2
  exit 2
fi

output="verification/quality-assurance.json"
execute_arguments=()
if [[ "$mode" == "--ephemeral" ]]; then
  output="target/minco/quality-assurance/release-receipt.json"
  execute_arguments+=(
    --performance-output
    target/minco/quality-assurance/release-candidate-load.json
  )
fi

tool_root="${MINCO_QUALITY_TOOL_ROOT:-/private/tmp/minco-quality-tools}"
if [[ "$mode" == "--check" ]]; then
  uv run --locked python scripts/quality_assurance.py \
    --check-output verification/quality-assurance.json
  exit 0
fi

if [[ ! -d "$tool_root/bin" ]]; then
  printf 'ASSURANCE-TOOL-001: pinned quality tool root is missing: %s\n' "$tool_root" >&2
  exit 1
fi

if ! rustup component list --installed | grep -qx 'llvm-tools.*'; then
  printf 'ASSURANCE-TOOL-002: the pinned Rust llvm-tools component is required\n' >&2
  exit 1
fi

uv run --locked python scripts/quality_assurance.py \
  --execute \
  --tool-root "$tool_root" \
  --output "$output" \
  "${execute_arguments[@]}"
uv run --locked python scripts/quality_assurance.py \
  --tool-root "$tool_root" \
  --check-output "$output"
