---
id: M12-T01
title: Add a local read-only Minco MCP server
milestone: M12
status: planned
priority: high
area: ai/mcp
depends_on: [M10-T03, M11-T06]
operations: []
owned_paths:
  - crates/minco-mcp/**
  - crates/minco-cli/**
  - docs/adrs/**
  - docs/how-to/**
  - docs/reference/**
  - tasks/M12/M12-T01-local-read-only-mcp.md
checks:
  - cargo test -p minco-mcp -p cargo-minco --all-features --locked
  - cargo clippy -p minco-mcp -p cargo-minco --all-targets --all-features --locked -- -D warnings
  - cargo minco mcp --check
---

## Goal

Expose the stable application graph, operation explanations, ownership,
redacted configuration, migration/seed state, deployment/cost plans, task
readiness, quality/release evidence, and Feedback context through a local-only,
read-only-by-default MCP server.

## Acceptance

- the server binds locally and requires an explicit project root;
- tools expose bounded schema-versioned read models;
- credentials, secret values, tokens, service instances, arbitrary files, and
  shell execution are unreachable;
- path and response-size limits fail closed;
- any future write tool requires a separately reviewed explicit local grant.

## Non-goals

- a hosted Minco service;
- remote repository access;
- default write capabilities.
