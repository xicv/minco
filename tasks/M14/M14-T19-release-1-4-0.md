---
id: M14-T19
title: Release Minco 1.4.0 maintenance minor
milestone: M14
status: active
priority: high
area: release/docs/agent
depends_on: [M14-T18]
operations: []
owned_paths:
  - Cargo.toml
  - Cargo.lock
  - CHANGELOG.md
  - README.md
  - CODEX_HANDOFF.md
  - PUBLISHING.md
  - REVIEW_STATUS.md
  - VERIFICATION.md
  - quality.toml
  - crates/**/Cargo.toml
  - extensions/**/Cargo.toml
  - plugins/**/Cargo.toml
  - examples/**/Cargo.toml
  - examples/plugins/third-party-minimal/Cargo.lock
  - proofs/realtime-appsync/aws-handler/Cargo.lock
  - extensions/**/minco-plugin.json
  - plugins/**/minco-plugin.json
  - plugins/minco-plugin-payments-waffo/README.md
  - plugins/minco-plugin-payments-waffo/agent/**
  - infra/aws/generated/plan.json
  - crates/minco-cli/assets/agent/**
  - crates/minco-cli/src/agent_cmd.rs
  - crates/minco-cli/src/delivery_evidence.rs
  - crates/minco-cli/src/handover_cmd.rs
  - crates/minco-cli/tests/agent_skills.rs
  - crates/minco-cli/tests/plugin_cli.rs
  - docs/adoption/1.3.0-to-1.4.0.md
  - docs/adoption/incremental-adoption.md
  - docs/development/publishing.md
  - docs/development/using-minco-crate.md
  - docs/reference/compatibility.md
  - docs/reference/generated/**
  - docs/reference/supported-matrix.md
  - docs/vision/minco-framework-definition.md
  - docs-site/**
  - roadmap/**
  - scripts/test/agent_workflows.py
  - scripts/test/candidate_qualification.py
  - scripts/test/hosted_ci_policy.py
  - scripts/test/operational_evidence.py
  - scripts/test/repository_truth.py
  - scripts/source_manifest.py
  - scripts/validate_operational_evidence.py
  - scripts/validate_static.py
  - tasks/M14/M14-T19-release-1-4-0.md
  - verification/**
checks:
  - cargo test -p cargo-minco --test agent_skills --locked
  - cargo test -p cargo-minco --test plugin_cli --locked
  - uv run --locked python scripts/test/agent_workflows.py --check-output verification/agent-workflows.json
  - uv run --locked python scripts/test/repository_truth.py
  - uv run --locked python scripts/validate_static.py
  - uv run --locked python scripts/validate_publish.py --check-registry --require-registry
  - scripts/docs/generate-reference.sh --check
  - scripts/docs/check-snippets.sh
  - scripts/docs/check-links.sh
  - scripts/docs/test-browser.sh
  - scripts/ci/local-release.sh
  - uv run --locked python scripts/source_manifest.py --check
---

# M14-T19 - Release Minco 1.4.0 maintenance minor

## Goal

Publish one exact, additive `1.4.0` lock-step release containing the homepage
documentation improvements from M14-T17 and the language/package ecosystem
refresh from M14-T18. Freeze the complete current manual, update every packaged
AI skill and release-feature mapping, and retain separate proof for source
qualification, tag, registry, docs.rs, GitHub release and Pages publication.

## Acceptance

- all 34 publishable packages and official plugin descriptors advance in
  lock-step to `1.4.0`, with no new package or first-publication boundary;
- the changelog and 1.3.0-to-1.4.0 guide describe the homepage, dependency and
  toolchain changes without claiming an API, topology or provider expansion;
- `docs-site/1.4.0/` is a complete frozen copy of the current manual, and the
  root, version index and installation commands distinguish candidate from
  published state;
- every packaged Codex and Claude skill uses version-matched 1.4.0 document
  identities and cumulative release-feature coverage maps every 1.4.0 note;
- generated references, package archives, repository truth, evidence ledgers
  and the source manifest bind one exact candidate tree;
- exact-tree local release qualification and clean-Linux compatibility pass
  before tagging or publication; and
- immutable tag, GitHub release, all exact crates.io records, all docs.rs
  routes and stable Pages deployment are verified independently after publish.

## Non-goals

- adding or changing a public Rust API, serialized Plan IR, plugin capability,
  provider implementation, deployment topology or compatibility boundary;
- publishing a new crate or changing crates.io ownership;
- creating AWS resources, deploying a Minco application, contacting Waffo, or
  converting local/hosted checks into provider or production-SLO evidence; or
- adding a task-specific workflow or bypassing the exact three-workflow policy.

## Recovery and workspace

The task did not exist when the release was requested, so its dedicated JJ
workspace was bootstrapped directly from exact merged `main`
`8ff0be93c11f6a36040aa7671ccb22c6ae731227` at
`/Users/xicao/Projects/minco-task-m14-t19`. The stale detached primary checkout
and the unrelated `task-m12-t09` workspace are not used for release mutation.

## Evidence

Candidate-focused verification on macOS passed the full all-target/all-feature
workspace tests, warning-denying Clippy, generated-reference checks, static and
operational validators, 38 browser scenarios with two desktop-inapplicable
skips, and all 34 exact-version registry-absence checks. The canonical source
manifest and operational receipt bind the final candidate tree.

The authoritative local release gate, clean-Linux compatibility, publication,
registry/docs.rs verification and Pages publication remain in progress at this
candidate stage. Performance remains `NOT RUN`; current live-provider evidence
is absent and must not be inferred from release gates.
