---
id: M14-T15
title: Release Minco 1.2.2 documentation presentation hardening
milestone: M14
status: active
priority: high
area: docs/release
depends_on: [M14-T14]
operations: []
owned_paths:
  - Cargo.toml
  - Cargo.lock
  - CHANGELOG.md
  - README.md
  - CODEX_HANDOFF.md
  - PUBLISHING.md
  - REVIEW_STATUS.md
  - VERIFICATION.md
  - quality.toml
  - crates/**/Cargo.toml
  - extensions/**/Cargo.toml
  - plugins/**/Cargo.toml
  - examples/**/Cargo.toml
  - examples/plugins/third-party-minimal/Cargo.lock
  - proofs/realtime-appsync/aws-handler/Cargo.lock
  - extensions/**/minco-plugin.json
  - plugins/**/minco-plugin.json
  - infra/aws/generated/plan.json
  - crates/minco-cli/assets/agent/**
  - crates/minco-cli/src/agent_cmd.rs
  - crates/minco-cli/tests/agent_skills.rs
  - crates/minco-cli/tests/plugin_cli.rs
  - docs/adoption/1.2.1-to-1.2.2.md
  - docs/adoption/incremental-adoption.md
  - docs/development/publishing.md
  - docs/development/using-minco-crate.md
  - docs/reference/compatibility.md
  - docs/reference/generated/**
  - docs/reference/supported-matrix.md
  - docs/vision/minco-framework-definition.md
  - docs-site/**
  - roadmap/**
  - scripts/test/agent_workflows.py
  - scripts/test/candidate_qualification.py
  - scripts/test/hosted_ci_policy.py
  - scripts/test/operational_evidence.py
  - scripts/test/repository_truth.py
  - scripts/validate_static.py
  - tasks/M14/M14-T15-release-1-2-2-docs-polish.md
  - verification/**
checks:
  - bash scripts/docs/build.sh
  - scripts/docs/check-snippets.sh
  - scripts/docs/check-links.sh
  - scripts/docs/test-browser.sh
  - cargo test -p cargo-minco --test agent_skills --locked
  - cargo test -p cargo-minco --test plugin_cli --locked
  - uv run --locked python scripts/test/agent_workflows.py --check-output verification/agent-workflows.json
  - uv run --locked python scripts/test/repository_truth.py
  - uv run --locked python scripts/validate_static.py
  - scripts/docs/generate-reference.sh --check
  - bash scripts/ci/hosted-essential.sh
  - scripts/ci/local-release.sh
  - uv run --locked python scripts/source_manifest.py --check
---

# M14-T15 - Release Minco 1.2.2 documentation presentation hardening

## Goal

Ship a compatible lock-step `1.2.2` patch from one exact source tree that fixes
the public homepage's overflowing system-diagram labels and misaligned operating
model cards, retains responsive behavior, and keeps the packaged agent bundle
release-aware without claiming new provider or production evidence.

## Acceptance

- the homepage system diagram contains every label within its intended node at
  supported desktop and mobile sizes;
- the four operating-model cards have no native ordered-list markers, align on
  desktop, and retain intentional stacked spacing on mobile;
- browser regression coverage checks marker suppression, desktop alignment,
  mobile confinement, and the production-blueprint link;
- `1.2.2` is a SemVer-compatible patch across the complete 33-package family;
- the cumulative agent-release coverage records the documentation presentation
  contract and every packaged skill remains valid for the current version;
- the frozen `1.2.2` manual, changelog, upgrade guide, generated references,
  package archives and repository truth agree on the exact release;
- local release qualification and the bounded clean-Linux compatibility gate
  pass for the exact source before tagging; and
- tag, GitHub release, crates.io publication, docs.rs, Pages and provider/runtime
  evidence remain independently verified states.

## Non-goals

- changing public Rust APIs, Plan IR, plugin schema, runtime behavior or
  deployment topology;
- creating AWS resources, deploying an application, or claiming a production
  SLO from local or hosted qualification;
- changing skill names, projection paths, trigger semantics or mutation
  authority; or
- rewriting the immutable `v1.2.1` tag or its published crate archives.

## Recovery note

The two homepage defects were reported after `v1.2.1` and were first corrected
in the isolated `minco-task-docs-overflow` JJ workspace based exactly on merged
`main` `3bb4db37234c082073be0a997ae49b93fa4c4757`. That workspace was retained and
adopted as this task rather than copying changes through the stale primary
checkout.

## Evidence

Exact source `0496e6294b213c839af551a82858e2c1c3f7f45d` passed local release
qualification, PR-head clean-Linux run `31395154514` and merged-main run
`31395740260`. Immutable tag `v1.2.2` points to that source and OIDC publication
run `31396167046` uploaded the complete family. Independent validation found all
33 exact versions present and non-yanked, and the GitHub release is published
from the same tag.

Keep this task `active` until the post-publication truth change reaches `main`
and each attainable Pages/docs.rs evidence lane is recorded. Unavailable
live-provider and performance evidence remains explicit and does not block this
documentation-only patch from being truthfully qualified and published.
