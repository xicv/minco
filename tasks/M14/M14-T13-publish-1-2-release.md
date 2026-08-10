---
id: M14-T13
title: Publish and promote the Minco 1.2.0 release
milestone: M14
status: active
priority: critical
area: release/1.2
depends_on: [M14-T03, M14-T06, M14-T07, M14-T08, M14-T11, M14-T12]
operations: []
owned_paths:
  - CHANGELOG.md
  - .github/workflows/publish-crates.yml
  - README.md
  - CODEX_HANDOFF.md
  - PUBLISHING.md
  - quality.toml
  - REVIEW_STATUS.md
  - VERIFICATION.md
  - docs/development/publishing.md
  - docs/development/release-qualification.md
  - docs/adoption/1.1.0-to-1.2.0.md
  - docs-site/**
  - crates/minco-cli/src/delivery_evidence.rs
  - roadmap/**
  - scripts/release/candidate_gate_runner.py
  - scripts/release/candidate_qualification.py
  - scripts/source_manifest.py
  - scripts/test/candidate_qualification.py
  - scripts/test/operational_evidence.py
  - scripts/test/repository_truth.py
  - tasks/M14/M14-T02-promote-1-1-publication.md
  - tasks/M14/M14-T13-publish-1-2-release.md
  - verification/**
checks:
  - uv run --locked python scripts/test/candidate_qualification.py
  - uv run --locked python scripts/test/repository_truth.py
  - uv run --locked python scripts/validate_static.py
  - uv run --locked python scripts/validate_publish.py --check-registry --require-registry
  - uv run --locked python scripts/source_manifest.py --check
  - scripts/release/qualify-candidate.sh
  - scripts/ci/local-release.sh
  - bash scripts/ci/hosted-essential.sh
---

# M14-T13 - Publish and promote the Minco 1.2.0 release

## Goal

Publish every existing crate in the compatible 33-package Minco family at
1.2.0 from one exact reviewed source tree, create the immutable tag and GitHub
release, then promote the already versioned 1.2.0 documentation as stable.

## Acceptance

- the release notes cover every merged post-1.1 tranche: mobile-neutral HTTP,
  verified object uploads, rich mail, owned local services, release-bound
  feedback and handover, topology-aware cost/evidence, the Signal website, and
  the local-first release boundary;
- all scan blockers are fixed and regression-covered on the exact candidate;
- candidate records use the current 1.2 release-series names and bind the
  current verified source manifest;
- authoritative local release qualification, the bounded clean-Linux check,
  package dry runs and exact-version registry preflight pass before tagging;
- lightweight tag `v1.2.0`, the GitHub release and the 33 crates.io uploads bind
  the same exact candidate commit, with partial publication resumed only from
  a verified registry complement;
- repository truth, README, changelog and stable documentation are promoted
  only after every exact 1.2.0 crate is present and non-yanked; and
- Pages and docs.rs are verified independently without implying an AWS
  application deployment or a production SLO.

## Non-goals

- creating, changing or deleting live AWS application resources;
- completing M14-T10's unavailable exact hosted performance or current
  live-provider evidence by relabelling local or historical results;
- implementing the planned M14-T09 object-upload profiles;
- changing crate ownership, adding a package, weakening release gates, or
  publishing from an unreviewed working tree; or
- treating package publication, documentation deployment or local runtime
  evidence as production deployment authority.

## Release boundary

This task starts from exact merged main
`5fb5bb45ec1f763391f20d6aceaf45f43848edfc`. Version 1.2.0 has 33 existing
publishable packages and no first-publication package. The initial
registry-connected validator reached all 33 records and found every exact
1.2.0 version absent. All 33 exact 1.1.0 docs.rs package URLs returned HTTP 200,
closing M14-T02's historical propagation gate.

The candidate source change remains `active` until it is reviewed, merged,
qualified from exact main, tagged and published. Machine-generated load,
recovery and aggregate gate receipts are excluded narrowly from the source
digest and carry the exact manifest digest instead; raw logs remain beneath
ignored `target/minco/`. Publication and documentation evidence will be added
only in a post-publication truth change so the immutable tag is never rewritten.

The M14-T10 performance baseline stays `NOT RUN`, its current provider profile
records no contact, and historical provider evidence stays stale. These are
truthful bounded release limitations, not silently converted passes. No live
AWS application operation is authorised by this task.
