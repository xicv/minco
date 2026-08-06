---
id: M14-T02
title: Promote published Minco 1.1.0 repository and documentation truth
milestone: M14
status: in_progress
priority: critical
area: release/1.1
depends_on: [M14-T01, M14-T04, M14-T05]
operations: []
owned_paths:
  - README.md
  - CHANGELOG.md
  - CODEX_HANDOFF.md
  - PUBLISHING.md
  - VERIFICATION.md
  - docs/**
  - docs-site/**
  - roadmap/**
  - scripts/test/repository_truth.py
  - tasks/M14/M14-T02-promote-1-1-publication.md
  - verification/**
checks:
  - uv run --locked python scripts/validate_static.py
  - uv run --locked python scripts/test/repository_truth.py
  - uv run --locked python scripts/validate_publish.py --expect-published --check-registry --require-registry
  - npm --prefix docs-site run build
  - npm --prefix docs-site run test:browser
---

## Goal

After immutable `v1.1.0` publication is independently verified, update current
repository truth and the documentation landing/version surfaces to make
`1.1.0` the stable release without rewriting its tag.

## Acceptance

- registry verification proves every exact package is present and non-yanked;
- README and repository truth identify `1.1.0` as the published baseline;
- the stable site navigation and landing page point to the frozen `1.1.0`
  manual while `next` remains unreleased; and
- the Pages deployment is bound to and verified from the exact merged
  post-publication main SHA.

## Non-goals

- changing the immutable release tag or rebuilding release archives;
- treating documentation deployment as live AWS application deployment; or
- deleting older versioned manuals.

## Evidence

The promotion source phase started in isolated workspace
`minco-task-m14-t02` from refreshed exact main
`981dfa26e1dec0111eb3297845403d125f020485`, after the safe publication-resume
change merged. Immutable tag `v1.1.0` still resolved to qualified release
source `4d81543f7c5adb773655f23278abfe084de9f3e0`.

Recovery run
[`31072152251`](https://github.com/xicv/minco/actions/runs/31072152251)
passed the complete tag-bound release gate, proved the exact
five-present/28-absent crates.io complement, obtained a short-lived OIDC token,
published only the 28 absent packages, and revoked the token. Independent
registry validation then reported zero errors and 33 successful exact-version,
non-yanked checks. GitHub release
[`v1.1.0`](https://github.com/xicv/minco/releases/tag/v1.1.0) is published from
the same immutable tag.

The source promotion updates repository truth, README/install examples, the
stable landing/version surfaces, the frozen 1.1 manual, compatibility/support
guidance and the 1.0-to-1.1 upgrade record. A repository-truth regression first
exposed the historical `0.6.0` previous-baseline fixture; its focused correction
to `1.0.0` passes all 41 fixtures. Static validation reports zero findings,
registry validation reports 33 successes, documentation snippets report 252
blocks, links report 325 internal/14 external/132 canonical pages, and the
desktop/mobile Playwright suite reports 19 passes with its desktop-only mobile
case skipped by design.

At this phase boundary, the five packages from the first upload have live
docs.rs pages and the remaining 28 exact builds are visibly queued. Exact
post-merge Pages deployment and complete docs.rs propagation remain external
closure gates; they are not claimed by this pre-merge source evidence.
