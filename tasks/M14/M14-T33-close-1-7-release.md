---
id: M14-T33
title: Close the Minco 1.7.0 published release boundary
milestone: M14
status: active
priority: high
area: release/docs/evidence
depends_on: [M14-T32]
operations: []
owned_paths:
  - CHANGELOG.md
  - CODEX_HANDOFF.md
  - PUBLISHING.md
  - README.md
  - REVIEW_STATUS.md
  - VERIFICATION.md
  - docs-site/**
  - docs/adoption/1.6.0-to-1.7.0.md
  - docs/adoption/incremental-adoption.md
  - docs/development/publishing.md
  - docs/development/using-minco-crate.md
  - docs/reference/compatibility.md
  - docs/reference/generated/**
  - docs/reference/supported-matrix.md
  - docs/vision/minco-framework-definition.md
  - roadmap/tasks.mmd
  - tasks/M14/M14-T33-close-1-7-release.md
  - verification/1.7-performance-baseline.json
  - verification/1.7-published-release-validation.json
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

# M14-T33 - Close the Minco 1.7.0 published release boundary

## Goal

Promote repository and documentation truth only after the exact qualified
`1.7.0` source is tagged, published and independently verified. Retain
separate evidence for source, hosted Linux, registry, GitHub release, docs.rs,
Pages and live-provider states.

## Acceptance

- immutable tag `v1.7.0` resolves to the exact reviewed release source;
- all 34 exact crates.io versions are present and non-yanked;
- a fresh public `cargo-minco =1.7.0` install reports `minco 1.7.0`;
- the GitHub release is published from that tag;
- repository truth, stable documentation, install commands and the upgrade
  guide identify `1.7.0` as published;
- the post-publication change passes local static, registry and documentation
  checks plus clean hosted Linux review;
- stable Pages and all exact docs.rs routes are verified independently; and
- absent live AWS, hosted performance and model/human evidence stays explicit.

## Non-goals

- changing a public Rust API, serialized Plan IR, CLI, plugin capability,
  package inventory, runtime selection or deployment topology;
- removing the explicit Docker fallback or migrating/deleting user data;
- adding another workflow, service, schedule, poller or fixed compute resource;
- contacting AWS, deploying an application or mutating production; or
- treating registry, Pages, docs.rs or provider availability as interchangeable
  evidence.

## Recovery and workspace

This post-publication task was bootstrapped in the dedicated JJ workspace
`/Users/xicao/Projects/minco-task-m14-t33` from exact released `main`
`7773892792696ccf061ddbb49fa284e5ba7f6747`. The merged candidate workspace
was forgotten and moved to Trash after its Minco-owned Docker test artifacts
were removed.

## Evidence

PR #163 reviewed exact candidate source
`22d62cb75a24011e2e83e9ccb3c4e07df4b02081`, tree
`31d279aca70e747ea934258ec2ce1548c66fd90d`, with zero unresolved review
threads and passing clean-Linux run `31712458388`. It merged by guarded squash
as the identical tree in commit
`7773892792696ccf061ddbb49fa284e5ba7f6747`.

Exact merged-main clean-Linux run `31712808528` and authentication-only OIDC
run `31713263154` passed. Immutable tag `v1.7.0` resolves to the merge commit.
Publication run `31713475849` passed its archive, selected-package,
external-consumer and dependency-ordered upload gates. Independent registry
validation found all 34 exact `1.7.0` versions present and non-yanked, and a
fresh public install reported `minco 1.7.0`. The
[`v1.7.0` GitHub release](https://github.com/xicv/minco/releases/tag/v1.7.0)
is published.

Keep this task active until post-publication truth reaches `main`, stable Pages
is deployed and all exact docs.rs routes are checked. No AWS application
operation or production mutation was performed. Hosted performance, current
live-provider evidence, model-driven application evaluation and human-review
measurement remain `NOT RUN` or absent.
