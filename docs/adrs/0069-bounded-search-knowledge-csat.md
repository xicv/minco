# ADR 0069: Bounded search, knowledge links, one-shot CSAT

## Status

Accepted.

## Context

Stage E closes with the last three productivity items from the
continuation prompt: bounded search, knowledge links and optional CSAT.
The same constraint governs all three: small, explicit, reviewable
surfaces — no search engine, no content system, no survey machinery.

## Decision

1. **Search is a bounded substring match** over subject, display
   reference and description — never message bodies. The query is
   2..=200 trimmed characters without control characters; LIKE
   wildcards in the query are escaped (`ESCAPE '\'`) so user input can
   never widen the match; results ride the ordinary summary pagination
   newest-first with no ranking engine.
2. **Knowledge links are a bounded replacement decision**: up to 16
   links per ticket, each a bounded `article_id` (unique per ticket),
   `title` and https `url`; one `PUT` replaces the whole list gated by
   If-Match, mirroring the management operation's atomicity. Links live
   in a columnar `knowledge_links_json` with full round-trip.
3. **CSAT is one-shot and requester-owned**: only the ticket's requester
   may rate, only a resolved or closed ticket accepts it, and exactly
   once (score 1..=5 plus an optional bounded comment). The record is
   immutable once written and surfaces on the agent ticket and the
   requester projection; no aggregate dashboards exist in this slice.
4. SQLite migration 0009 adds `knowledge_links_json` and `csat_json`;
   the descriptor, distribution record and contract carry the three new
   operations.

## Consequences

- Agents find tickets by what requesters wrote without a search
  subsystem to operate; wildcards are literals, so results are
  predictable.
- Knowledge curation is auditable ticket state, revision-gated like
  every other mutation; a future knowledge plugin must integrate
  through these links, not a parallel store.
- CSAT gives small helpdesks a real satisfaction signal with zero
  infrastructure; resubmission and cross-requester rating fail closed.
