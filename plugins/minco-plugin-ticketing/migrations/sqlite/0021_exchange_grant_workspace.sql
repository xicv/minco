-- Authoritative workspace binding for session exchange grants (round 1
-- finding 1): grants record the workspace identity they were issued
-- under, so rotation, revocation, and recovery validate persisted
-- ownership instead of trusting the receiving service's configuration.
-- NULL for legacy grants; the explicit upgrade inventory binds them.

ALTER TABLE ticketing_session_exchange_grants ADD COLUMN workspace_id TEXT;
