---
id: M12-T05
title: Pass security recovery load and documentation release gates
milestone: M12
status: complete
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
  - crates/minco-mcp/tests/**
  - crates/minco-workbench/tests/**
  - tasks/M12/M12-T05-release-gates.md
checks:
  - ./scripts/quality.sh
  - npm run --prefix plugins/minco-plugin-feedback test:browser
  - scripts/test/e2e.sh
  - scripts/dev/rustack-smoke.sh
  - scripts/release/publish.sh --skip-quality
  - scripts/release/candidate-recovery.sh --output verification/1.0-candidate-recovery.json
  - scripts/release/candidate-load.sh --output verification/1.0-candidate-load.json
  - scripts/release/package-list.sh
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

## Evidence

The schema-closed candidate runner records each mandatory command as `PASS`,
`FAIL`, `BLOCKED` or `NOT RUN`, plus exit status, duration and the size and
SHA-256 digest of its ignored local log. A release record cannot be `PASS`
unless every mandatory local command is `PASS`. The final source identity and
per-command evidence are in
`verification/1.0-candidate-release-gates.json`; raw logs remain beneath
ignored `target/minco/candidate-release-gates/`.

The complete local gate covers full compiler, Clippy, test, dependency,
secret, browser, documentation, generated-consumer and package dry-run checks.
The manual hosted release profile now also runs the bounded recovery and load
gates and uploads only their aggregate JSON, without provider identifiers,
credentials, response bodies or synthetic database files.

### Recovery

`scripts/release/candidate-recovery.sh` uses the public, digest-approved
`cargo minco db` lifecycle and its catalog-owned `_minco_orders_migrations`
history, not the lower-level example migrator. It applies both SQLite
migrations, proves a repeat application is idempotent, creates one synthetic
order through HTTP, performs an online backup, deletes the temporary source
database, restores to a separate file, verifies integrity and both migrations,
and reads the order through a fresh application process. The complete data
boundary is a system temporary directory removed by the runner; receipts stay
inside ignored project `target/` as required by CLI containment.

The focused deployment rollback tests and complete provider-free multi-release
rehearsal plan suite pass in the same gate. Reverse SQL and automatic data
repair remain explicitly false. The completed M10-T08 prior/current/prior live
rehearsal remains historical provider evidence bound to public revisions
`9cbe8fdb64a6f68363fd1cac949ddfa554106667` and
`4573239d83fff91fffd79ea9bda58afbe217ffe9`, with all fourteen cleanup
boundaries proved absent. M12-T05 did not obtain fresh account/provider
authority and records exact-current AWS redeployment as `NOT RUN`; it made no
AWS call.

### Load and cost dimensions

`scripts/release/candidate-load.sh` drives the real loopback Axum application
with file-backed SQLite, four database connections, eight concurrent clients
and 80 fresh synthetic write connections. It separately builds a disposable
external crate on the manifest-pinned Rust toolchain and sends 1,000 messages
through the public SQS worker batch API in 100 batches. A passing record
requires zero API/worker failures and observed worker concurrency at or below
the configured bound.

The reviewed schema-2 Plan fixture contributes batch size, visibility,
retention, mapping concurrency, reserved concurrency and per-instance database
connections. The report retains request/message/invocation and aggregate
connection dimensions and actual local artifact byte sizes. It intentionally
makes no moving provider-price or production-SLO claim; loopback latency is
machine-specific and the worker run does not emulate Lambda poller scaling,
SQS retries, throttling, quotas or network latency.

### Qualification defects caught

Red-green qualification caught and fixed six evidence-boundary defects without
changing application/runtime source: an external Cargo project falling back to
Rust 1.91 instead of the pinned 1.97.1, use of a lower-level migration history
table that conflicts with the catalog-owned lifecycle, an absolute receipt path
rejected by the CLI's project-containment guard, MCP and Workbench archive tests
that incorrectly reached outside their packaged crates for a repository root,
and interactive cleanup that could prompt on read-only disposable Git objects.
The final runner uses the pinned toolchain, public migration command and
relative ignored receipt paths; both packaged crates use synthetic ProjectView
fixtures contained in `tests/**`, and the exact `mktemp`-owned rehearsal root is
removed non-interactively. No failed attempt produced an accepted qualification
record.

The [candidate qualification guide](../../docs/development/release-qualification.md)
documents official AWS, Cargo and Python sources, evidence states, commands,
limitations and the no-public-data boundary. Source, hosted CI, historical
provider proof, exact-current provider execution, tag, registry upload, docs
publication and production adoption remain separate states.
