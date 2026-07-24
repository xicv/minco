CREATE TABLE IF NOT EXISTS minco_feedback_threads (
    id UUID PRIMARY KEY,
    client_token_hash TEXT NOT NULL,
    project_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN (
        'new',
        'acknowledged',
        'needs_clarification',
        'ready_for_development',
        'in_progress',
        'resolved',
        'closed'
    )),
    document JSONB NOT NULL,
    revision BIGINT NOT NULL CHECK (revision >= 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS minco_feedback_threads_inbox
    ON minco_feedback_threads (project_id, status, updated_at DESC);
