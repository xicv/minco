---
id: M14-T14
title: Automate release-bound AI skill freshness
milestone: M14
status: active
priority: high
area: agent/release
depends_on: [M14-T13]
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
  - crates/minco-cli/src/agent_cmd.rs
  - crates/minco-cli/tests/agent_skills.rs
  - crates/minco-cli/tests/plugin_cli.rs
  - crates/minco-cli/assets/agent/**
  - extensions/**/minco-plugin.json
  - plugins/**/minco-plugin.json
  - infra/aws/generated/plan.json
  - proofs/realtime-appsync/aws-handler/Cargo.lock
  - docs/adoption/1.2.0-to-1.2.1.md
  - docs/adoption/incremental-adoption.md
  - docs/development/publishing.md
  - docs/development/using-minco-crate.md
  - docs/reference/generated/**
  - docs/vision/minco-framework-definition.md
  - docs-site/1.2.1/**
  - docs-site/next/**
  - docs-site/release.json
  - docs-site/tests/**
  - docs-site/versions.md
  - roadmap/**
  - scripts/ci/hosted-essential.sh
  - scripts/quality.sh
  - scripts/test/agent_workflows.py
  - scripts/test/candidate_qualification.py
  - scripts/test/hosted_ci_policy.py
  - scripts/test/operational_evidence.py
  - scripts/test/repository_truth.py
  - scripts/validate_static.py
  - tasks/M14/M14-T14-agent-skill-release-freshness.md
  - verification/**
checks:
  - cargo test -p cargo-minco --test agent_skills --locked
  - uv run --locked python scripts/test/agent_workflows.py --check-output verification/agent-workflows.json
  - uv run --locked python scripts/test/repository_truth.py
  - uv run --locked python scripts/validate_static.py
  - scripts/docs/generate-reference.sh --check
  - bash scripts/ci/hosted-essential.sh
  - scripts/ci/local-release.sh
---

# M14-T14 - Automate release-bound AI skill freshness

## Goal

Ship a compatible lock-step 1.2.1 patch whose packaged Codex and Claude skills
cover the complete 1.2 product boundary, and make every later release fail
closed when its changelog, feature-to-skill mapping, documentation identifiers,
skill instructions or deterministic projection receipt drift.

## Acceptance

- all eight canonical skills describe the relevant 1.2 browser/native,
  verified-upload, rich-mail, owned-local-service, release-bound evidence,
  topology-aware cost and Signal documentation workflows;
- the bundle carries cumulative, versioned feature coverage tied to exact
  changelog release-section digests;
- every top-level release-note bullet is covered by at least one stable feature
  record, every mapped skill contains its declared marker and every feature
  references existing versioned documentation;
- Rust bundle evaluation and repository static validation reject malformed,
  stale, incomplete or misleading coverage;
- deterministic Codex/Claude workflow qualification supports create and exact
  check modes and is an ordinary quality/release gate;
- the complete 33-package family advances together to an unpublished 1.2.1
  candidate before exact-source qualification, tagging and publication; and
- publication, Pages, docs.rs and any provider/runtime evidence remain separate
  claims.

## Non-goals

- changing skill names, trigger semantics, projection paths or mutation
  authority;
- adding a hosted agent runtime, mutable skill download, always-on control
  plane or model-quality claim;
- changing live AWS resources, treating local/hosted checks as provider proof,
  or rewriting the immutable 1.2.0 tag or crate archives.

## Release boundary

Published 1.2.0 is immutable at
`48df3cc0ebb8990061b60d9383ced63532941079`. Packaged skill byte changes must
therefore ship as a new lock-step patch rather than as unversioned source under
the already-published package version. The 1.2.1 candidate remains
`unpublished` until separately qualified and uploaded from one exact reviewed
tree.
