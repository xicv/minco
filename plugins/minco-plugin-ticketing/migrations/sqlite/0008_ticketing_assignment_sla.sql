PRAGMA foreign_keys = ON;

ALTER TABLE ticketing_tickets
    ADD COLUMN first_response_deadline TEXT;
ALTER TABLE ticketing_tickets
    ADD COLUMN resolution_deadline TEXT;

CREATE TABLE ticketing_assignment_cursor (
    project_id TEXT PRIMARY KEY,
    next_index INTEGER NOT NULL CHECK (next_index >= 0)
);
