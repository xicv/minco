---
id: M6-T02
title: Stabilize provider-neutral essential plugin contracts
milestone: M6
status: complete
priority: medium
area: plugins
depends_on: [M5-T01]
operations: []
owned_paths:
  - plugins/**
  - extensions/**
  - infra/aws/generated/plan.json
  - tasks/M6/M6-T02-essential-plugins.md
checks:
  - cargo minco plugin validate
  - cargo test --workspace --all-targets --all-features --locked
---

## Goal

Stabilize independently selectable sessions, identity, object-storage, events/outbox, notifications, audit, and static-site contracts without adding provider dependencies, fixed capacity, or hidden schedules to the core. Concrete AWS implementations are tracked separately in M6-T04.

## Completion evidence

Completed on 2026-07-25 after prerequisite `M5-T01` completed.

- `cargo minco plugin validate` returned `[]`.
- `cargo test --workspace --all-targets --all-features --locked` passed on
  `rustc 1.97.1`; only the explicitly environment-gated PostgreSQL and Rustack
  tests were ignored.
- `cargo test -p minco-plugin-idempotency --locked` passed all 4 tests.
- `cargo clippy -p minco-plugin-idempotency --all-targets --all-features
  --locked -- -D warnings` passed.
- `rustfmt --edition 2024 --check
  plugins/minco-plugin-idempotency/src/lib.rs` passed without modifying files.
- `cargo minco inspect --json` and the committed deployment plan now report the
  default idempotency plugin as `stable`, matching `plugins/catalog.toml` and
  the documented stable-default contract.
- Direct manifest metadata for sessions, identity, object storage, events,
  notifications, audit, and static site contains no Axum, SQLx, Lambda, or AWS
  SDK dependency. Source review found no hidden spawned worker, timer, cron, or
  schedule; event recovery remains explicitly application/operator selected.
- The focused single-task review found no remaining correctness, architecture,
  security, cost, or ownership finding. Concrete AWS adapters and their bounded
  cloud conformance remain separately owned by `M6-T04`.
- This task made no real-cloud calls or mutations.

## Issues caught and permanent corrections

- `task-start` initially created the empty working commit beside rather than
  after `M5-T01`; it was rebased onto `task/m5-t01` before any source edit.
- The first inspection redirect failed because the ignored
  `target/minco/` directory did not exist, and the first query assumed the wrong
  JSON field. The directory is now created before evidence capture and the
  checked path is `deployment.application_graph`.
- The catalog classified idempotency as stable while its runtime descriptor and
  generated plan classified it as beta. The descriptor is now stable, the
  generated plan is synchronized, and a regression test locks the runtime
  contract.
- An initial dependency review searched the full transitive tree and therefore
  produced an irrelevant Axum match. The durable review uses Cargo's direct
  package metadata, which tests the provider-neutral boundary precisely.
- `git diff --check` was inapplicable in this JJ-only workspace. Review and
  conflict checks use JJ, while Rust formatting is checked only for the modified
  Rust file.
- The task-finish wrapper unconditionally runs repository-wide formatting and
  Clippy checks, which conflicts with this closure's modified-file-only lint
  boundary. Its final JJ describe/bookmark operations are performed directly
  after the scoped lint and full required test gate, with no remote push.
- The task originally omitted its own evidence file and the deterministic plan
  snapshot from `owned_paths`; both exact paths are now owned so completion
  evidence and generated output cannot sit outside the task boundary.
