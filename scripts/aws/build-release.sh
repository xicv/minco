#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"

config="${1:-examples/orders/config/minco.dev.toml}"
output_directory="${2:-infra/aws/generated}"
artifact="${3:-target/lambda/orders-lambda/bootstrap.zip}"
release_manifest="${4:-target/minco/release.json}"
plan="$output_directory/plan.json"
template="$output_directory/template.yaml"

mkdir -p "$output_directory" "$(dirname "$release_manifest")"
scripts/aws/build-lambda.sh
cargo minco deploy plan --config "$config" --output "$plan"
cargo minco deploy render-sam --config "$config" --output "$template"
uv run --locked python scripts/validate_static.py
SAM_CLI_TELEMETRY=0 sam validate --lint --template-file "$template"
cargo minco release create \
  --artifact "$artifact" \
  --plan "$plan" \
  --template "$template" \
  --output "$release_manifest"
cargo minco release verify "$release_manifest"

printf 'Verified release manifest: %s\n' "$release_manifest"
