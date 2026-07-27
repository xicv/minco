---
id: M12-T02
title: Build the optional local developer workbench
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
  - tasks/M12/M12-T02-local-workbench.md
checks:
  - cargo test -p minco-workbench -p cargo-minco --all-features --locked
  - cargo clippy -p minco-workbench -p cargo-minco --all-targets --all-features --locked -- -D warnings
  - cargo minco workbench --check
---

## Goal

Build an optional local dashboard from existing JSON/read interfaces for
OpenAPI exploration, application/resource graphs, local process status,
migrations/seeds, request traces, cost/deployment previews, tasks, and Feedback.

## Acceptance

- the workbench is local-only and opt-in;
- it reuses stable read models rather than creating a second application graph;
- secret/redaction and response bounds match the MCP/CLI contracts;
- static assets add no default facade dependency;
- accessibility and small-screen behavior are tested.

## Non-goals

- a production admin UI;
- hosted telemetry collection;
- performing deployment or database writes by default.
