#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../../.."

uv run --locked python scripts/test/examples/validate.py --check
exec uv run --locked python scripts/test/examples/validate.py --run "$@"
