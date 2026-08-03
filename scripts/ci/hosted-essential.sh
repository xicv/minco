#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0

uv run --locked python scripts/validate_static.py
scripts/docs/generate-reference.sh --check
uv run --locked python scripts/test/repository_truth.py
uv run --locked python scripts/test/hosted_ci_policy.py
uv run --locked python scripts/test/examples/validate.py --check
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features --locked
uv run --locked python scripts/source_manifest.py --check
