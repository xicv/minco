---
id: M14-T23
title: Prepare the Minco 1.5.0 candidate
milestone: M14
status: complete
priority: high
area: release/documentation/ai
depends_on: [M14-T22]
operations: []
owned_paths:
  - CHANGELOG.md
  - CODEX_HANDOFF.md
  - Cargo.lock
  - Cargo.toml
  - PUBLISHING.md
  - README.md
  - REVIEW_STATUS.md
  - VERIFICATION.md
  - crates/minco-cli/assets/agent/**
  - crates/minco-cli/src/agent_cmd.rs
  - crates/minco-cli/src/delivery_evidence.rs
  - crates/minco-cli/src/handover_cmd.rs
  - docs-site/.vitepress/config.mts
  - docs-site/1.5.0/**
  - docs-site/next/getting-started/installation.md
  - docs-site/next/guides/agent-development.md
  - docs-site/next/reference/testing.md
  - docs-site/release.json
  - docs-site/tests/**
  - docs-site/versions.md
  - docs/adoption/1.4.0-to-1.5.0.md
  - docs/adoption/incremental-adoption.md
  - docs/development/publishing.md
  - docs/development/using-minco-crate.md
  - docs/reference/compatibility.md
  - docs/reference/generated/**
  - docs/reference/supported-matrix.md
  - docs/research/minimal-cost-framework-review-2026-08.md
  - docs/vision/minco-framework-definition.md
  - examples/plugins/third-party-minimal/Cargo.lock
  - extensions/**/minco-plugin.json
  - infra/aws/generated/plan.json
  - infra/aws/generated/template.yaml
  - plugins/**/minco-plugin.json
  - plugins/minco-plugin-payments-waffo/README.md
  - plugins/minco-plugin-payments-waffo/agent/**
  - proofs/realtime-appsync/aws-handler/Cargo.lock
  - quality.toml
  - roadmap/tasks.mmd
  - scripts/source_manifest.py
  - scripts/release/candidate_qualification.py
  - scripts/test/candidate_qualification.py
  - scripts/test/operational_evidence.py
  - scripts/test/quality_assurance.py
  - scripts/test/release_identity.py
  - scripts/test/repository_truth.py
  - scripts/validate_static.py
  - scripts/validate_operational_evidence.py
  - tasks/M14/M14-T23-prepare-1-5-release.md
  - verification/1.5-performance-baseline.json
  - verification/1.5-candidate-load.json
  - verification/agent-workflows.json
  - verification/deep-review.json
  - verification/deployment-assurance.toml
  - verification/operational-evidence-validation.json
  - verification/performance-policy.toml
  - verification/provider-evidence.toml
  - verification/quality-assurance-policy.toml
  - verification/quality-assurance.json
  - verification/publish-validation.json
  - verification/release-identity.json
  - verification/repository-truth.toml
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - cargo generate-lockfile
  - cargo test -p cargo-minco --test agent_skills --locked
  - uv run --locked python scripts/test/agent_workflows.py --check-output verification/agent-workflows.json
  - uv run --locked python scripts/test/repository_truth.py
  - uv run --locked python scripts/validate_static.py
  - scripts/docs/generate-reference.sh --check
  - scripts/docs/build.sh
  - scripts/docs/check-links.sh
  - scripts/docs/check-snippets.sh
  - scripts/docs/test-browser.sh
  - scripts/quality.sh
  - scripts/ci/local-release.sh
  - uv run --locked python scripts/source_manifest.py --check
---

# M14-T23 - Prepare the Minco 1.5.0 candidate

## Goal

Prepare one truthful lock-step `1.5.0` candidate from the exact merged
framework-improvement baseline. Freeze current documentation, update every
version-matched portable AI skill, and bind the measured-assurance,
golden-topology cost-regression and typed side-effect-fake tranches to one
reviewable candidate without tagging, publishing or deploying it.

## Acceptance

- all 34 publishable packages and official plugin descriptors advance in
  lock-step to `1.5.0`, with no new package or ownership boundary;
- Cargo-generated lockfiles retain the declared dependency ranges while
  refreshing only compatible transitive patch releases;
- the changelog and 1.4.0-to-1.5.0 guide distinguish additive public fake APIs
  from provider-free quality and cost evidence;
- `docs-site/1.5.0/` is a complete frozen copy of the current manual and its
  testing reference teaches every official fake and the measured local gate;
- all nine packaged skills remain byte-identical across Codex and Claude and
  collectively teach every 1.5.0 release feature through cumulative,
  version-matched coverage;
- model-driven application evaluation and human-review effort remain
  explicitly `NOT RUN`, rather than inferred from deterministic skill checks;
- generated references, package archives, repository truth and evidence
  records bind one candidate source tree;
- exact local quality and release qualification pass before any later tag or
  publication request; and
- hosted Linux, registry, docs.rs, Pages, live-provider, deployment, runtime
  and production evidence remain separate, truthful states.

## Non-goals

- adding another framework feature, provider capability, package, workflow,
  scheduler, poller, fixed compute resource or always-on control plane;
- changing serialized Plan IR, existing CLI names or production adapter
  selection;
- claiming application-specific agent outcome or human-review evidence without
  actually running and reviewing a model-driven application experiment;
- contacting AWS, Waffo, crates.io or another live provider; or
- creating `v1.5.0`, a GitHub release, a crates.io upload, a deployment or a
  production mutation.

## Recovery and workspace

The task did not exist when release preparation was requested. Its dedicated
JJ workspace was bootstrapped directly from exact merged `main`
`ef7c3e30bebcae162d0c145ed4d9b6ba94cfc2f9` at
`/private/tmp/minco-task-m14-t23`. The stale detached primary checkout and the
unrelated `task-m12-t09` workspace are not used for release mutation.

## Evidence

Complete for the bounded candidate-preparation scope. The architecture audit
found no additional safe implementation that passed the deletion test. The
only unfulfilled P2 item requires an actual Codex/Claude application run and
human review; deterministic scenario schema work alone would be shallow and
would not qualify that outcome. This candidate therefore retains model and
review evidence as `NOT RUN` and packages only the already merged, testable
feature set.

The exact local macOS source passed `./scripts/quality.sh` and, from an empty JJ
child of the source change, `./scripts/ci/local-release.sh`. The release gate
covered pinned nextest, coverage, mutation and SemVer assurance; all 34 package
archive dry-runs and selected unpacked-archive consumers; SAM validation;
native Lambda and worker builds; owned PostgreSQL and Rustack runtime checks;
and Orders E2E. The canonical local receipt records 126 nextest tests plus one
doctest, 85.65% line and 82.01% function coverage, 43 caught viable mutants
with zero misses or timeouts, all 34 SemVer comparisons, 80 successful local
API requests and 1,000 successful worker messages. These measurements are not
a production SLO.

No tag, GitHub release, crates.io upload, Pages deployment, live AWS/Waffo
contact or production mutation occurred. Exact-head clean-Linux review remains
a separate PR gate; hosted performance and live-provider evidence remain
`NOT RUN` or absent.
