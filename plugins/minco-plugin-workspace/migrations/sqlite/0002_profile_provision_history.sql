-- Provision history (round 1 finding 2): every profile id ever seeded
-- into this database is retained permanently. A configured profile that
-- is missing from the live registry but present here was removed after
-- provisioning and demands explicit reconciliation instead of silent
-- recreation.

CREATE TABLE IF NOT EXISTS workspace_profile_history (
    profile_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    provisioned_at TEXT NOT NULL,
    PRIMARY KEY (profile_id, workspace_id),
    FOREIGN KEY (workspace_id) REFERENCES workspace_deployment_binding (workspace_id)
);
