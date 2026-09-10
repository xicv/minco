PRAGMA foreign_keys = ON;

CREATE TABLE ticketing_tickets (
    project_id TEXT NOT NULL,
    id TEXT NOT NULL,
    display_reference TEXT NOT NULL,
    status TEXT NOT NULL,
    queue_id TEXT,
    assignee_subject TEXT,
    requester_subject TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision >= 0),
    ticket_json TEXT NOT NULL,
    PRIMARY KEY (project_id, id),
    UNIQUE (project_id, display_reference)
);

CREATE TABLE ticketing_messages (
    project_id TEXT NOT NULL,
    ticket_id TEXT NOT NULL,
    id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    message_json TEXT NOT NULL,
    PRIMARY KEY (project_id, ticket_id, id),
    FOREIGN KEY (project_id, ticket_id) REFERENCES ticketing_tickets(project_id, id) ON DELETE CASCADE
);

CREATE TABLE ticketing_attachments (
    project_id TEXT NOT NULL,
    ticket_id TEXT NOT NULL,
    id TEXT NOT NULL,
    object_key TEXT NOT NULL,
    attachment_json TEXT NOT NULL,
    PRIMARY KEY (project_id, ticket_id, id),
    FOREIGN KEY (project_id, ticket_id) REFERENCES ticketing_tickets(project_id, id) ON DELETE CASCADE
);

CREATE TABLE ticketing_followers (
    project_id TEXT NOT NULL,
    ticket_id TEXT NOT NULL,
    subject TEXT NOT NULL,
    PRIMARY KEY (project_id, ticket_id, subject),
    FOREIGN KEY (project_id, ticket_id) REFERENCES ticketing_tickets(project_id, id) ON DELETE CASCADE
);

CREATE TABLE ticketing_tags (
    project_id TEXT NOT NULL,
    ticket_id TEXT NOT NULL,
    tag TEXT NOT NULL,
    PRIMARY KEY (project_id, ticket_id, tag),
    FOREIGN KEY (project_id, ticket_id) REFERENCES ticketing_tickets(project_id, id) ON DELETE CASCADE
);

CREATE TABLE ticketing_source_references (
    project_id TEXT NOT NULL,
    ticket_id TEXT NOT NULL,
    provider TEXT NOT NULL,
    scope TEXT NOT NULL,
    external_id TEXT NOT NULL,
    PRIMARY KEY (project_id, ticket_id, provider, scope, external_id),
    FOREIGN KEY (project_id, ticket_id) REFERENCES ticketing_tickets(project_id, id) ON DELETE CASCADE
);

CREATE TABLE ticketing_resource_references (
    project_id TEXT NOT NULL,
    ticket_id TEXT NOT NULL,
    system TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT NOT NULL,
    PRIMARY KEY (project_id, ticket_id, system, resource_type, resource_id),
    FOREIGN KEY (project_id, ticket_id) REFERENCES ticketing_tickets(project_id, id) ON DELETE CASCADE
);

CREATE TABLE ticketing_handoffs (
    digest TEXT PRIMARY KEY,
    handoff_id TEXT NOT NULL UNIQUE,
    project_id TEXT NOT NULL,
    portal_origin TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    handoff_json TEXT NOT NULL,
    consumed_result_json TEXT,
    completed_fingerprint TEXT
);

CREATE TABLE ticketing_external_messages (
    project_id TEXT NOT NULL,
    provider TEXT NOT NULL,
    mailbox_scope TEXT NOT NULL,
    external_id TEXT NOT NULL,
    content_sha256 TEXT NOT NULL,
    identity_json TEXT NOT NULL,
    ticket_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (project_id, provider, mailbox_scope, external_id),
    FOREIGN KEY (project_id, ticket_id) REFERENCES ticketing_tickets(project_id, id) ON DELETE CASCADE
);

CREATE TABLE ticketing_activity_intents (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    ticket_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    correlation_id TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    published_at TEXT,
    FOREIGN KEY (project_id, ticket_id) REFERENCES ticketing_tickets(project_id, id) ON DELETE CASCADE
);

CREATE INDEX ticketing_tickets_project_status_updated_idx
    ON ticketing_tickets(project_id, status, updated_at, id);
CREATE INDEX ticketing_tickets_queue_status_idx
    ON ticketing_tickets(project_id, queue_id, status, updated_at);
CREATE INDEX ticketing_tickets_assignee_status_idx
    ON ticketing_tickets(project_id, assignee_subject, status, updated_at);
CREATE INDEX ticketing_tickets_requester_idx
    ON ticketing_tickets(project_id, requester_subject, updated_at);
CREATE INDEX ticketing_messages_order_idx
    ON ticketing_messages(project_id, ticket_id, created_at, id);
CREATE INDEX ticketing_handoffs_expiry_idx
    ON ticketing_handoffs(project_id, expires_at);
CREATE INDEX ticketing_external_identity_idx
    ON ticketing_external_messages(project_id, provider, mailbox_scope, external_id);
CREATE INDEX ticketing_activity_pending_idx
    ON ticketing_activity_intents(project_id, published_at, created_at);
