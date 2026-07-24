#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
command -v docker >/dev/null || { echo 'Docker is required' >&2; exit 1; }
docker compose -f infra/local/compose.yaml up -d --wait
printf '%s\n' 'PostgreSQL: postgres://minco:minco@127.0.0.1:55432/minco_orders'
printf '%s\n' 'Rustack:    http://127.0.0.1:4566'
