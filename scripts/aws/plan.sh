#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
config="${1:-examples/orders/config/minco.dev.toml}"
cargo minco deploy plan --config "$config" --output infra/aws/generated/plan.json
cargo minco deploy render-sam --config "$config" --output infra/aws/generated/template.yaml
