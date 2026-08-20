---
id: M14-T41
title: Release Minco 1.10.0 Ticketing support entry
milestone: M14
status: active
priority: critical
area: release/ticketing/agent/docs
depends_on: [M14-T40]
operations: []
checks:
  - uv run --locked python scripts/validate_publish.py --check-registry --require-registry
  - uv run --locked python scripts/test/repository_truth.py
  - uv run --locked python scripts/validate_static.py
  - scripts/docs/generate-reference.sh --check
  - cargo test -p cargo-minco --test agent_skills --locked
  - uv run --locked python scripts/test/agent_workflows.py --check-output verification/agent-workflows.json
  - scripts/docs/build.sh
  - scripts/docs/check-links.sh
  - scripts/docs/check-snippets.sh
  - scripts/docs/test-browser.sh
  - scripts/ci/local-release.sh
  - scripts/release/publish.sh
  - MINCO_QUALITY_TOOL_ROOT=/Users/xicao/.cargo scripts/ci/local-assurance.sh --ephemeral
  - uv run --locked python scripts/source_manifest.py --check
owned_paths:
  - CHANGELOG.md
  - CODEX_HANDOFF.md
  - PUBLISHING.md
  - README.md
  - REVIEW_STATUS.md
  - VERIFICATION.md
  - crates/minco-cli/assets/agent/**
  - docs-site/.vitepress/config.mts
  - docs-site/1.10.0/**
  - docs-site/next/**
  - docs-site/release.json
  - docs-site/tests/docs-discovery.spec.mts
  - docs-site/versions.md
  - docs/adoption/1.9.0-to-1.10.0.md
  - docs/adoption/incremental-adoption.md
  - docs/development/publishing.md
  - docs/development/using-minco-crate.md
  - docs/reference/compatibility.md
  - docs/reference/generated/diagnostics.md
  - docs/reference/supported-matrix.md
  - docs/vision/minco-framework-definition.md
  - infra/aws/generated/**
  - plugins/minco-plugin-feedback/src/lib.rs
  - plugins/minco-plugin-feedback/src/model.rs
  - plugins/minco-plugin-feedback/src/service.rs
  - plugins/minco-plugin-feedback/src/transcription.rs
  - proofs/realtime-appsync/aws-handler/Cargo.lock
  - roadmap/tasks.mmd
  - scripts/quality_assurance.py
  - scripts/test/quality_assurance.py
  - tasks/M14/M14-T41-release-1-10-0.md
  - verification/1.9-performance-baseline.json
  - verification/1.10-candidate-load.json
  - verification/1.10-published-release-validation.json
  - verification/agent-workflows.json
  - verification/deep-review.json
  - verification/deployment-assurance.toml
  - verification/operational-evidence-validation.json
  - verification/quality-assurance-policy.toml
  - verification/quality-assurance.json
  - verification/provider-evidence.toml
  - verification/publish-validation.json
  - verification/release-identity.json
  - verification/repository-truth.toml
  - verification/source-manifest.json
  - verification/static-validation.json
---

# M14-T41 - Release Minco 1.10.0 Ticketing support entry

## Goal

Release the additive Minco `1.10.0` lock-step family prepared by M14-T40. Keep
the complete 36-package inventory, nine portable AI skills, frozen versioned
manual, stable website and independent release evidence current. Cross the
first-publication boundary for `minco-interaction` and
`minco-plugin-ticketing` with the documented manual authenticated path, then
verify the complete family before configuring their future trusted publishers.

## Acceptance

- the exact candidate source passes the complete local release matrix and
  package dry run with all 36 packages in dependency order;
- every `1.10.0` changelog feature maps to current versioned documentation and
  each packaged skill that teaches it, with a refreshed deterministic receipt;
- the frozen `1.10.0` manual builds, links, snippets and browser journeys pass;
- the release change merges and the exact resulting `main` SHA passes the
  bounded clean-Linux compatibility workflow;
- immutable tag `v1.10.0` resolves to that exact qualified `main` SHA;
- one short-lived/manual authenticated publication uploads the complete family
  because the release contains two first-publication crates;
- independent registry validation proves all 36 exact `1.10.0` versions are
  present and non-yanked, then both new crates receive the reviewed trusted
  publisher configuration for future releases;
- the GitHub release is published from the pre-existing exact tag;
- repository truth and stable documentation are promoted only after registry
  proof, and Pages plus docs.rs are verified as separate states; and
- release evidence records exact commands, SHAs and run identifiers without
  promoting local or registry proof into provider, deployment or runtime proof.

## Non-goals

- changing the public Ticketing or Interaction API after candidate review;
- weakening first-publication, package, skill, documentation or source gates;
- publishing only a subset of the family except after an independently
  verified partial-upload failure;
- AWS, provider, application, database, migration or production mutation; and
- treating Pages, docs.rs, hosted compatibility or registry publication as
  proof of one another.

## Evidence

Pending exact-source qualification, hosted compatibility, tag, publication,
registry, GitHub release, docs.rs and stable Pages evidence. These states will
be recorded separately as they become authoritative.
