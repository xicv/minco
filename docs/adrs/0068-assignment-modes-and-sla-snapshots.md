# ADR 0068: Assignment modes and SLA deadline snapshots

## Status

Accepted.

## Context

Stage E continues: helpdesks need more than manual assignment — a
rotation, a least-loaded pick, and response-time targets that stay
stable instead of drifting with every edit. The continuation prompt
lists manual/round-robin/least-workload assignment and
first-response/resolution deadline snapshots.

## Decision

1. **Assignment decisions are explicit** on the existing
   `changeTicketAssignment` operation: a required `mode` — `manual`
   (carries or clears the subject, exactly as before), `round_robin`
   and `least_workload` (server-picked from a configured pool). The
   pool is a bounded, unique, explicit `assignment_pool` in the ticket
   configuration — never discovered from live traffic, never hidden
   automation.
2. **Round-robin uses a durable per-project cursor** (atomic
   read-advance in one transaction, SQLite table
   `ticketing_assignment_cursor`); **least-workload** counts each
   member's open (unresolved, unclosed) tickets in one grouped query
   and breaks ties lexicographically so the pick is deterministic.
   Pool modes without a configured pool fail closed with a
   configuration error.
3. **SLA deadlines are snapshots, not live clocks**: an optional
   `TicketSlaConfig { first_response_hours, resolution_hours }` (0
   disables that one deadline) stamps `first_response_deadline` and
   `resolution_deadline` at creation — on the API create path and the
   widget handoff path — and they are never recomputed. They surface on
   the agent ticket and agent summaries only; requesters never see a
   promised time (that would be a commitment the system cannot keep).
4. SQLite migration 0008 adds the two nullable deadline columns and the
   cursor table; summaries read the deadlines from the projection
   query.

## Consequences

- Curated views and future overdue indicators can filter on the
  snapshots without re-deriving SLAs.
- Assignments stay auditable agent decisions; pool modes are
  deterministic and reviewable, and the cursor is the only mutable
  assignment state.
- Existing clients sending the old body shape (no `mode`) fail
  validation — the field is required; this is a draft-stage contract
  change, recorded here deliberately.
