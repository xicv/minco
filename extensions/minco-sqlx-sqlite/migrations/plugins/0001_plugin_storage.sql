CREATE TABLE IF NOT EXISTS minco_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    token_hash BLOB NOT NULL UNIQUE CHECK (length(token_hash) = 32),
    subject TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    revoked_at TEXT,
    attributes TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS minco_sessions_subject_active
    ON minco_sessions (subject, revoked_at);

CREATE TABLE IF NOT EXISTS minco_idempotency (
    key TEXT PRIMARY KEY NOT NULL,
    fingerprint TEXT NOT NULL CHECK (length(fingerprint) = 64),
    state TEXT NOT NULL CHECK (state IN ('in_progress', 'completed')),
    lease_id TEXT,
    started_at TEXT NOT NULL,
    response TEXT,
    completed_at TEXT
);

CREATE TABLE IF NOT EXISTS minco_audit (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT NOT NULL UNIQUE,
    action TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT NOT NULL,
    actor_subject TEXT,
    correlation_id TEXT NOT NULL,
    occurred_at TEXT NOT NULL,
    metadata TEXT NOT NULL,
    fingerprint TEXT
);

CREATE INDEX IF NOT EXISTS minco_audit_resource
    ON minco_audit (resource_type, resource_id, sequence);
