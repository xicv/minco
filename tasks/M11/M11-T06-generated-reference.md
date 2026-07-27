---
id: M11-T06
title: Generate feature plugin package CLI and diagnostic reference
milestone: M11
status: planned
priority: high
area: documentation/reference
depends_on: [M11-T01, M11-T02, M11-T04]
operations: []
owned_paths:
  - crates/minco-cli/**
  - docs/reference/**
  - scripts/docs/**
  - scripts/test/repository_truth.py
  - README.md
  - tasks/M11/M11-T06-generated-reference.md
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
