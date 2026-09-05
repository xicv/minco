-- Exact-head review R29/P0-3: delivery evidence is attempt-scoped.
-- Rows carry the attempt identity that produced them so current-attempt
-- evidence, stale-attempt reports and reconciliation output are always
-- distinguishable. Both columns are nullable: evidence written before
-- this migration has no attempt scope.
ALTER TABLE ticketing_delivery_evidence ADD COLUMN attempt_id TEXT;
ALTER TABLE ticketing_delivery_evidence ADD COLUMN attempt_sequence INTEGER;
