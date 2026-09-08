PRAGMA foreign_keys = ON;

-- Requester portal-session handoff consumption: separate one-time claim
-- from ticket creation, so one handoff can establish a session identity
-- and, independently, create its ticket exactly once each.
ALTER TABLE ticketing_handoffs ADD COLUMN consumed_identity_json TEXT;
ALTER TABLE ticketing_handoffs ADD COLUMN completed_identity_fingerprint TEXT;
