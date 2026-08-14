---
id: M14-T37
title: Close the Minco 1.8.0 published release boundary
milestone: M14
status: active
priority: high
area: release/docs/evidence
depends_on: [M14-T36]
operations: []
owned_paths:
  - CHANGELOG.md
  - CODEX_HANDOFF.md
  - PUBLISHING.md
  - README.md
  - REVIEW_STATUS.md
  - VERIFICATION.md
  - docs-site/**
  - docs/adoption/1.7.0-to-1.8.0.md
  - docs/adoption/incremental-adoption.md
  - docs/development/publishing.md
  - docs/development/using-minco-crate.md
  - docs/reference/compatibility.md
  - docs/reference/generated/**
  - docs/reference/supported-matrix.md
  - docs/vision/minco-framework-definition.md
  - roadmap/tasks.mmd
  - tasks/M14/M14-T37-close-1-8-release.md
  - verification/1.8-performance-baseline.json
  - verification/1.8-published-release-validation.json
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

# M14-T37 - Close the Minco 1.8.0 published release boundary

## Goal

Promote repository and documentation truth only after the exact qualified
`1.8.0` source is tagged, published and independently verified. Retain
separate evidence for source, hosted Linux, registry, GitHub release, docs.rs,
Pages and live-provider states.

## Acceptance

- immutable tag `v1.8.0` resolves to the exact reviewed release source;
- all 34 exact crates.io versions are present and non-yanked;
- a fresh public `cargo-minco =1.8.0` install reports `minco 1.8.0`;
- the GitHub release is published from that tag;
- repository truth, stable documentation, install commands and the upgrade
  guide identify `1.8.0` as published;
- the post-publication change passes local static, registry and documentation
  checks plus clean hosted Linux review;
- stable Pages and all exact docs.rs routes are verified independently; and
- absent live AWS, hosted performance and content-safety evidence stays
  explicit.

## Non-goals

- changing a public Rust API, serialized Plan IR, CLI, plugin capability,
  package inventory, runtime selection or deployment topology;
- enabling a CDN, acceleration, scanner, scheduler, fixed compute, NAT Gateway
  or provisioned concurrency;
- adding another workflow, contacting AWS, deploying an application or
  mutating production; or
- treating registry, Pages, docs.rs or provider availability as interchangeable
  evidence.

## Recovery and workspace

The task did not exist when publication completed, so its dedicated JJ
workspace was bootstrapped directly from exact released `main`
`fe1a20d4a6c76c7adef268727bb30b92b594e072` at
`/Users/xicao/Projects/minco-task-m14-t37`. The stale detached primary checkout
and unrelated task workspaces are not used for this release-truth mutation.

## Evidence

PR #168 reviewed exact candidate source
`b589612b17c2288a92e176cb08543eb6eacb826b`, tree
`3def2f3b5852f418d92e9ed87e86395b67d9870f`, with zero unresolved review
threads, passing exact-head clean-Linux run `31774750512` and a sealed security
review with zero findings. It merged by guarded squash as the identical tree in
commit `fe1a20d4a6c76c7adef268727bb30b92b594e072`.

Exact merged-main clean-Linux run `31775061737` and authentication-only OIDC
run `31775371863` passed. Immutable tag `v1.8.0` resolves to the merge commit.
Publication run `31775399279` passed its archive, selected-package,
external-consumer and dependency-ordered upload gates. Independent registry
validation found all 34 exact `1.8.0` versions present and non-yanked, and a
fresh public install reported `minco 1.8.0`. The
[`v1.8.0` GitHub release](https://github.com/xicv/minco/releases/tag/v1.8.0)
is published.

Post-publication source, stable Pages and docs.rs evidence remain in progress.
No AWS application operation or production mutation has been performed.
Hosted performance, current live-provider conformance and content-safety proof
remain `NOT RUN` or absent.
