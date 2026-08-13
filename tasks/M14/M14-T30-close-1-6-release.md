---
id: M14-T30
title: Close the Minco 1.6.0 published release boundary
milestone: M14
status: active
priority: high
area: release/docs/evidence
depends_on: [M14-T29]
operations: []
owned_paths:
  - CHANGELOG.md
  - CODEX_HANDOFF.md
  - PUBLISHING.md
  - README.md
  - REVIEW_STATUS.md
  - VERIFICATION.md
  - docs-site/**
  - docs/adoption/1.5.0-to-1.6.0.md
  - docs/adoption/incremental-adoption.md
  - docs/development/publishing.md
  - docs/development/using-minco-crate.md
  - docs/reference/compatibility.md
  - docs/reference/generated/**
  - docs/reference/supported-matrix.md
  - docs/vision/minco-framework-definition.md
  - roadmap/tasks.mmd
  - tasks/M14/M14-T30-close-1-6-release.md
  - verification/1.6-performance-baseline.json
  - verification/1.6-published-release-validation.json
  - verification/deep-review.json
  - verification/deployment-assurance.toml
  - verification/operational-evidence-validation.json
  - verification/provider-evidence.toml
  - verification/release-identity.json
  - verification/repository-truth.toml
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - uv run --locked python scripts/validate_publish.py --expect-published --check-registry --require-registry
  - uv run --locked python scripts/test/repository_truth.py
  - uv run --locked python scripts/validate_static.py
  - scripts/docs/generate-reference.sh --check
  - scripts/docs/build.sh
  - scripts/docs/check-links.sh
  - scripts/docs/check-snippets.sh
  - scripts/docs/test-browser.sh
  - bash scripts/ci/hosted-essential.sh
  - uv run --locked python scripts/source_manifest.py --check
---

# M14-T30 - Close the Minco 1.6.0 published release boundary

## Goal

Promote repository and documentation truth only after the exact qualified
`1.6.0` source is tagged, published, and independently verified. Retain
separate evidence for source, hosted Linux, registry, GitHub release, docs.rs,
Pages and live-provider states.

## Acceptance

- immutable tag `v1.6.0` resolves to the exact reviewed release source;
- all 34 exact crates.io versions are present and non-yanked;
- the GitHub release is published from that tag;
- repository truth, stable documentation, install commands and the upgrade
  guide identify `1.6.0` as published;
- all nine packaged AI skills and their cumulative release-feature mapping
  remain current and byte-identical across Codex and Claude projections;
- the post-publication change passes local static, registry and documentation
  checks plus clean hosted Linux review;
- stable Pages and all exact docs.rs routes are verified independently; and
- absent live AWS, performance and model/human evidence stays explicit.

## Non-goals

- changing a public Rust API, serialized Plan IR, CLI, plugin capability,
  package inventory, audit storage semantics or deployment topology;
- adding another workflow, service, schedule, poller, fixed compute resource or
  always-on control plane;
- contacting AWS, deploying an application, or mutating production;
- treating registry, Pages, docs.rs or provider availability as interchangeable
  evidence; or
- fabricating model outcomes, human-review effort, provider proof or SLOs.

## Recovery and workspace

This post-publication task was bootstrapped in the dedicated JJ workspace
`/Users/xicao/Projects/minco-task-m14-t30` from exact released `main`
`9abae9128dddc9bc32d099732e1421a0332e4785`. The stale primary checkout and
earlier release/audit workspaces are not used for mutation.

## Evidence

PR #160 reviewed exact candidate source `f47f28d696df9372a627c07b7590274e0da18dd9`,
tree `8747a5bf12991bc54263b635c1202912f729609d`, with zero unresolved review
threads and passing clean-Linux run `31689050949`. It merged by guarded squash
as the identical tree in commit
`9abae9128dddc9bc32d099732e1421a0332e4785`.

Exact merged-main clean-Linux run `31689854658` and authentication-only OIDC
run `31689854606` passed. Immutable tag `v1.6.0` resolves to the merge commit.
Publication run `31690283715` passed its archive, selected package,
external-consumer and dependency-ordered upload gates. Independent registry
validation found all 34 exact `1.6.0` versions present and non-yanked. The
[`v1.6.0` GitHub release](https://github.com/xicv/minco/releases/tag/v1.6.0)
is published.

Keep this task active until the post-publication truth reaches `main`, stable
Pages is deployed, and all exact docs.rs routes are checked. No AWS application
operation or production mutation was performed. Hosted performance, current
live-provider evidence, model-driven application evaluation and human review
measurement remain `NOT RUN` or absent.
