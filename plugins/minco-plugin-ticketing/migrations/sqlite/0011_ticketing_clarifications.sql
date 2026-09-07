PRAGMA foreign_keys = ON;

CREATE TABLE ticketing_clarifications (
    project_id TEXT NOT NULL,
    id TEXT NOT NULL,
    ticket_id TEXT NOT NULL,
    reason TEXT NOT NULL CHECK (reason IN ('missing_requirement', 'contradictory_requirement')),
    questions_json TEXT NOT NULL,
    checkpoint TEXT NOT NULL,
    created_by TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('draft', 'sent', 'answered', 'withdrawn')),
    created_at TEXT NOT NULL,
    sent_at TEXT,
    answered_at TEXT,
    answers_json TEXT,
    PRIMARY KEY (project_id, id),
    FOREIGN KEY (project_id, ticket_id) REFERENCES ticketing_tickets(project_id, id) ON DELETE CASCADE
);

CREATE INDEX ticketing_clarifications_ticket_idx
    ON ticketing_clarifications(project_id, ticket_id, created_at);
