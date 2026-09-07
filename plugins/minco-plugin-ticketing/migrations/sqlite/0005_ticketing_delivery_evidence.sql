PRAGMA foreign_keys = ON;

CREATE TABLE ticketing_delivery_evidence (
    project_id TEXT NOT NULL,
    ticket_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('accepted', 'ambiguous', 'permanent_failure', 'feedback')),
    provider TEXT NOT NULL,
    provider_message_id TEXT NOT NULL,
    feedback TEXT CHECK (feedback IS NULL OR feedback IN ('bounce', 'complaint', 'delay')),
    failure_kind TEXT,
    recorded_at TEXT NOT NULL,
    evidence_json TEXT NOT NULL,
    PRIMARY KEY (project_id, ticket_id, message_id, recorded_at, kind, provider_message_id)
);

CREATE INDEX ticketing_delivery_evidence_message_idx
    ON ticketing_delivery_evidence(project_id, ticket_id, message_id, recorded_at);
