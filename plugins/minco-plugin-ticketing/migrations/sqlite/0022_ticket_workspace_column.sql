-- Workspace ownership column (round 1 finding 5): tickets carry the
-- workspace identity they belong to. '' marks not-yet-bound rows; the
-- explicit provisioning phase (which registers every existing project
-- under the provisioned workspace) performs the authoritative bind and
-- the composite-foreign-key rebuild — a migration alone cannot, because
-- the registry rows it references are created by provisioning.

ALTER TABLE ticketing_tickets ADD COLUMN workspace_id TEXT NOT NULL DEFAULT '';
