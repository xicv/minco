#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
uv run --locked python scripts/validate_static.py --output verification/static-validation.json
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
uv run --locked python scripts/test/quality_assurance.py
uv run --locked python scripts/quality_assurance.py
uv run --locked python scripts/test/release_identity.py
uv run --locked python scripts/release/release_identity.py --check
uv run --locked python scripts/test/hosted_ci_policy.py
uv run --locked python scripts/test/examples/test_recipes.py
uv run --locked python scripts/test/examples/validate.py --check
uv run --locked python scripts/validate_publish.py --output verification/publish-validation.json
uv run --locked python scripts/test/publish_validation.py
uv run --locked python scripts/deep_review.py
uv run --locked python scripts/test/deep_review_exclusions.py
uv run --locked python scripts/test/candidate_qualification.py
uv run --locked python scripts/test/feedback_contract.py
uv run --locked python scripts/test/sqlite_schema.py
uv run --locked python scripts/test/scaffold_templates.py
uv run --locked python scripts/test/rust_dependency_hygiene.py
uv run --locked python scripts/test/lambda_artifact_reproducibility.py
bash scripts/test/aws_shell_portability.sh
bash scripts/test/sqlx_feature_isolation.sh
node --check plugins/minco-plugin-feedback/assets/widget.js
scripts/test/feedback_browser.sh
scripts/docs/check-snippets.sh
scripts/docs/build.sh
scripts/docs/check-links.sh
scripts/docs/test-browser.sh
cargo fmt --all -- --check
cargo check -p minco --no-default-features --locked
cargo check -p minco --locked
cargo check -p minco --features official-plugins --locked
cargo check -p minco --features aws-worker --locked
cargo check -p minco --all-features --locked
cargo check --workspace --all-targets --all-features --locked
cargo check -p cargo-minco --locked
cargo test -p cargo-minco --test agent_skills --locked
cargo test -p minco --no-default-features --locked
cargo test -p minco --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
scripts/test/generated_apps.sh
uv run --locked python scripts/test/desk_binary_lifecycle.py
cargo rustdoc -p cargo-minco --lib --all-features --locked
cargo doc --workspace --all-features --no-deps --locked
cargo deny check advisories bans licenses sources
cargo audit
npm audit --prefix plugins/minco-plugin-feedback --audit-level=high
gitleaks dir . --no-banner --redact
uv run --locked python scripts/source_manifest.py --check
