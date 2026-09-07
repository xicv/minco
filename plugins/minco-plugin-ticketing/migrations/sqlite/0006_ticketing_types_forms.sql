PRAGMA foreign_keys = ON;

ALTER TABLE ticketing_tickets
    ADD COLUMN ticket_type TEXT NOT NULL DEFAULT 'question';
ALTER TABLE ticketing_tickets
    ADD COLUMN form_answers_json TEXT NOT NULL DEFAULT '[]';
