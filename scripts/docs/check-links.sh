#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

if [[ ! -d docs-site/.vitepress/dist ]]; then
  scripts/docs/build.sh
fi

uv run --locked python scripts/docs/check_links.py
