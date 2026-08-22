---
id: M14-T41
title: Release Minco 1.10.0 Ticketing support entry
milestone: M14
status: complete
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

Released on 2026-08-21 local time. Candidate commit
`fcf0cf88d541bb1f93e1b89300fe0c2de0020f27` with source digest
`258cb3c4e31138b7aa9b269ad9e198799ab1c9e63dc6b6f5b51c1b92f97e67c0`
passed the uninterrupted full local release qualification, including measured
assurance, 34 established-package SemVer comparisons, the two-new-package
boundary, all 36 archives and external consumers, deterministic artifacts,
owned isolated PostgreSQL/Rustack, and Orders E2E. PR #180 merged the same Git
tree as `2075b60b8fe86c04d3c8289d71eb8293a39fc378`; exact-main hosted run
`32392228228` passed before immutable tag `v1.10.0` was pushed.

Manual authenticated publication uploaded 32 packages before crates.io returned
429 with retry time `2026-08-20T16:44:16Z`. Independent registry validation
proved exactly four packages missing. A four-package partial resume published
Feedback, the new Ticketing crate, and `minco` before a second 429 with retry
time `2026-08-20T16:46:16Z`; a final `cargo-minco`-only resume passed.
`verification/1.10-published-release-validation.json` reports status `ok`, zero
errors and warnings, and all 36 exact versions present and non-yanked. The
GitHub release is published at
<https://github.com/xicv/minco/releases/tag/v1.10.0>.

The two new crate names crossed ownership. On 2026-08-21 an authenticated,
read-before-write crates.io API reconciliation found no existing GitHub
trusted-publisher entry for either crate, then created and read back exactly one
reviewed entry for each: IDs `17250` (`minco-interaction`) and `17251`
(`minco-plugin-ticketing`), repository `xicv/minco`, workflow
`publish-crates.yml`, environment `crates-io`. The existing Cargo credential
was consumed in memory and was not printed or committed.

Exact merged-main Pages run `32476082843` passed for
`9e9013bca378716c8131c23b4d547883231f7f1c`; the versioned
<https://xicv.github.io/minco/1.10.0/> manual returned HTTP 200 and identified
`1.10.0` as stable. All 36 exact versioned docs.rs library routes independently
returned HTTP 200. These checks close the release task without claiming an AWS
or other live application-provider request, application deployment or
production mutation.
