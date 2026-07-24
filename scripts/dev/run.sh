#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
set -a
[[ ! -f .env ]] || source .env
set +a
exec cargo run -p orders-service --bin orders-local --features all-runtimes
