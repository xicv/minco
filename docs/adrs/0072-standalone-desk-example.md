# ADR 0072: The standalone Minco Desk example application

## Status

Accepted.

## Context

Stage G requires a private example application composing identity,
sessions/CSRF, idempotency, object storage, notifications/mail, audit,
events/outbox, jobs, health, observability, ticketing, `SQLite` or
PostgreSQL and a native runtime — the standalone private-beta proof.
Only after its evidence can Minco Desk be described as standalone.

## Decision

1. **The example lives at `examples/minco-desk`** as a workspace crate
   (`minco-desk-example`) with two binaries: `minco-desk-local` (one
   native Axum process serving the desk) and `minco-desk-migrate`
   (clean install and migration in one command: a fresh database file
   receives every table, an existing one advances). Every default is
   safe for a purely local, providerless run: `SQLite` file database,
   memory object storage, memory notifications, no provider contact.
2. **The composition root is explicit and singular** (ADR-0011): the
   ticketing service composes on one `SQLite` pool with the released
   jobs store (`FailClosedDispatcher`, `SystemJobClock`, the registered
   ticketing handlers under an ingest-only worker identity), memory
   object storage, and memory notifications. The plugin graph (health,
   observability, identity, sessions, idempotency, notifications,
   events, audit) is registered and selected explicitly — no runtime
   scanning, no hidden topology.
3. **Health proves the composition**: the health registry carries
   ticketing-store and jobs-store checks over the live pool; the
   composition graph is exposed on the built application for
   inspection.
4. **The proofs are in-process tests** (`standalone_proof.rs`): clean
   install creates every expected table and re-running migrations is
   idempotent; the composed desk serves the public support entry and
   the authenticated agent bootstrap through the full middleware stack;
   an end-to-end ticket lifecycle (create through HTTP, bounded search
   collision-aware agent detail) runs on the one database.
5. The first proof exposed and fixed a real defect: the agent search
   endpoint fed its own `q` parameter into the pagination parser,
   which rejects unknown parameters — every real search request
   returned 422. The handler now consumes `q` before delegating.

## Consequences

- The remaining Stage G evidence (upgrade, backup/restore, retention,
  email replay/DLQ, job recovery, load, accessibility, security, cost
  topology, PeoplePlanner BFF integration, separate database/release
  identity) builds on this composition in later slices.
- The example is a workspace member and ships in the workspace gate;
  it cannot rot silently.
- A recipe entry (`standalone-desk`) documents how to run it.

## Features

Composed on one SQLite database behind one native Axum process:
ticketing (45 operations), jobs with same-transaction enqueue and an
operated in-process worker, durable requester sessions with CSRF,
durable idempotency, audit, the in-process event bus, memory object
storage (no provider contact), in-app notifications, health and
observability. The trust boundary is explicit: requester routes
authenticate with the durable session cookie; every other route
requires the loopback service bearer token (`DESK_AGENT_TOKEN`), and
development identity headers are never trusted. Verified first-contact
email intake is off by default; mail delivery is deliberately absent.

## Provider assumptions

No provider access: memory objects and notifications, one SQLite file
database, one native process bound to loopback by default. No SES, no
SQS, no mail transport is contacted by the example.

## Cost and wake behavior

Zero compute beyond the local process; wake source is the HTTP request
itself plus the explicit jobs worker (`DeskWorker::run_once`, driven on
an interval by the local binary). No schedules, no provisioned
concurrency, no queues.

## Verification

The `minco-desk-example` check (`cargo test -p minco-desk-example`) runs
the in-process proofs plus the real-TCP durability proofs
(`tests/desk_durability_proofs.rs`): the bearer trust boundary refuses
anonymous, forged-header and wrong-token calls; sessions, the idempotent
exchange and notification jobs survive a full process rebuild on the
same database; the restarted worker completes the pending notification
job; logout revokes the session and expires the cookie.

## Unsupported gates

Hosted Linux qualification, provider contact, email delivery, the
PostgreSQL profile, browser-driven console verification (covered by the
agent-console Playwright suite) and mobile clients are out of scope for
this example.

## Amendment (2026-09-01, M14-T74 stabilization reviews 5060065907 and
5072859042)

Operational hardening landed during stabilization; the original
decision stands, with these refinements authoritative:

- Non-local credentials accept file-sourced secrets
  (`*_FILE` rotation by update-and-restart) and hex/base64 key material
  judged on decoded bytes (at least 32); readiness covers the
  subsystems a request traverses (sessions, idempotency receipts, audit
  backlog, object storage, ticketing and jobs stores) with liveness
  kept to the critical subset.
- The example's positioning is unchanged and explicit: a
  local/providerless standalone reference and a private BFF integration
  foundation — not a production-ready standalone service.
