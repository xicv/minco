CREATE TABLE IF NOT EXISTS minco_outbox (
    event_id UUID PRIMARY KEY,
    event_type TEXT NOT NULL,
    aggregate_type TEXT NOT NULL,
    aggregate_id TEXT NOT NULL,
    correlation_id UUID NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    payload JSONB NOT NULL,
    metadata JSONB NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'claimed', 'published', 'failed')),
    attempt_count INTEGER NOT NULL CHECK (attempt_count >= 0),
    available_at TIMESTAMPTZ NOT NULL,
    claimed_by TEXT,
    claim_expires_at TIMESTAMPTZ,
    last_error TEXT
);

CREATE INDEX IF NOT EXISTS minco_outbox_dispatch
    ON minco_outbox (status, available_at, occurred_at);

CREATE TABLE IF NOT EXISTS minco_sessions (
    id UUID PRIMARY KEY,
    token_hash BYTEA NOT NULL UNIQUE CHECK (octet_length(token_hash) = 32),
    subject TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    attributes JSONB NOT NULL
);

CREATE INDEX IF NOT EXISTS minco_sessions_subject_active
    ON minco_sessions (subject, revoked_at);

CREATE TABLE IF NOT EXISTS minco_idempotency (
    key TEXT PRIMARY KEY,
    fingerprint TEXT NOT NULL CHECK (length(fingerprint) = 64),
    state TEXT NOT NULL CHECK (state IN ('in_progress', 'completed')),
    lease_id UUID,
    started_at TIMESTAMPTZ NOT NULL,
    response JSONB,
    completed_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS minco_audit (
    sequence BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID NOT NULL UNIQUE,
    action TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT NOT NULL,
    actor_subject TEXT,
    correlation_id UUID NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    metadata JSONB NOT NULL
);

CREATE INDEX IF NOT EXISTS minco_audit_resource
    ON minco_audit (resource_type, resource_id, sequence);
