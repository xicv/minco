---
id: M14-T39
title: Release Minco 1.9.0 traffic and compression controls
milestone: M14
status: complete
priority: critical
area: release/http/plan
depends_on: [M14-T38]
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
  - bash scripts/ci/hosted-essential.sh
  - cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
  - cargo test --workspace --all-targets --all-features --locked
  - scripts/test/generated_apps.sh
  - scripts/release/publish.sh
  - uv run --locked python scripts/source_manifest.py --check
owned_paths:
  - CHANGELOG.md
  - CODEX_HANDOFF.md
  - Cargo.lock
  - Cargo.toml
  - PUBLISHING.md
  - README.md
  - REVIEW_STATUS.md
  - SECURITY.md
  - VERIFICATION.md
  - crates/minco-cli/assets/agent/**
  - crates/minco-cli/src/agent_cmd.rs
  - crates/minco-cli/src/delivery_evidence.rs
  - crates/minco-cli/src/handover_cmd.rs
  - docs-site/.vitepress/config.mts
  - extensions/**/minco-plugin.json
  - plugins/**/minco-plugin.json
  - docs-site/1.9.0/**
  - docs-site/next/**
  - docs-site/release.json
  - docs-site/versions.md
  - docs/adoption/1.8.0-to-1.9.0.md
  - docs/adoption/incremental-adoption.md
  - docs/development/publishing.md
  - docs/development/using-minco-crate.md
  - docs/reference/compatibility.md
  - docs/reference/generated/**
  - docs/reference/supported-matrix.md
  - docs/vision/minco-framework-definition.md
  - roadmap/tasks.mmd
  - scripts/source_manifest.py
  - crates/minco-cli/src/delivery_evidence.rs
  - crates/minco-cli/src/handover_cmd.rs
  - scripts/test/operational_evidence.py
  - scripts/test/repository_truth.py
  - scripts/validate_operational_evidence.py
  - tasks/M14/M14-T39-release-1-9-0.md
  - verification/1.9-performance-baseline.json
  - verification/agent-workflows.json
  - verification/deep-review.json
  - verification/deployment-assurance.toml
  - verification/operational-evidence-validation.json
  - verification/performance-policy.toml
  - verification/provider-evidence.toml
  - verification/publish-validation.json
  - verification/quality-assurance-policy.toml
  - verification/quality-assurance.json
  - verification/repository-truth.toml
  - verification/source-manifest.json
  - verification/static-validation.json
---

# M14-T39 - Release Minco 1.9.0 traffic and compression controls

## Goal

Release the additive Minco `1.9.0` family: the API Gateway HTTP traffic policy
and hardened negotiated response compression merged as PR #171, plus the
publish-workflow repair from PR #173. Keep the complete 34-package lock-step
inventory, the cumulative agent feature coverage, the versioned documentation
snapshot and every release evidence receipt current. Tag, publish and verify
through the repository's guarded OIDC boundary without any AWS application
operation.

## Acceptance

- the workspace version moves to `1.9.0` once in the root manifest and every
  internal path dependency carries the same explicit version;
- the changelog describes the release and every top-level bullet maps to a
  stable feature, current versioned documentation and teaching skills;
- the agent bundle projection and its deterministic receipt stay current;
- `docs-site` gains the frozen `1.9.0` snapshot, the candidate release state
  and the version index, and `next` carries the new traffic and compression
  guidance;
- adoption, compatibility and reference documentation identify the `1.9.0`
  boundary;
- the full local gate battery and publish dry run pass at the exact release
  source;
- the release merges, passes the exact-main hosted compatibility run, and is
  tagged `v1.9.0` at that qualified SHA;
- authentication-only and guarded publication dispatches pass, and the
  registry is independently verified for all 34 exact `1.9.0` versions;
- the GitHub release is published from the tag; and
- docs.rs and stable Pages evidence for `1.9.0` is verified or explicitly
  pending.

## Non-goals

- changing any public Rust API beyond the already merged additive surface;
- new packages, ownership changes or first-publication boundaries;
- live AWS, provider, deployment or production evidence; and
- skipping or weakening any release gate.

## Evidence

Released on 2026-08-19. The candidate change (squash merge
`8922aab5c9ed6770d8df7f5d906f768152d3e06c`, PR #176) passed the full local
battery at its exact source: hosted-essential end-to-end, repository truth
53/53, workspace fmt/Clippy/tests/rustdoc/doc, documentation
snippet/link/build/browser checks, generated apps, the cumulative
changelog-to-skill coverage (4/4) with a refreshed deterministic agent
workflow receipt, and the publication dry run whose external consumer install
printed `minco 1.9.0`.

Exact-main hosted compatibility run `32246605343` passed. Immutable tag
`v1.9.0` resolves to the merge commit. Authentication-only OIDC run
`32247017888` proved the short-lived crates.io boundary without upload.
Guarded publication run `32247061809` passed the first-publication refusal,
archive and consumer gates and uploaded the dependency-ordered 34-package
family. Independent registry validation reported status `ok` with zero errors
and warnings for all 34 exact `1.9.0` versions. The GitHub release is
published at <https://github.com/xicv/minco/releases/tag/v1.9.0>.

Stable Pages deploys through the sanctioned documentation workflow from this
change; the API documentation service's `1.9.0` builds were still queued when
this evidence was recorded and must be verified independently. Live AWS,
provider, deployment, hosted performance and content-safety evidence remain
`NOT RUN` and unclaimed.
