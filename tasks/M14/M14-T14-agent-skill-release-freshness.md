---
id: M14-T14
title: Automate release-bound AI skill freshness
milestone: M14
status: complete
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
  - docs/reference/compatibility.md
  - docs/reference/generated/**
  - docs/reference/supported-matrix.md
  - docs/vision/minco-framework-definition.md
  - docs-site/1.2.1/**
  - docs-site/next/**
  - docs-site/.vitepress/config.mts
  - docs-site/index.md
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
- the complete 33-package family advances together through exact-source
  qualification, immutable tagging and independently verified publication; and
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

Published 1.2.0 remains immutable at
`48df3cc0ebb8990061b60d9383ced63532941079`. The packaged skill byte changes
therefore shipped as the new lock-step `v1.2.1` patch at
`5f329ebbabef2840b01f10743f8dbb25a0b0dbe4`, rather than altering already
published archives. Exact registry publication is retained separately from
Pages, docs.rs, provider, runtime and production evidence. This task is
complete after promotion PR #141 passed exact-head clean-Linux run `31383722610`
at `681fd11bf078fdd4c0f8eb7a26f0703ca3f7e4b4`, merged as exact tree
`2c0cb03598f879ae80cf5f60e8d106a7a910914f` in main commit
`140c7278c9c7f60cb7ce3be949583f17f0d71a17`, and merged-main Pages run
`31384082079` passed. Cache-busted live checks returned HTTP 200 for the root,
frozen `/1.2.1/` manual and versions page, and every one of the 33 exact 1.2.1
docs.rs rustdoc routes returned HTTP 200. No live AWS application resource was
created, changed or deleted, and performance remains `NOT RUN` rather than a
production SLO.
