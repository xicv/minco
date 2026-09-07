-- Exact-head review R21/P0-2: operation receipts carry their full
-- scope and an expiry so stale-lease recovery can verify operation,
-- project, subject digest, fingerprint AND freshness before replaying
-- the stored response — and so receipt retention is explicit rather
-- than implicitly unbounded.
ALTER TABLE ticketing_operation_receipts ADD COLUMN operation TEXT NOT NULL DEFAULT '';
ALTER TABLE ticketing_operation_receipts ADD COLUMN project_id TEXT NOT NULL DEFAULT '';
ALTER TABLE ticketing_operation_receipts ADD COLUMN subject_digest TEXT NOT NULL DEFAULT '';
ALTER TABLE ticketing_operation_receipts ADD COLUMN expires_at TEXT;
