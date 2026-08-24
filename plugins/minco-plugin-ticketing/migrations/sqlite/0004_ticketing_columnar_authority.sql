PRAGMA foreign_keys = ON;

-- Columnar ticket authority (ADR-0052): reads reconstruct the aggregate
-- from these columns and child tables; ticket_json stops being
-- authoritative and becomes a create/full-save diagnostic snapshot.
ALTER TABLE ticketing_tickets ADD COLUMN description TEXT NOT NULL DEFAULT '';
ALTER TABLE ticketing_tickets ADD COLUMN channel TEXT NOT NULL DEFAULT 'api';
ALTER TABLE ticketing_tickets ADD COLUMN requester_display_name TEXT;
ALTER TABLE ticketing_tickets ADD COLUMN requester_email TEXT;
ALTER TABLE ticketing_tickets ADD COLUMN first_public_response_at TEXT;
ALTER TABLE ticketing_tickets ADD COLUMN waiting_since TEXT;
ALTER TABLE ticketing_tickets ADD COLUMN resolved_at TEXT;
ALTER TABLE ticketing_tickets ADD COLUMN closed_at TEXT;
ALTER TABLE ticketing_tickets ADD COLUMN resolution TEXT;
ALTER TABLE ticketing_tickets ADD COLUMN close_reason TEXT;

UPDATE ticketing_tickets
   SET description = json_extract(ticket_json, '$.description'),
       channel = json_extract(ticket_json, '$.channel'),
       requester_display_name = json_extract(ticket_json, '$.requester.display_name'),
       requester_email = json_extract(ticket_json, '$.requester.email'),
       first_public_response_at = json_extract(ticket_json, '$.first_public_response_at'),
       waiting_since = json_extract(ticket_json, '$.waiting_since'),
       resolved_at = json_extract(ticket_json, '$.resolved_at'),
       closed_at = json_extract(ticket_json, '$.closed_at'),
       resolution = json_extract(ticket_json, '$.resolution'),
       close_reason = json_extract(ticket_json, '$.close_reason');
