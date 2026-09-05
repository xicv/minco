-- Exact-head review R4: durable outbound send intents. One row per
-- logical outbound public reply, committed BEFORE provider contact, so
-- ambiguous outcomes resolve by stable identity instead of resending.
-- States: sending -> sent (provider id recorded) | recovery_required.
-- A reconciled authoritative no-send returns the intent to pending_send
-- so exactly one identity-stable resend may occur.
CREATE TABLE IF NOT EXISTS ticketing_send_intents (
    logical_send_id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    ticket_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('pending_send', 'sending', 'sent', 'recovery_required', 'failed_no_send')),
    provider_message_id TEXT,
    updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL
);
