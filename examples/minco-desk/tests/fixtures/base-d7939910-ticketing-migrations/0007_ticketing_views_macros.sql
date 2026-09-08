PRAGMA foreign_keys = ON;

CREATE TABLE ticketing_ticket_views (
    project_id TEXT NOT NULL,
    ticket_id TEXT NOT NULL,
    subject TEXT NOT NULL,
    viewed_at TEXT NOT NULL,
    PRIMARY KEY (project_id, ticket_id, subject),
    FOREIGN KEY (project_id, ticket_id) REFERENCES ticketing_tickets(project_id, id) ON DELETE CASCADE
);

CREATE INDEX ticketing_ticket_views_recent_idx
    ON ticketing_ticket_views(project_id, ticket_id, viewed_at);

CREATE TABLE ticketing_macros (
    project_id TEXT NOT NULL,
    id TEXT NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision >= 0),
    PRIMARY KEY (project_id, id),
    UNIQUE (project_id, title)
);
