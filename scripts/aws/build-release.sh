#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"

config="${1:-examples/orders/config/minco.dev.toml}"
output_directory="${2:-target/minco}"
artifact="${3:-target/lambda/orders-lambda/bootstrap.zip}"
release_manifest="${4:-target/minco/release.json}"
plan="$output_directory/plan.json"
template="$output_directory/template.yaml"

mkdir -p "$output_directory" "$(dirname "$release_manifest")"
cargo minco package \
  --config "$config" \
  --plan "$plan" \
  --template "$template" \
  --output "$release_manifest"
uv run --locked python scripts/validate_static.py
SAM_CLI_TELEMETRY=0 sam validate --lint --template-file "$template"
cargo minco release verify "$release_manifest"
jq -e --arg artifact "$artifact" '
  [.artifacts[] | select(.file.path == $artifact)] | length == 1
' "$release_manifest" >/dev/null

printf 'Verified release manifest: %s\n' "$release_manifest"
