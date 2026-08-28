-- Exact-head review R5: audit dispatch delivery marks. Intents carry a
-- separate audit delivery timestamp so the explicit audit dispatcher has
-- its own claim/delivered semantics; the audit event id equals the
-- intent id, making at-least-once appends dedupeable downstream.
ALTER TABLE ticketing_activity_intents ADD COLUMN audit_published_at TEXT;
