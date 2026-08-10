---
id: M14-T02
title: Promote published Minco 1.1.0 repository and documentation truth
milestone: M14
status: complete
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

Source-promotion PR
[`#127`](https://github.com/xicv/minco/pull/127) passed exact-head hosted
qualification run
[`31075075306`](https://github.com/xicv/minco/actions/runs/31075075306) at
`2a7cf87739148ba185f227b15d85843b31797463`. It merged as exact main
`828fdb61557cb5135921a8067b2eb93d17ebc2bd`; merge tree
`b351f3062741c1baa49b3be9d565934b1ead6075` exactly matched the reviewed PR
tree. Pages run
[`31075322828`](https://github.com/xicv/minco/actions/runs/31075322828)
built and deployed from that merge. The hosted site then passed all 19
applicable desktop/mobile Playwright checks, with its desktop-only
mobile-viewport case skipped by design.

At publication time, the five packages from the first upload had live docs.rs
pages and the remaining 28 exact builds were visibly queued without a build
failure. Complete docs.rs propagation was retained as the final external
closure gate rather than inferred from the queue state.

On 2026-08-10, a fresh HEAD request to every package URL derived from the
checked 33-package publication order returned HTTP 200 for exact version
`1.1.0`. This closes the external docs.rs propagation gate without changing the
immutable tag, release archives, registry records or deployed documentation.
