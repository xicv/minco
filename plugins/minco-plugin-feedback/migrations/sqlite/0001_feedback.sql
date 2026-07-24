CREATE TABLE IF NOT EXISTS minco_feedback_threads (
    id TEXT PRIMARY KEY,
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
    document TEXT NOT NULL CHECK (json_valid(document)),
    revision INTEGER NOT NULL CHECK (revision >= 0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS minco_feedback_threads_inbox
    ON minco_feedback_threads (project_id, status, updated_at DESC);
