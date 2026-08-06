---
id: M14-T02
title: Promote published Minco 1.1.0 repository and documentation truth
milestone: M14
status: planned
priority: critical
area: release/1.1
depends_on: [M14-T01, M14-T04]
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
  - tasks/M14/M14-T02-promote-1-1-publication.md
  - verification/**
checks:
  - uv run --locked python scripts/validate_static.py
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

Planned. Publication and stable documentation evidence must not be written
before those external states exist.
