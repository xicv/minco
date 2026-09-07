-- Forward-only: the semantic-fingerprint column for audit conflict
-- detection (exact-head reviews R24 and R27/R31). Additive only; the
-- published 0001 migration is never edited, so databases migrated by
-- earlier releases keep matching their recorded checksums. Rows written
-- before this migration keep NULL and are content-verified and adopted
-- by the adapter on their first redelivery (see PostgresAuditSink::append).
ALTER TABLE minco_audit ADD COLUMN fingerprint TEXT;
