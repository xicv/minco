#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

missing=0
for command in python3 uv git jj cargo rustc rustfmt node npm; do
  if ! command -v "$command" >/dev/null 2>&1; then
    printf 'missing required development tool: %s\n' "$command" >&2
    missing=1
  fi
done
if (( missing != 0 )); then
  cat >&2 <<'TEXT'
Install the missing tools, then rerun this script. The repository intentionally does not
execute remote installation scripts without operator review. Rust is pinned in
rust-toolchain.toml; Jujutsu should use a colocated Git backend for GitHub interoperability.
TEXT
  exit 1
fi
node_major="$(node -p 'process.versions.node.split(".")[0]')"
if (( node_major < 20 )); then
  printf 'Node.js 20 or newer is required; found %s.\n' "$(node --version)" >&2
  exit 1
fi
if [[ ! -f Cargo.lock ]]; then
  cargo generate-lockfile
  printf 'Generated Cargo.lock; review and commit it before any release.\n'
fi
uv sync --locked --only-dev
if [[ ! -d .jj ]]; then
  cargo minco vcs init
fi
cargo minco doctor
cargo minco contract sync --check
uv run --locked python scripts/validate_publish.py
printf 'Minco development prerequisites are ready.\n'
