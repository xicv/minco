PRAGMA foreign_keys = ON;

CREATE TABLE ticketing_automation_proposals (
    project_id TEXT NOT NULL,
    id TEXT NOT NULL,
    ticket_id TEXT NOT NULL,
    summary TEXT NOT NULL,
    requested_actions_json TEXT NOT NULL,
    created_by TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('awaiting_review', 'accepted', 'rejected')),
    created_at TEXT NOT NULL,
    decided_at TEXT,
    PRIMARY KEY (project_id, id),
    FOREIGN KEY (project_id, ticket_id) REFERENCES ticketing_tickets(project_id, id) ON DELETE CASCADE
);

CREATE INDEX ticketing_automation_proposals_ticket_idx
    ON ticketing_automation_proposals(project_id, ticket_id, created_at);
