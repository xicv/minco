#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

uv run --locked python scripts/docs/check_snippets.py
