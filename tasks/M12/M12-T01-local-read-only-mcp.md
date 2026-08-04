---
id: M12-T01
title: Add bounded project read models and a local read-only Minco MCP server
milestone: M12
status: planned
priority: high
area: ai/mcp
depends_on: [M10-T03, M11-T10]
operations: []
owned_paths:
  - crates/minco-project-view/**
  - crates/minco-mcp/**
  - crates/minco-cli/**
  - docs/adrs/**
  - docs/how-to/**
  - docs/reference/**
  - tasks/M12/M12-T01-local-read-only-mcp.md
checks:
  - cargo test -p minco-project-view -p minco-mcp -p cargo-minco --all-features --locked
  - cargo clippy -p minco-project-view -p minco-mcp -p cargo-minco --all-targets --all-features --locked -- -D warnings
  - cargo minco mcp --check
---

## Goal

Define the bounded, schema-versioned `ProjectView` over existing authoritative
read models, then expose the stable application graph, operation explanations,
ownership, redacted configuration, migration/seed state, deployment/cost
plans, task readiness, quality/release evidence, and Feedback context through
a local-only, read-only-by-default MCP server.

## Acceptance

- `ProjectView` preserves raw statuses, explicit semantic mappings, source
  provenance and separate source/local/hosted/deployment/runtime/review lanes;
- aggregates are deterministic derived values and never become a second source
  of project progress;
- the initial server uses child-process stdio, opens no listening socket and
  requires an explicit canonical project root;
- tools expose bounded schema-versioned read models;
- credentials, secret values, tokens, service instances, arbitrary files, and
  shell execution are unreachable;
- traversal and unsafe symlink boundaries fail closed, and file, text, node,
  edge and total response-size limits are explicit;
- any future write tool requires a separately reviewed explicit local grant.

## Non-goals

- a hosted Minco service;
- a TCP, HTTP or other network MCP transport;
- remote repository access;
- rendering the Workbench UI or synthesizing audio;
- default write capabilities.
