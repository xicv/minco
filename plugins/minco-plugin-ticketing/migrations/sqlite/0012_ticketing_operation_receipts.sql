-- Exact-head review R2: recoverable idempotency receipts. One row per
-- idempotency-guarded ticketing mutation, committed in the same
-- transaction as the mutation so a lost HTTP response can always be
-- replayed from the authoritative receipt instead of re-executing the
-- business mutation.
CREATE TABLE IF NOT EXISTS ticketing_operation_receipts (
    idempotency_key TEXT PRIMARY KEY,
    fingerprint TEXT NOT NULL,
    response_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);
