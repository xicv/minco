---
id: M14-T43
title: Release Minco 1.11.0 contract-enforced request boundary
milestone: M14
status: active
priority: critical
area: release/contract/http/identity/agent/docs
depends_on: [M14-T41, M14-T42]
operations:
  - placeOrder
  - updateOrder
checks:
  - cargo minco contract check
  - cargo minco contract sync --check
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
  - scripts/release/publish.sh
  - uv run --locked python scripts/source_manifest.py --check
owned_paths:
  - Cargo.lock
  - Cargo.toml
  - CHANGELOG.md
  - CODEX_HANDOFF.md
  - PUBLISHING.md
  - README.md
  - REVIEW_STATUS.md
  - VERIFICATION.md
  - crates/**/Cargo.toml
  - crates/minco-cli/assets/agent/**
  - crates/minco-cli/src/agent_cmd.rs
  - crates/minco-cli/src/delivery_evidence.rs
  - docs-site/.vitepress/config.mts
  - docs-site/1.11.0/**
  - docs-site/next/**
  - docs-site/index.md
  - docs-site/release.json
  - docs-site/tests/**
  - docs-site/versions.md
  - docs/adoption/1.10.0-to-1.11.0.md
  - docs/adoption/incremental-adoption.md
  - docs/development/publishing.md
  - docs/development/using-minco-crate.md
  - docs/how-to/contract-request-validation.md
  - docs/reference/compatibility.md
  - docs/reference/generated/**
  - docs/reference/http-request-boundary.md
  - docs/reference/supported-matrix.md
  - docs/vision/minco-framework-definition.md
  - examples/**/Cargo.toml
  - examples/**/Cargo.lock
  - examples/orders/api/src/generated.rs
  - extensions/**/Cargo.toml
  - extensions/**/minco-plugin.json
  - plugins/**/Cargo.toml
  - plugins/**/minco-plugin.json
  - proofs/**/Cargo.toml
  - proofs/**/Cargo.lock
  - infra/aws/generated/plan.json
  - infra/aws/generated/template.yaml
  - quality.toml
  - roadmap/tasks.mmd
  - scripts/source_manifest.py
  - scripts/generate_bootstrap_artifacts.py
  - scripts/test/repository_truth.py
  - scripts/validate_static.py
  - tasks/M14/M14-T43-release-1-11-0.md
  - verification/1.9-performance-baseline.json
  - verification/1.11-candidate-load.json
  - verification/1.11-candidate-recovery.json
  - verification/1.11-candidate-release-gates.json
  - verification/agent-workflows.json
  - verification/deep-review.json
  - verification/deployment-assurance.toml
  - verification/operational-evidence-validation.json
  - verification/performance-policy.toml
  - verification/provider-evidence.toml
  - verification/publish-validation.json
  - verification/quality-assurance.json
  - verification/quality-assurance-policy.toml
  - verification/release-identity.json
  - verification/repository-truth.toml
  - verification/source-manifest.json
  - verification/static-validation.json
---

# M14-T43 - Release Minco 1.11.0 contract-enforced request boundary

## Goal

Release the additive Minco `1.11.0` lock-step family containing the reviewed
contract-enforced request boundary from M14-T42. Keep the complete 36-package
inventory, frozen versioned manual, stable website, adoption guidance and nine
portable AI skills current, then publish from one exact reviewed source tree.

## Acceptance

- all 36 publishable packages and official descriptors advance in lock-step to
  additive `1.11.0` without changing the reviewed request-boundary API;
- the changelog, upgrade guidance, compatibility references and frozen
  `1.11.0` manual describe generated validation, typed extraction, authorization,
  safe correlation IDs, body limits and timeouts without overstating business or
  provider enforcement;
- the packaged AI bundle teaches the new contract and HTTP boundary and its
  deterministic feature-coverage receipt is current;
- documentation snippets, links, build, browser journeys and generated
  references pass for both current and frozen documentation;
- full local quality and exact clean-source release qualification pass before
  transport;
- the exact reviewed tree merges, and the resulting exact main SHA passes the
  bounded clean-Linux compatibility workflow before tagging;
- immutable tag `v1.11.0` identifies that qualified main SHA;
- authentication-only trusted publishing and guarded OIDC publication pass;
- independent registry validation proves all 36 exact `1.11.0` versions are
  present and non-yanked before the GitHub release and stable docs are claimed;
  and
- Pages, docs.rs, registry, GitHub release and any provider/application runtime
  evidence remain separate truthful states.

## Non-goals

- changing the public request-boundary behavior already reviewed in M14-T42;
- adding packages, workflows, runtime plugin discovery or provider resources;
- live AWS, provider, deployment, database, migration or production mutation;
  and
- treating local, hosted-Linux, registry, Pages or docs.rs evidence as proof of
  another lane.

## Evidence

Release preparation started from merged `main`
`fc6483ccb42f86a7247dd65e1500716ed7132313`. PR #170 and the 1.10 publication
evidence closure PR #182 were already merged with exact-tree proof. The primary
workspace's unrelated `.mimosa` state was not modified.
