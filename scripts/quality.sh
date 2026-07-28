#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
uv run --locked python scripts/validate_static.py --output verification/static-validation.json
uv run --locked python scripts/test/repository_truth.py
uv run --locked python scripts/validate_publish.py --output verification/publish-validation.json
uv run --locked python scripts/test/publish_validation.py
uv run --locked python scripts/deep_review.py
uv run --locked python scripts/test/deep_review_exclusions.py
uv run --locked python scripts/test/feedback_contract.py
uv run --locked python scripts/test/sqlite_schema.py
uv run --locked python scripts/test/scaffold_templates.py
uv run --locked python scripts/test/rust_dependency_hygiene.py
uv run --locked python scripts/test/lambda_artifact_reproducibility.py
bash scripts/test/sqlx_feature_isolation.sh
node --check plugins/minco-plugin-feedback/assets/widget.js
cargo fmt --all -- --check
cargo check -p minco --no-default-features --locked
cargo check -p minco --locked
cargo check -p minco --features official-plugins --locked
cargo check -p minco --features aws-worker --locked
cargo check -p minco --all-features --locked
cargo check --workspace --all-targets --all-features --locked
cargo check -p cargo-minco --locked
cargo test -p minco --no-default-features --locked
cargo test -p minco --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
scripts/test/generated_apps.sh
cargo rustdoc -p cargo-minco --lib --all-features --locked
cargo doc --workspace --all-features --no-deps --locked
cargo deny check advisories bans licenses sources
cargo audit
npm audit --prefix plugins/minco-plugin-feedback --audit-level=high
gitleaks dir . --no-banner --redact
uv run --locked python scripts/source_manifest.py --check
