# ADR 0073: Desk data-lifecycle proofs — backup, restore, retention, job recovery

## Status

Accepted.

## Context

Stage G's private-beta evidence continues: backup, restore,
retention/erasure and job recovery must be proven on the standalone
composition (ADR-0072) before Minco Desk can be described as
standalone.

## Decision

1. **Backup is SQLite's online `VACUUM INTO`** — a consistent snapshot
   without stopping the process. Because `VACUUM INTO` cannot be
   parameterized, the statement is wrapped in sqlx's
   `AssertSqlSafe` after construction from a controlled path (never
   request input). **Restore** is simply composing a fresh desk on the
   backup file: the proof serves every ticket from before the backup
   through the full HTTP stack.
2. **Retention erasure is an explicit, bounded operator operation**:
   the ticketing store gains `erase_tickets_resolved_before(cutoff,
   limit)` — resolved-or-closed tickets last updated before the cutoff
   are deleted oldest-first, bounded by `limit`; every child row
   (messages, views, clarifications, proposals, evidence) cascades via
   foreign keys. Nothing schedules it: an operator calls it (or wires
   it to an explicit job later). The desk example exposes it as
   `erase_resolved_before`; the proof verifies exactly the resolved
   ticket disappears, the open ticket survives, and children cascaded.
3. **Job recovery is proven across a simulated process death**: the
   first process composes, submits three durable jobs and is dropped
   entirely (pool included); the second process opens the same file,
   finds all three still `pending`, and a single `dispatch_due_once`
   claim pass recovers every one.
4. All three proofs run as workspace-gated in-process tests against
   the real composition — no scripts to discover rotting, no fakes.

## Consequences

- Backup/restore covers the SQLite profile today; PostgreSQL restores
  through its own tooling (`pg_dump`) and needs its own proof when
  that profile ships.
- Erasure is ticket-scoped retention, not per-subject GDPR erasure;
  requester-data erasure across tickets is a separate future decision.
- Remaining Stage G evidence (upgrade across release versions, load,
  accessibility, security review, cost topology, PeoplePlanner BFF,
  separate database/release identity) builds on these proofs.
