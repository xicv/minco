#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
root="$PWD"

command -v cargo >/dev/null 2>&1 || {
  echo 'cargo is required to compile generated Minco applications' >&2
  exit 1
}

temporary="$(mktemp -d "${TMPDIR:-/tmp}/minco-generated-apps.XXXXXX")"
trap 'rm -rf "$temporary"' EXIT

toml_path() {
  python3 - "$1" <<'PY'
import json
import sys
print(json.dumps(sys.argv[1]))
PY
}

append_local_patches() {
  local project="$1"
  cat >>"$project/Cargo.toml" <<PATCH

[patch.crates-io]
minco = { path = $(toml_path "$root/crates/minco") }
minco-core = { path = $(toml_path "$root/crates/minco-core") }
minco-contract = { path = $(toml_path "$root/crates/minco-contract") }
PATCH
}

for database in postgres sqlite; do
  project="$temporary/minco-smoke-$database"
  cargo run --locked -p cargo-minco -- \
    new "minco-smoke-$database" \
    --directory "$project" \
    --database "$database" \
    --vcs none
  cargo run --locked -p cargo-minco -- \
    --root "$project" \
    --json \
    db plan \
    --set "minco-smoke-$database-$database" >/dev/null
  append_local_patches "$project"
  cargo generate-lockfile --manifest-path "$project/Cargo.toml"
  CARGO_TARGET_DIR="$root/target" \
    cargo check --locked --manifest-path "$project/Cargo.toml" --workspace --all-targets --all-features
  CARGO_TARGET_DIR="$root/target" \
    cargo test --locked --manifest-path "$project/Cargo.toml" --workspace --all-targets --all-features
  printf 'generated %s application compiled and tested successfully\n' "$database"
done
