# ADR 0067: Curated views, revision-aware saved replies, collision indication

## Status

Accepted.

## Context

Stage E continues with agent productivity: a fixed set of curated views,
shared saved replies (macros), and an indication when another agent is
looking at the same ticket. The continuation prompt requires macros to
stay editable before submission and revision-aware, and forbids a
generic automation DSL.

## Decision

1. **Curated views are a closed server-defined set** (ADR-0067):
   `new-unassigned`, `pending-requester`, `pending-internal`, `mine`
   (the authenticated agent) and `recently-resolved`, exposed as
   `GET /agent/views/{viewId}` over the existing summary machinery and
   pagination. Unknown view ids are rejected; there is no ad-hoc query
   surface. The `new-unassigned` predicate extends the summary filter
   with an explicit `unassigned` flag honored in SQL and memory.
2. **Saved replies are a project-scoped shared library**: bounded title
   (≤300) and body (≤20000) plain text; listing is an agent-read,
   writing requires `ticketing.manage`. Applying a macro to a draft is a
   client-side text insertion — the server never submits on the agent's
   behalf, so "editable before submission" is inherent. The library is
   revision-aware: updates are full replacements gated by `If-Match`
   (`macro:{id}:{revision+1}` strong tags), and duplicate titles in a
   project are refused with a stable conflict code.
3. **Collision indication is advisory**: an agent ticket detail fetch
   records the viewer and returns `other_recent_viewers` (other agents
   who viewed within five minutes, newest first, at most eight, never
   the requesting agent). If-Match on mutations remains the actual
   collision authority; no realtime machinery exists.
4. SQLite migration 0007 stores the view heartbeats and the macro
   library with a unique `(project, title)`; the plugin descriptor and
   distribution record carry the four new operations.

## Consequences

- Agents get the five canonical working queues without learning a query
  language; adding a view is a server decision, not configuration.
- Concurrent macro editors fail fast (412) instead of overwriting; the
  agent console can merge.
- View records are ephemeral heartbeats (advisory only) — retention
  cleanup is unnecessary because stale rows simply stop matching the
  window; an operator may prune them with ordinary maintenance.
