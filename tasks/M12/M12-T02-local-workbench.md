---
id: M12-T02
title: Build the optional local developer workbench and project views
milestone: M12
status: planned
priority: medium
area: ai/workbench
depends_on: [M12-T01]
operations: []
owned_paths:
  - crates/minco-workbench/**
  - crates/minco-cli/**
  - docs/how-to/**
  - docs/reference/**
  - scripts/test/workbench_browser.sh
  - tasks/M12/M12-T02-local-workbench.md
checks:
  - cargo test -p minco-workbench -p cargo-minco --all-features --locked
  - cargo clippy -p minco-workbench -p cargo-minco --all-targets --all-features --locked -- -D warnings
  - cargo minco workbench --check --json
  - scripts/test/workbench_browser.sh
---

## Goal

Build an optional local dashboard and deterministic export surface from the
M12-T01 `ProjectView` for OpenAPI exploration, application/resource graphs,
feature and task progress, local process status, migrations/seeds, request
traces, cost/deployment previews, evidence lanes, accessible narration and
Feedback.

## Acceptance

- the workbench is local-only and opt-in;
- it reuses stable read models rather than creating a second application graph;
- `--check`, `export --format json|mermaid|static`, and loopback-only `serve`
  preserve the ADR-0030 read/write and evidence boundaries;
- export accepts only a new, project-relative, non-symlink destination outside
  canonical inputs, publishes atomically and never replaces unrelated content;
- `serve` binds directly to loopback, rejects non-loopback `Host` values and
  cross-origin browser access, enables no permissive CORS, serves only local
  assets under a restrictive Content Security Policy and marks project-view
  responses `Cache-Control: no-store`;
- visual progress retains raw status, explains derived totals and keeps source,
  local, hosted, deployment, runtime and review/UAT evidence separate;
- diagrams, keyboard navigation, screen-reader structure, accessible text and
  explicit client-side read-aloud controls cover desktop and small screens;
- the Minco repository and separately authorized first-party application
  evidence consume the same schema before an adapter boundary is frozen;
- secret/redaction and response bounds match the MCP/CLI contracts;
- static assets add no default facade dependency;
- accessibility and small-screen behavior are tested.

## Non-goals

- a production admin UI;
- hosted telemetry collection;
- a text-to-speech provider, stored voice data or generated audio;
- performing deployment or database writes by default.
