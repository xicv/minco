PRAGMA foreign_keys = ON;

-- Compact agent summary projection: the summary list must never read
-- ticket_json, so subject and priority become first-class projection columns.
ALTER TABLE ticketing_tickets ADD COLUMN subject TEXT NOT NULL DEFAULT '';
ALTER TABLE ticketing_tickets ADD COLUMN priority TEXT NOT NULL DEFAULT 'normal';
ALTER TABLE ticketing_tickets ADD COLUMN created_at TEXT NOT NULL DEFAULT '';

UPDATE ticketing_tickets
   SET subject = json_extract(ticket_json, '$.subject'),
       priority = json_extract(ticket_json, '$.priority'),
       created_at = json_extract(ticket_json, '$.created_at');

CREATE INDEX ticketing_tickets_summary_order_idx
    ON ticketing_tickets(project_id, updated_at DESC, id DESC);
