-- Exact-head review R28/P0-2: rotation staging closes the last
-- double-bearer window. A rotation now revokes the previous bearer
-- BEFORE minting a replacement and durably stages the freshly minted
-- session id here first; a rotation interrupted between mint and
-- compare-and-swap is recovered (staged bearer retired, marker cleared)
-- instead of leaking a second live bearer. The marker is only ever
-- written while the grant still records the expected previous session,
-- so concurrent rotations cannot stage over each other.
ALTER TABLE ticketing_session_exchange_grants ADD COLUMN rotation_staged_session_id TEXT;
