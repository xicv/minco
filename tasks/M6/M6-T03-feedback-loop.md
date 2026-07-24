---
id: M6-T03
title: Ship the official Feedback review-loop plugin
milestone: M6
status: complete
priority: high
area: plugins/feedback
depends_on: [M2-T01]
operations:
  - createFeedback
  - getClientFeedback
  - replyToFeedback
  - listDeveloperFeedback
  - getFeedbackAiContext
owned_paths:
  - plugins/minco-plugin-feedback/**
  - crates/minco-cli/src/feedback_cmd.rs
  - docs/architecture/feedback-loop.md
  - docs/architecture/capability-audit.md
  - tasks/M6/M6-T03-feedback-loop.md
checks:
  - uv run --with pyyaml python3 scripts/test/feedback_contract.py
  - node --check plugins/minco-plugin-feedback/assets/widget.js
  - cargo test -p minco-plugin-feedback --all-features --locked
  - cargo test -p cargo-minco --locked
  - MINCO_FEEDBACK_TEST_POSTGRES_URL='postgres://minco:minco@127.0.0.1:55432/minco_orders' cargo test -p minco-plugin-feedback --all-features --test persistence -- --nocapture
  - npm --prefix plugins/minco-plugin-feedback ci
  - npm --prefix plugins/minco-plugin-feedback run test:browser
  - npm --prefix plugins/minco-plugin-feedback run test:browser:repeat
---

## Goal

Provide an embeddable, frontend-agnostic client feedback loop with screenshot and
voice attachments, optional transcription, durable threaded clarification,
notifications/audit/events, explicit workflow states, and deterministic AI-ready
handoff.

## Non-goals

- silently starting implementation from an unclarified comment;
- bypassing browser screen or microphone consent;
- shipping a mandatory transcription vendor;
- hiding an always-running poller or managed Minco control plane.

## Acceptance

- OpenAPI operations and Rust route descriptors are bijective.
- Browser tokens are tab-scoped by default and only hashes are persisted.
- Internal developer notes and object keys never enter client projections.
- PostgreSQL, SQLite, memory, and application-provided stores implement the same
  optimistic-concurrency contract.
- A developer can list, discuss, transition, export, and download feedback
  through `cargo minco feedback`.
- Compiler, Clippy, HTTP, database, and browser checks pass on the pinned
  toolchain before the task becomes complete.

## Current evidence

Prerequisite `M2-T01` is complete. The pinned Rust toolchain passes 37 Feedback
unit/HTTP/plugin tests, both SQLite and PostgreSQL persistence tests, strict
all-feature Clippy, and no-dependency documentation. `cargo-minco` passes its 12
tests; `cargo minco plugin validate` reports no findings; `minco explain
createFeedback --json` traces the contract, handler, use case, adapters, and
tests; and `cargo minco deploy plan` completes without diagnostics.

The contract check passes for all 13 operations under an isolated `uv` PyYAML
environment, static validation reports zero errors and warnings, Node syntax
passes, and `npm ci` reports zero vulnerabilities. The unchanged widget passes
Chromium and Firefox at `38/38`, including the `114/114` repeated stability run.
The package inventory includes both embedded migration directories.

A local PostgreSQL 18 container at `127.0.0.1:55432` exercised the real SQLx
adapter. Feedback now owns `_minco_feedback_migrations` rather than sharing the
application `_sqlx_migrations` ledger. The test removed all created feedback
records (`feedback_rows=0`) and retained one migration-history row as expected.
SQLite proves the same isolated ledger. This was local Docker infrastructure;
no AWS or other cloud service was contacted.

The focused task review also closed fail-open anonymous deserialization, public
provider-error disclosure, configuration/debug credential exposure, and the
stale compiler-verification audit statement. Anonymous submission now requires
explicit opt-in, and regression tests cover both struct defaults and
deserialized plugin configuration. The plugin remains beta until the separate
provider-adapter and bounded real-AWS gates pass.

The system `python3 scripts/test/feedback_contract.py` command initially exposed
an undeclared PyYAML prerequisite. This task now records the isolated passing
command. Repository-wide bootstrap and hosted-workflow dependency installation
remain a separate CI-closure task rather than relying on a global Mac package.
