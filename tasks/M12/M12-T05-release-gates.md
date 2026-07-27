---
id: M12-T05
title: Pass security recovery load and documentation release gates
milestone: M12
status: planned
priority: critical
area: release/qualification
depends_on: [M12-T04]
operations: []
owned_paths:
  - scripts/**
  - quality.toml
  - verification/**
  - docs/**
  - .github/workflows/**
  - tasks/M12/M12-T05-release-gates.md
checks:
  - ./scripts/quality.sh
  - npm run --prefix plugins/minco-plugin-feedback test:browser
  - scripts/test/e2e.sh
  - scripts/dev/rustack-smoke.sh
  - scripts/release/publish.sh --skip-quality
  - uv run --locked python scripts/source_manifest.py --check
---

## Goal

Qualify the exact 1.0 candidate across compiler, conformance, security,
recovery, load, documentation, package, local/provider, and bounded live-AWS
gates without converting unavailable evidence into a pass.

## Acceptance

- every mandatory command is PASS, FAIL, BLOCKED, or NOT RUN with exact output
  and source identity;
- restore/rollback and migration recovery are rehearsed within explicit data
  boundaries;
- API/worker load includes connection, queue, cost, and artifact measurements;
- documentation journeys and external consumer fixtures pass;
- no unresolved critical/high security finding or silent waiver remains.

## Non-goals

- crate upload or tag creation;
- unlimited production load testing;
- treating emulator proof as real-AWS proof.
