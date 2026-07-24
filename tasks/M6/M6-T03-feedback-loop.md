---
id: M6-T03
title: Ship the official Feedback review-loop plugin
milestone: M6
status: active
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
checks:
  - python3 scripts/test/feedback_contract.py
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

The complete compiler/Clippy/test/doc matrix, all Feedback feature shapes,
SQLite and real PostgreSQL conformance, and the Chromium/Firefox browser suite
pass. The browser results are `38/38`, with a repeated stability run of
`114/114`. Server-side tests prove developer authorization, private-note
isolation and attachment response headers. This task remains active because
prerequisite `M2-T01` is active.
