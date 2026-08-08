---
id: M14-T07
title: Close the release-bound feedback and operational evidence loop
milestone: M14
status: active
priority: critical
area: delivery/evidence

depends_on: [M14-T06]
operations: []
owned_paths:
  - Cargo.toml
  - Cargo.lock
  - CHANGELOG.md
  - README.md
  - VERIFICATION.md
  - CODEX_HANDOFF.md
  - crates/minco-plan/src/model.rs
  - crates/minco-plan/src/cost.rs
  - crates/minco-plan/tests/multi_runtime.rs
  - crates/minco-cli/src/feedback_cmd.rs
  - crates/minco-cli/src/delivery_evidence.rs
  - crates/minco-cli/src/handover_cmd.rs
  - crates/minco-cli/src/main.rs
  - crates/minco-cli/README.md
  - crates/minco-cli/assets/agent/bundle.json
  - crates/minco-cli/tests/plugin_cli.rs
  - plugins/minco-plugin-feedback/src/model.rs
  - plugins/minco-plugin-feedback/src/service.rs
  - plugins/minco-plugin-feedback/README.md
  - plugins/minco-plugin-feedback/src/plugin.rs
  - plugins/minco-plugin-feedback/tests/persistence.rs
  - plugins/minco-plugin-audit/minco-plugin.json
  - plugins/minco-plugin-events/minco-plugin.json
  - plugins/minco-plugin-feedback/minco-plugin.json
  - plugins/minco-plugin-health/minco-plugin.json
  - plugins/minco-plugin-idempotency/minco-plugin.json
  - plugins/minco-plugin-identity/minco-plugin.json
  - plugins/minco-plugin-notifications/minco-plugin.json
  - plugins/minco-plugin-object-storage/minco-plugin.json
  - plugins/minco-plugin-observability/minco-plugin.json
  - plugins/minco-plugin-realtime/minco-plugin.json
  - plugins/minco-plugin-sessions/minco-plugin.json
  - plugins/minco-plugin-static-site/minco-plugin.json
  - extensions/minco-aws-adapters/minco-plugin.json
  - extensions/minco-aws-dynamodb/minco-plugin.json
  - extensions/minco-aws-lambda/minco-plugin.json
  - extensions/minco-aws-worker/minco-plugin.json
  - extensions/minco-sqlx-postgres/minco-plugin.json
  - extensions/minco-sqlx-sqlite/minco-plugin.json
  - examples/orders/api/src/generated.rs
  - examples/plugins/third-party-minimal/Cargo.lock
  - scripts/release/candidate_qualification.py
  - scripts/test/candidate_qualification.py
  - scripts/validate_operational_evidence.py
  - scripts/test/operational_evidence.py
  - scripts/source_manifest.py
  - scripts/validate_static.py
  - scripts/docs/generate_reference.py
  - scripts/ci/hosted-essential.sh
  - scripts/test/hosted_ci_policy.py
  - scripts/test/repository_truth.py
  - scripts/quality.sh
  - quality.toml
  - verification/repository-truth.toml
  - verification/performance-policy.toml
  - verification/provider-evidence.toml
  - verification/aws-capability-candidates.toml
  - verification/1.2-performance-baseline.json
  - verification/operational-evidence-validation.json
  - verification/static-validation.json
  - verification/deep-review.json
  - verification/publish-validation.json
  - verification/source-manifest.json
  - docs/DECISIONS.md
  - docs/adrs/0034-release-bound-delivery-evidence.md
  - docs/adoption/1.1.0-to-1.2.0.md
  - docs/adoption/incremental-adoption.md
  - docs/architecture/feedback-loop.md
  - docs/architecture/performance.md
  - docs/deployment/release.md
  - docs/development/using-minco-crate.md
  - docs/research/aws-rust-capability-review-2026-08.md
  - docs/vision/minco-framework-definition.md
  - docs/reference/generated/cli.md
  - docs/reference/generated/diagnostics.md
  - docs/reference/generated/packages.md
  - docs/reference/generated/plugins.md
  - docs/reference/generated/schemas.md
  - docs-site/release.json
  - docs-site/package.json
  - docs-site/package-lock.json
  - docs-site/tests/docs.spec.mts
  - docs-site/next/getting-started/installation.md
  - docs-site/next/index.md
  - docs-site/1.2.0/cookbook/index.md
  - docs-site/1.2.0/cookbook/orders-api.md
  - docs-site/1.2.0/examples/index.md
  - docs-site/1.2.0/explanation/zero-idle.md
  - docs-site/1.2.0/features/index.md
  - docs-site/1.2.0/getting-started/first-application.md
  - docs-site/1.2.0/getting-started/framework-tour.md
  - docs-site/1.2.0/getting-started/installation.md
  - docs-site/1.2.0/getting-started/project-structure.md
  - docs-site/1.2.0/guides/agent-development.md
  - docs-site/1.2.0/guides/background-work.md
  - docs-site/1.2.0/guides/configuration.md
  - docs-site/1.2.0/guides/database-lifecycle.md
  - docs-site/1.2.0/guides/deployment.md
  - docs-site/1.2.0/guides/dynamodb.md
  - docs-site/1.2.0/guides/events-and-notifications.md
  - docs-site/1.2.0/guides/feedback.md
  - docs-site/1.2.0/guides/files-and-static-sites.md
  - docs-site/1.2.0/guides/identity-and-sessions.md
  - docs-site/1.2.0/guides/local-development.md
  - docs-site/1.2.0/guides/plugin-conformance.md
  - docs-site/1.2.0/guides/project-view.md
  - docs-site/1.2.0/guides/realtime.md
  - docs-site/1.2.0/guides/resource-api.md
  - docs-site/1.2.0/index.md
  - docs-site/1.2.0/plugins/index.md
  - docs-site/1.2.0/plugins/using-plugins.md
  - docs-site/1.2.0/reference/cli.md
  - docs-site/1.2.0/reference/feature-flags.md
  - docs-site/1.2.0/reference/plugin-conformance.md
  - docs-site/1.2.0/reference/resource-api.md
  - docs-site/1.2.0/reference/testing.md
  - docs-site/next/guides/background-work.md
  - docs-site/next/reference/feature-flags.md
  - tasks/M14/M14-T07-delivery-evidence-loop.md
checks:
  - python -m py_compile scripts/release/candidate_qualification.py scripts/validate_operational_evidence.py scripts/test/candidate_qualification.py scripts/test/operational_evidence.py scripts/source_manifest.py scripts/validate_static.py scripts/docs/generate_reference.py scripts/test/hosted_ci_policy.py scripts/test/repository_truth.py
  - uv run --locked python scripts/test/candidate_qualification.py
  - uv run --locked python scripts/validate_operational_evidence.py
  - uv run --locked python scripts/test/operational_evidence.py
  - uv run --locked python scripts/validate_static.py
  - cargo test -p minco-plan --locked
  - cargo test -p minco-plugin-feedback --all-features --locked
  - cargo test -p cargo-minco feedback_cmd --locked
  - cargo test -p cargo-minco handover_cmd --locked
  - cargo test -p cargo-minco --test plugin_cli --locked
  - rustfmt --edition 2024 --check over modified/created Rust files only
  - scripts/docs/generate-reference.sh --check
  - bash scripts/ci/hosted-essential.sh
  - uv run --locked python scripts/source_manifest.py --check
---

## Goal

Turn Minco's rapid, low-idle-cost deployment advantage into one verifiable
client-delivery loop. Plan validation and cost evidence must understand the
selected runtime/ingress topology; client feedback must be convertible into a
repository task only when it is bound to an exact verified release and
successful deployment; current performance and provider evidence must remain
machine-readable and freshness-aware; and an exact handover packet must expose
what is proven, stale, deferred, or still unverified without revealing secrets.

## Acceptance

- unsupported runtime/ingress combinations fail during Plan IR validation rather
  than only during provider rendering;
- runtime cost evidence and missing-rate requirements are selected by runtime and
  ingress instead of always assuming API Gateway plus Lambda;
- `cargo minco feedback task` is read-only by default, verifies exact release and
  deployment bindings, rejects ambiguous or stale feedback context, and writes a
  task plus immutable receipt only with an explicit digest approval;
- `cargo minco handover` emits a deterministic, secret-free packet bound to an
  exact successful deployment and the repository's current evidence ledgers;
- the candidate qualification report records p50, p95, p99 and maximum latency,
  and a checked-in policy validates the unpublished 1.2.0 candidate baseline without
  presenting a hosted-runner threshold as a production SLO;
- provider evidence records exact source scope, observed time, Region, cleanup,
  evidence dimensions and freshness, while historical proof remains visible but
  cannot qualify the current release;
- modern AWS and Rust capabilities are recorded in one reviewed candidate ledger
  with explicit support status, residual cost class, wake sources, prerequisites
  and adoption triggers rather than becoming implicit framework promises;
- the bounded hosted essential gate runs the new evidence validator and targeted
  tests before workspace checking and source-manifest verification;
- generated CLI and diagnostic references are regenerated from the exact final
  tree; and
- no live AWS resource, production deployment, release tag or registry
  publication is created or changed.

## Non-goals

- changing the default API Gateway HTTP API plus native ARM64 Lambda profile;
- claiming Lambda Function URLs, Managed Instances, MicroVMs, Durable Functions,
  Aurora zero-ACU, AppConfig experimentation or Cedar authorization as supported
  merely because AWS exposes them;
- adding an always-on Minco control plane, telemetry collector or background
  poller;
- treating client feedback text as trusted instructions or automatically
  implementing it; or
- imposing repository-wide coverage or mutation thresholds without measured
  signal and CI-cost evidence.

## Evidence

Local task-bounded Python, Plan, Feedback, handover, plugin-catalog, Clippy,
restricted rustfmt and generated-reference checks pass on macOS with the pinned
toolchain. The committed performance candidate remains `NOT RUN`, the current
provider profile records no provider contact, and historical 0.4 evidence is
`stale`; therefore no production SLO or live-provider acceptance is claimed.
The sealed operational-validation receipt contains the two expected
`NOT RUN`/missing-provider warnings without embedding its own digest into the
source authority it identifies.
Exact-tree hosted Linux qualification of the final PR head remains pending, so
this task stays `active`. No AWS resource, deployment, promotion, tag, release
or publication was created by M14-T07.
