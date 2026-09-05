# ADR 0074: Desk security, accessibility and upgrade proofs

## Status

Accepted.

## Context

Stage G continues: the hardening of every public surface, requester
isolation on the real composition, and additive schema upgrade without
data loss must be proven before Minco Desk is standalone private beta.

## Decision

1. **Every public surface is hardened**: the agent console page serves
   a strict CSP (`default-src 'none'`, `frame-ancestors 'none'`),
   `nosniff` and `no-referrer`; every script and stylesheet carries the
   same nosniff discipline with exact content types. The public
   support-entry script previously lacked nosniff and no-referrer —
   the proof caught and fixed it by routing through the shared
   `hardened_asset` helper. The agent bootstrap never carries token,
   secret, password, credential or api-key material — only truthful
   permission-derived capabilities.
2. **Requesters are isolated and anonymous access refused**: an
   unauthenticated agent bootstrap gets 401; a requester reading
   another requester's ticket gets 404/403 — never the body.
3. **Upgrade from an earlier schema preserves every ticket**: the
   proof hand-applies migration 0001 (the first-generation schema),
   creates a real ticket in it, records the migration as applied in
   the sqlx bookkeeping table (with the correct SHA-384 checksum), then
   runs the current migrator — which advances through every later
   migration — and proves the pre-upgrade ticket survives with its
   data intact, the newer columns are readable, and the upgraded
   database serves through the full desk stack. This proof exposed and
   fixed **two real migration bugs**: the backfill UPDATEs in
   migrations 0002 and 0004 read `ticket_json` fields with
   `json_extract` without COALESCE, so any first-generation row whose
   JSON predated those fields crashed the NOT NULL constraint on
   upgrade. Both backfills now COALESCE to the column defaults.
4. All three proofs run as workspace-gated in-process tests.

## Consequences

- The migration fixes change checksums of already-committed
  migrations; this is safe because the plugin is unpublished (draft
  PR) and every existing database was created by the full migrator in
  one pass (never at the intermediate schema the fixes protect).
- Remaining Stage G evidence: load/performance, cost topology,
  PeoplePlanner BFF integration, separate database/release identity.
