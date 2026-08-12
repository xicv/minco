---
id: M14-T24
title: Close the Minco 1.5.0 published release boundary
milestone: M14
status: active
priority: high
area: release/docs/evidence
depends_on: [M14-T23]
operations: []
owned_paths:
  - CHANGELOG.md
  - CODEX_HANDOFF.md
  - PUBLISHING.md
  - README.md
  - REVIEW_STATUS.md
  - VERIFICATION.md
  - docs-site/**
  - docs/adoption/1.4.0-to-1.5.0.md
  - docs/adoption/incremental-adoption.md
  - docs/development/publishing.md
  - docs/development/using-minco-crate.md
  - docs/reference/compatibility.md
  - docs/reference/generated/**
  - docs/reference/supported-matrix.md
  - docs/vision/minco-framework-definition.md
  - roadmap/tasks.mmd
  - scripts/test/repository_truth.py
  - tasks/M14/M14-T24-close-1-5-release.md
  - verification/1.5-performance-baseline.json
  - verification/1.5-published-release-validation.json
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

# M14-T24 - Close the Minco 1.5.0 published release boundary

## Goal

Promote repository and documentation truth only after the exact qualified
`1.5.0` source is tagged, published, and independently verified. Retain
separate evidence for source, hosted Linux, registry, GitHub release, docs.rs,
Pages and live-provider states.

## Acceptance

- immutable tag `v1.5.0` resolves to the exact reviewed release source;
- all 34 exact crates.io versions are present and non-yanked;
- the GitHub release is published from that tag;
- repository truth, stable documentation, install commands and the upgrade
  guide identify `1.5.0` as published;
- all nine packaged AI skills and their cumulative release-feature mapping
  remain current and byte-identical across Codex and Claude projections;
- the post-publication change passes local static, registry and documentation
  checks plus clean hosted Linux review;
- stable Pages and all exact docs.rs routes are verified independently; and
- absent live AWS/Waffo, performance and model/human evidence stays explicit.

## Non-goals

- changing a public Rust API, serialized Plan IR, CLI, plugin capability,
  package inventory, provider selection or deployment topology;
- adding another framework feature, workflow, service, schedule, poller, fixed
  compute resource or always-on control plane;
- contacting AWS or Waffo, deploying an application, or mutating production;
- treating registry, Pages, docs.rs or provider availability as interchangeable
  evidence; or
- fabricating model outcomes, human-review effort, provider proof or SLOs.

## Recovery and workspace

This post-publication task was bootstrapped in the dedicated JJ workspace
`/private/tmp/minco-task-m14-t24` from exact merged `main`
`c3706559357510d33d046fa461f8550fbbd4c04c`. The stale detached primary
checkout and unrelated `task-m12-t09` workspace are not used for mutation.

## Evidence

PR #157 reviewed exact head `0e6f02296ef69a84274eb74daed1dfaaccb50243`,
tree `6d7bd41cb1af0d83eb2e16324906a67b17643e0b`, with zero review threads and
passing clean-Linux run `31588777070`. It merged by guarded squash as exact
tree `6d7bd41cb1af0d83eb2e16324906a67b17643e0b` in commit
`c3706559357510d33d046fa461f8550fbbd4c04c`.

Exact merged-main clean-Linux run `31593051123` and authentication-only OIDC
run `31593053757` passed. Immutable tag `v1.5.0` resolves to the merge commit.
Publication run `31593507996` passed its archive, selected package, external
consumer and dependency-ordered upload gates. Independent registry validation
found all 34 exact `1.5.0` versions present and non-yanked. The
[`v1.5.0` GitHub release](https://github.com/xicv/minco/releases/tag/v1.5.0)
is published.

Keep this task active until the post-publication truth reaches `main`, stable
Pages is deployed, and all exact docs.rs routes are checked. No AWS/Waffo
application operation or production mutation was performed. Hosted performance,
current live-provider evidence, model-driven application evaluation and human
review measurement remain `NOT RUN` or absent.
