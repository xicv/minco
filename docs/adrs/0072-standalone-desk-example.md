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
