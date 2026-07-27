---
id: M9-T05
title: Add graph-driven cargo minco dev
milestone: M9
status: planned
priority: critical
area: developer-experience/runtime
depends_on: [M9-T02, M9-T03, M9-T04]
operations: []
owned_paths:
  - crates/minco-dev/**
  - crates/minco-cli/**
  - scripts/dev/**
  - infra/local/**
  - docs/adrs/**
  - docs/development/**
  - tasks/M9/M9-T05-graph-driven-dev.md
checks:
  - cargo test -p minco-dev -p cargo-minco --all-features --locked
  - cargo clippy -p minco-dev -p cargo-minco --all-targets --all-features --locked -- -D warnings
  - cargo minco dev --dry-run --json
  - python3 scripts/dev/test_topology.py
---

## Goal

Derive a deterministic `DevPlan` that starts only declared local dependencies,
optional migrations/seed profiles, the API, selected workers, and an optional
application-defined frontend command with labelled logs, readiness, signal
handling, and coordinated shutdown.

## Acceptance

- dry-run and JSON expose every service/process before startup;
- defaults do not contact AWS, reset data, run schedules, or seed implicitly;
- selected PostgreSQL/SQLite/Rustack services and ports are deterministic;
- child-process failure and termination leave no detached Minco process;
- API and worker readiness is visible.

## Non-goals

- a frontend framework;
- automatic local schedules;
- replacing provider fidelity checks with emulator success.
