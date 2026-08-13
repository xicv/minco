---
id: M14-T32
title: Prepare the Minco 1.7.0 Apple Container release candidate
milestone: M14
status: completed
priority: high
area: release/runtime/documentation
depends_on: [M14-T31]
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
  - docs-site/1.7.0/**
  - docs-site/next/**
  - docs-site/release.json
  - docs-site/versions.md
  - docs/adoption/1.6.0-to-1.7.0.md
  - docs/adoption/incremental-adoption.md
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
  - scripts/source_manifest.py
  - scripts/test/candidate_qualification.py
  - scripts/test/operational_evidence.py
  - scripts/test/quality_assurance.py
  - scripts/test/repository_truth.py
  - scripts/validate_operational_evidence.py
  - tasks/M14/M14-T32-prepare-1-7-release.md
  - verification/1.7-candidate-load.json
  - verification/1.7-candidate-recovery.json
  - verification/1.7-performance-baseline.json
  - verification/agent-workflows.json
  - verification/deep-review.json
  - verification/deployment-assurance.toml
  - verification/operational-evidence-validation.json
  - verification/performance-policy.toml
  - verification/provider-evidence.toml
  - verification/publish-validation.json
  - verification/quality-assurance-policy.toml
  - verification/quality-assurance.json
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

# M14-T32 - Prepare the Minco 1.7.0 Apple Container release candidate

## Goal

Release the already merged Apple-first local-service selection as one truthful,
lock-step `1.7.0` minor candidate. Freeze current documentation and update the
portable AI workflows without changing the exact-resource recovery rules,
Docker fallback, production topology or cloud cost model.

## Acceptance

- all 34 publishable packages and official plugin descriptors advance in
  lock-step to `1.7.0`, with no new package or ownership boundary;
- fresh automatic local-service selection is documented as Apple-first on a
  qualified Apple host and Docker-first only when Apple is unavailable;
- lifecycle receipts and exact owned resources still outrank the fresh default,
  and explicit `docker` and `apple` selections retain fail-closed behavior;
- the frozen `1.7.0` manual, changelog and 1.6.0-to-1.7.0 guide describe the
  compatibility and migration boundary without rewriting the 1.6.0 manual;
- generated references, package archives, skills and evidence bind one exact
  candidate source tree;
- SemVer comparison proves the complete public family remains additive against
  immutable `v1.6.0`;
- exact local quality and release qualification pass before tagging or
  publication; and
- hosted Linux, registry, docs.rs, Pages, provider, deployment and production
  evidence remain separate truthful states.

## Non-goals

- removing Docker or Compose support;
- automatically migrating or deleting persistent data;
- changing production runtime, Plan IR, AWS behavior or cloud cost;
- adding a package, provider, scheduler, fixed compute resource or deployment;
- creating `v1.7.0`, uploading crates or mutating production before exact-source
  qualification, review and merge.

## Starting evidence

The task starts from merged `main`
`a17c5d3d82b1f934ff4d82d16094e963c07d511f`. PR #162 qualified and merged the
Apple-first behavior with an identical source tree after full local quality and
exact-head clean-Linux run `31701044140`. crates.io, the GitHub release and the
frozen manual remain at `1.6.0`, so users installing `cargo-minco` cannot receive
the new behavior until a later exact release is published.

## Evidence

The exact sealed source passes `scripts/quality.sh` and the authoritative
`MINCO_QUALITY_TOOL_ROOT=/Users/xicao/.cargo scripts/ci/local-release.sh` from
an empty JJ child. The pinned measured lane records 127 nextest tests plus one
doctest, 85.80% line and 81.97% function coverage, 43 caught viable mutants
with zero misses/timeouts, and additive compatibility for all 34 packages
against immutable `v1.6.0`.

Candidate load passes 80/80 loopback API requests and 1,000/1,000 synthetic
worker messages. Candidate recovery passes repeatable migration, backup,
restore, application-read and rollback-contract checks. The clean release
matrix also passes AppSync local proof, all 34 package archive dry-runs,
selected unpacked-archive consumers, Plan/SAM validation, native Lambda and
worker builds, owned PostgreSQL and Rustack runtime qualification, and Orders
E2E.

These are bounded local results, not hosted Linux, AWS, deployment, production
or SLO proof. No tag, GitHub release, crates.io upload, live provider contact,
deployment or production mutation was performed while preparing the candidate.
