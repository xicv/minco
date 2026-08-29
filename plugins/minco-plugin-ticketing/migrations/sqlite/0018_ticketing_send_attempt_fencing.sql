-- Exact-head review R22/P0-3: attempt-fenced send state machine. Each
-- sending claim carries a unique attempt identity; every subsequent
-- state transition validates the same attempt so a stale worker can
-- never write results over a newer attempt's state.
ALTER TABLE ticketing_send_intents ADD COLUMN attempt_id TEXT;
ALTER TABLE ticketing_send_intents ADD COLUMN attempt_sequence INTEGER NOT NULL DEFAULT 0;
ALTER TABLE ticketing_send_intents ADD COLUMN lease_expires_at TEXT;
