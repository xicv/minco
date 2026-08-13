---
id: M14-T29
title: Prepare the Minco 1.6.0 audit release candidate
milestone: M14
status: complete
priority: high
area: release/audit/documentation
depends_on: [M14-T28]
operations: []
owned_paths:
  - CHANGELOG.md
  - CODEX_HANDOFF.md
  - Cargo.lock
  - Cargo.toml
  - PUBLISHING.md
  - README.md
  - REVIEW_STATUS.md
  - VERIFICATION.md
  - crates/minco-cli/assets/agent/**
  - crates/minco-cli/src/agent_cmd.rs
  - crates/minco-cli/src/delivery_evidence.rs
  - crates/minco-cli/src/handover_cmd.rs
  - docs-site/.vitepress/config.mts
  - docs-site/1.6.0/**
  - docs-site/next/**
  - docs-site/release.json
  - docs-site/tests/**
  - docs-site/versions.md
  - docs/adoption/1.5.0-to-1.6.0.md
  - docs/adoption/incremental-adoption.md
  - docs/deployment/audit-ledger.md
  - docs/development/publishing.md
  - docs/development/using-minco-crate.md
  - docs/reference/compatibility.md
  - docs/reference/generated/**
  - docs/reference/supported-matrix.md
  - docs/vision/minco-framework-definition.md
  - examples/orders/api/src/generated.rs
  - examples/plugins/third-party-minimal/Cargo.lock
  - extensions/**/minco-plugin.json
  - infra/aws/generated/**
  - plugins/**/minco-plugin.json
  - plugins/minco-plugin-payments-waffo/agent/**
  - plugins/minco-plugin-payments-waffo/README.md
  - proofs/realtime-appsync/aws-handler/Cargo.lock
  - quality.toml
  - roadmap/tasks.mmd
  - scripts/test/quality_assurance.py
  - scripts/source_manifest.py
  - scripts/test/candidate_qualification.py
  - scripts/test/operational_evidence.py
  - scripts/test/release_identity.py
  - scripts/test/repository_truth.py
  - scripts/validate_operational_evidence.py
  - tasks/M14/M14-T29-prepare-1-6-release.md
  - verification/1.6-performance-baseline.json
  - verification/1.6-candidate-load.json
  - verification/1.6-candidate-recovery.json
  - verification/agent-workflows.json
  - verification/deep-review.json
  - verification/deployment-assurance.toml
  - verification/operational-evidence-validation.json
  - verification/performance-policy.toml
  - verification/provider-evidence.toml
  - verification/quality-assurance-policy.toml
  - verification/quality-assurance.json
  - verification/publish-validation.json
  - verification/release-identity.json
  - verification/repository-truth.toml
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - cargo generate-lockfile
  - cargo test -p cargo-minco --test agent_skills --locked
  - uv run --locked python scripts/test/agent_workflows.py --check-output verification/agent-workflows.json
  - uv run --locked python scripts/test/repository_truth.py
  - uv run --locked python scripts/validate_static.py
  - scripts/docs/generate-reference.sh --check
  - scripts/docs/build.sh
  - scripts/docs/check-links.sh
  - scripts/docs/check-snippets.sh
  - scripts/docs/test-browser.sh
  - scripts/quality.sh
  - scripts/ci/local-release.sh
  - uv run --locked python scripts/source_manifest.py --check
---

# M14-T29 - Prepare the Minco 1.6.0 audit release candidate

## Goal

Prepare one truthful lock-step `1.6.0` candidate from the exact merged durable
audit-ledger baseline. Freeze current documentation, update every
version-matched portable AI skill, and bind the schema-agnostic ledger, SQL
journal/relay, DynamoDB transaction and Orders semantic-history tranches to one
reviewable candidate without tagging, publishing or deploying it.

## Acceptance

- all 34 publishable packages and official plugin descriptors advance in
  lock-step to `1.6.0`, with no new package or ownership boundary;
- the changelog and 1.5.0-to-1.6.0 guide describe the public audit model,
  atomicity/idempotency guarantees, separate-storage policy and compatibility;
- the frozen `1.6.0` manual documents SQL, SQLite and DynamoDB storage,
  relationship history, retention/archive, sizing and incomplete AWS cost;
- every packaged skill remains byte-identical across Codex and Claude and the
  cumulative release-feature map teaches the 1.6.0 audit boundary;
- generated references, package archives, repository truth and evidence bind
  one exact candidate source tree;
- SemVer comparison proves the complete public family remains additive against
  immutable `v1.5.0`;
- exact local quality and release qualification pass before any later tag or
  publication request; and
- hosted Linux, registry, docs.rs, Pages, live-provider, deployment, runtime,
  performance and production evidence remain separate truthful states.

## Non-goals

- changing the already merged audit architecture or adding a new provider;
- automatic audit deletion/rotation, a scheduler, poller, fixed compute
  resource or always-on control plane;
- inventing complete DynamoDB pricing without regional PITR and workload rates;
- contacting AWS, crates.io or another live provider; or
- creating `v1.6.0`, a GitHub release, a crates.io upload, a deployment or a
  production mutation.

## Recovery and workspace

This task did not exist when release preparation was requested. Its dedicated
JJ workspace was bootstrapped from exact merged `main`
`4bba904f498289bf2bfe6a4fa09a165e84e9d2e2` at
`/Users/xicao/Projects/minco-task-m14-t29`. The stale detached primary checkout
and prior audit workspaces are not used for release mutation.

## Evidence

Complete for the bounded candidate-preparation scope. Audit implementation PR
#159 passed the complete local release matrix and exact-head clean-Linux run
`31679230068`, then merged with an identical qualified tree at
`4bba904f498289bf2bfe6a4fa09a165e84e9d2e2`.

The exact `1.6.0` source passed `./scripts/quality.sh` and, from an empty JJ
child of the source change, `./scripts/ci/local-release.sh`. The canonical
assurance receipt records 127 nextest tests plus one doctest, 85.80% line and
81.97% function coverage, 43 caught viable mutants with zero misses or
timeouts, and all 34 SemVer comparisons against immutable `v1.5.0`. The local
candidate-load lane passed 80/80 loopback API requests and 1,000/1,000
synthetic worker messages; the separate recovery rehearsal passed migration,
backup, restore, application-read and rollback-contract checks. These local
measurements are not a production SLO or provider proof.

The clean release gate additionally passed all 34 package archive dry-runs,
selected unpacked-archive consumers, SAM validation, native Lambda and worker
builds, owned PostgreSQL and Rustack runtime qualification, AppSync local proof
and Orders E2E. Exact-head hosted Linux remains a separate PR review gate.

No tag, GitHub release, crates.io upload, Pages deployment, live provider
contact or production mutation is authorized by this task.
