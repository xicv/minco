#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

cargo build --quiet --locked -p cargo-minco
exec uv run --locked python scripts/docs/generate_reference.py "$@"
