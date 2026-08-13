CREATE TABLE IF NOT EXISTS minco_audit_journal (
    event_id TEXT PRIMARY KEY NOT NULL,
    occurred_at TEXT NOT NULL,
    record TEXT NOT NULL,
    encoded_bytes INTEGER NOT NULL CHECK (encoded_bytes > 0),
    status TEXT NOT NULL CHECK (status IN ('pending', 'claimed', 'failed', 'quarantined')),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    available_at TEXT NOT NULL,
    claimed_by TEXT,
    claim_expires_at TEXT,
    failure_code TEXT
);

CREATE INDEX IF NOT EXISTS minco_audit_journal_dispatch
    ON minco_audit_journal (status, available_at, occurred_at, event_id);
