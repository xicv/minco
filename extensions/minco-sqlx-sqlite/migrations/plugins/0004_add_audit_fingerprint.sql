-- Forward-only: the semantic-fingerprint column for audit conflict
-- detection (exact-head reviews R24 and R27/R31). Migration 0001 is
-- byte-identical to the Minco 1.12 release; schema changes only ever
-- land as new forward migrations so recorded checksums keep matching.
-- Pre-existing rows keep NULL and are content-verified and adopted by
-- the adapter on their first redelivery (see SqliteAuditSink::append).
ALTER TABLE minco_audit ADD COLUMN fingerprint TEXT;
