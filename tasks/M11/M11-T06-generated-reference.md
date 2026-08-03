---
id: M11-T06
title: Generate feature plugin package CLI and diagnostic reference
milestone: M11
status: complete
priority: high
area: documentation/reference
depends_on: [M11-T01, M11-T02, M11-T04]
operations: []
owned_paths:
  - CHANGELOG.md
  - crates/minco-cli/**
  - docs/reference/**
  - docs/adrs/0013-quality-and-update.md
  - docs/development/testing.md
  - scripts/docs/**
  - scripts/ci/hosted-essential.sh
  - scripts/quality.sh
  - scripts/test/hosted_ci_policy.py
  - scripts/test/repository_truth.py
  - README.md
  - tasks/M11/M11-T06-generated-reference.md
  - verification/adoption-measurements.json
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - scripts/docs/generate-reference.sh --check
  - uv run --locked python scripts/test/repository_truth.py
  - cargo minco --help
  - cargo minco plugin validate
---

## Goal

Derive package order, facade features, plugin/catalog fields, CLI help,
configuration/Plan schemas, and diagnostic codes from authoritative metadata
and make drift a deterministic quality failure.

## Acceptance

- generated files identify their authority and generator version;
- README links to generated reference and retains no competing exhaustive list;
- every public package has a current docs.rs link;
- stale CLI/config/plugin/diagnostic reference fails local and hosted quality;
- output is stable across clean runs.

## Non-goals

- generating tutorials or architectural rationale;
- treating generated docs as a runtime registry;
- hiding unsupported or incomplete provider evidence.

## Evidence

- Activated after M10-T04 merged because every declared dependency is complete;
  `cargo minco task ready --json` returned `[]` only because this task was still
  deliberately `planned`.
- Seven checked-in pages now cover 28 dependency-ordered public packages and
  versioned docs.rs links, 27 facade features, 16 catalog components, 98 public
  CLI help nodes, the composed configuration schema, the `DeploymentPlan`
  surface and reference paths, and 331 source-declared diagnostic codes.
- Every page identifies its authorities and generator schema. Canonical
  metadata digests make catalog/distribution value drift visible even when a
  displayed field shape does not change.
- `scripts/docs/generate-reference.sh --check`: passed with 7 current files.
- `uv run --locked python scripts/test/repository_truth.py`: 35 passed,
  including feature, catalog, distribution, configuration, diagnostic, CLI,
  byte-stability, whitespace normalization, secret-redaction, and input,
  output, and executable symlink-escape regressions.
- `uv run --locked python scripts/test/hosted_ci_policy.py`: 3 passed.
- `cargo minco --help`: passed; `cargo minco plugin validate --json`: returned
  the expected empty finding list.
- Documentation proof: 112 fenced snippets passed; VitePress production build
  and npm audits passed; 54 internal, 10 external, and 49 canonical links
  passed.
- Generation executes only the exact locked local `cargo-minco` binary and
  read-only repository commands. No AWS contact, deployment, promotion,
  rollback, tag, registry publication, or Pages publication occurred.
