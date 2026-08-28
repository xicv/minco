-- Exact-head review R20/P0-1: exchange generations fence the entire
-- session lifecycle. Every grant mutation carries a monotonically
-- increasing generation; a stale worker (one that lost its lease or
-- race) can never overwrite a newer generation's session. Revoking the
-- grant (logout) records revoked_at so replays die with the session.
ALTER TABLE ticketing_session_exchange_grants ADD COLUMN generation INTEGER NOT NULL DEFAULT 0;
ALTER TABLE ticketing_session_exchange_grants ADD COLUMN revoked_at TEXT;
