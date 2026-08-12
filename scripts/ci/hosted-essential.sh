#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0

uv run --locked python scripts/validate_static.py
scripts/docs/generate-reference.sh --check
uv run --locked python scripts/test/cost_regression.py
uv run --locked python scripts/cost_regression.py --check
uv run --locked python scripts/test/agent_workflows.py --check-output verification/agent-workflows.json
uv run --locked python scripts/test/repository_truth.py
uv run --locked python scripts/validate_deployment_assurance.py
uv run --locked python scripts/test/deployment_assurance.py
uv run --locked python scripts/test/current_product_truth.py
uv run --locked python scripts/validate_operational_evidence.py --check-output verification/operational-evidence-validation.json
uv run --locked python scripts/test/operational_evidence.py
uv run --locked python scripts/test/hosted_ci_policy.py
uv run --locked python scripts/test/examples/validate.py --check
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features --locked
cargo test -p cargo-minco --test agent_skills --locked
uv run --locked python scripts/source_manifest.py --check
