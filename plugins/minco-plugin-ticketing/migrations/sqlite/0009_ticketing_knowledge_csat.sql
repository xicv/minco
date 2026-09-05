PRAGMA foreign_keys = ON;

ALTER TABLE ticketing_tickets
    ADD COLUMN knowledge_links_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE ticketing_tickets
    ADD COLUMN csat_json TEXT;
