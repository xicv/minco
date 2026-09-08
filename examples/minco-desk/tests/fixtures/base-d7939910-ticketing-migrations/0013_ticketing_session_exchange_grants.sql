-- Exact-head review R3: session-exchange replay grants. One row per
-- handoff exchange, holding only non-secret rotation material — the
-- active session ID and the attributes needed to mint a replacement
-- session. Replays rotate: a new session is issued, the previous one is
-- revoked, and this row is updated. No bearer token is ever stored.
CREATE TABLE IF NOT EXISTS ticketing_session_exchange_grants (
    exchange_key TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    subject TEXT NOT NULL,
    project_id TEXT NOT NULL,
    permissions TEXT NOT NULL,
    portal_origin TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL
);
