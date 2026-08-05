---
id: M12-T01
title: Add bounded project read models and a local read-only Minco MCP server
milestone: M12
status: complete
priority: high
area: ai/mcp
depends_on: [M10-T03, M11-T10]
operations: []
owned_paths:
  - Cargo.lock
  - Cargo.toml
  - crates/minco-project-view/**
  - crates/minco-mcp/**
  - crates/minco-cli/**
  - docs/adrs/**
  - docs/how-to/**
  - docs/reference/**
  - roadmap/tasks.mmd
  - tasks/M12/M12-T01-local-read-only-mcp.md
  - verification/adoption-measurements.json
  - verification/deep-review.json
  - verification/publish-validation.json
  - verification/repository-truth.toml
  - verification/rust-dependency-hygiene.json
  - verification/source-manifest.json
  - verification/static-validation.json
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

## Evidence

Implementation and local qualification were completed on 2026-08-05
in the isolated `minco-task-m12-t01` JJ workspace:

- `minco-project-view` defines schema-versioned identity, provenance, graph,
  raw and semantic task status, six independent evidence lanes, redacted
  configuration, migration and seed catalogs, deployment and cost projections,
  task readiness, Feedback metadata, diagnostics and derived summaries;
- its reader accepts only explicit canonical roots, verifies declared
  allowlisted sources, rejects symlinks component by component (including
  dangling optional evidence), globally budgets directory entries, enforces
  per-file, aggregate-input, text, node, edge and response limits, and includes
  every read source in the provenance digest;
- `minco-mcp` uses `rmcp 3.1.0` with only macros, server and stdio transport
  features, exposes exactly six sorted read-only/non-destructive/idempotent/
  closed-world tools, rejects unknown arguments and path injection, limits
  newline-delimited client messages to 256 KiB, and budgets the complete SDK
  response envelope within the 2 MiB response limit;
- `cargo minco mcp --check --json` reported schema version 1, read-only stdio,
  zero listening sockets, explicit-root serving, all seven configured limits,
  104 source files, 126 nodes, 314 edges and the exact six-tool catalog;
- the CLI refuses live serving without an explicit `--root`, reserves stdout
  for protocol traffic, and its child-process integration test negotiates MCP
  `2026-07-28` and lists the exact tool catalog;
- targeted locked Clippy and tests passed for `minco-project-view`,
  `minco-mcp` and `cargo-minco`; all seven ProjectView boundary tests, five MCP
  catalog/transport tests and three CLI MCP tests passed;
- both new package archives contain their manifests, license files, README,
  source and tests; publish-policy validation reported 31 publishable packages,
  zero errors and zero warnings. Direct multi-package `cargo package` cannot
  resolve unpublished internal `0.7.0` dependencies from crates.io, while the
  repository's coordinated publish workflow remains the authoritative archive
  verification path; and
- generated package, CLI and diagnostic references are current. No AWS,
  database, network listener, hosted workflow, deployment, release, registry,
  documentation-site, push or merge mutation was performed by this task.

The separately owned truth layer then corrected the README package-count
marker and made the official-plugin dependency-budget fixture distinguish the
one new official plugin from the two new tooling packages. On that combined
local tree, complete `./scripts/quality.sh` passed repository truth, generated
reference, publish/release policy, static and deep review, compiler, Clippy,
workspace tests and Rustdoc, generated PostgreSQL and SQLite applications,
Feedback and documentation browser suites, dependency policy and audit,
secret scanning, and terminal source-manifest verification. Existing deep
review warnings in untouched Rust and SQL sources remain unchanged; configured
PostgreSQL/provider-backed tests retain their explicit ignored/not-run state.
