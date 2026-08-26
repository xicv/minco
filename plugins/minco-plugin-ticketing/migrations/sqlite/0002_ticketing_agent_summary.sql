PRAGMA foreign_keys = ON;

-- Compact agent summary projection: the summary list must never read
-- ticket_json, so subject and priority become first-class projection columns.
ALTER TABLE ticketing_tickets ADD COLUMN subject TEXT NOT NULL DEFAULT '';
ALTER TABLE ticketing_tickets ADD COLUMN priority TEXT NOT NULL DEFAULT 'normal';
ALTER TABLE ticketing_tickets ADD COLUMN created_at TEXT NOT NULL DEFAULT '';

UPDATE ticketing_tickets
   SET subject = COALESCE(json_extract(ticket_json, '$.subject'), ''),
       priority = COALESCE(json_extract(ticket_json, '$.priority'), 'normal'),
       created_at = COALESCE(json_extract(ticket_json, '$.created_at'), updated_at);

CREATE INDEX ticketing_tickets_summary_order_idx
    ON ticketing_tickets(project_id, updated_at DESC, id DESC);
