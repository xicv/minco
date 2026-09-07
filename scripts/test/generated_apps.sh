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
minco-test = { path = $(toml_path "$root/crates/minco-test") }
PATCH
}

run_generator() {
  local project="$1"
  shift
  cargo run --locked -p cargo-minco -- \
    --root "$project" \
    --json \
    "$@" >/dev/null
}

pin_known_broken_upstream() {
  local project="$1"
  # 2026-09-04: tinyvec 1.13.0 does not compile on the pinned toolchain
  # (its declared rust-version admits it, but the source calls the `vec!`
  # macro while importing only the `alloc::vec` module). The generated
  # applications resolve dependencies FRESH, so pin the last compatible
  # release until upstream yanks or fixes 1.13.0; no test is skipped or
  # weakened — the applications still compile and run their full suites.
  cargo update --manifest-path "$project/Cargo.toml" --precise 1.12.0 tinyvec
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
  pin_known_broken_upstream "$project"
  CARGO_TARGET_DIR="$root/target" \
    cargo check --locked --manifest-path "$project/Cargo.toml" --workspace --all-targets --all-features
  CARGO_TARGET_DIR="$root/target" \
    cargo test --locked --manifest-path "$project/Cargo.toml" --workspace --all-targets --all-features
  printf 'generated %s application compiled and tested successfully\n' "$database"

  run_generator "$project" stubs publish
  run_generator "$project" make module billing
  run_generator "$project" make migration add-widgets
  run_generator "$project" make seeder sample-widgets
  run_generator "$project" make worker email-dispatch
  run_generator "$project" make adapter widget-store
  run_generator "$project" make operation getPlatform
  run_generator "$project" make plugin metrics
  run_generator "$project" inspect
  run_generator "$project" db plan --set "minco-smoke-$database-$database"
  run_generator "$project" db seed \
    --profile demo \
    --environment local \
    --set "minco-smoke-$database-$database-seeds" \
    --dry-run

  cargo generate-lockfile --manifest-path "$project/Cargo.toml"
  pin_known_broken_upstream "$project"
  CARGO_TARGET_DIR="$root/target" \
    cargo check --locked --manifest-path "$project/Cargo.toml" --workspace --all-targets --all-features

  expected_failure="$temporary/$database-generated-specifications.log"
  if CARGO_TARGET_DIR="$root/target" \
    cargo test --locked --manifest-path "$project/Cargo.toml" --workspace --all-targets --all-features --no-fail-fast \
      >"$expected_failure" 2>&1; then
    echo "generated application and HTTP specifications unexpectedly passed" >&2
    exit 1
  fi
  rg --fixed-strings 'get_platform_business_behavior_must_be_implemented' "$expected_failure" >/dev/null
  rg --fixed-strings 'get_platform_http_contract_must_be_implemented' "$expected_failure" >/dev/null
  printf 'generated %s application generators compiled and failed on explicit TODO specifications as expected\n' "$database"
done
