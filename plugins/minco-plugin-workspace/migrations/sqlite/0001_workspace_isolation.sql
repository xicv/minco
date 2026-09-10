-- Workspace isolation registry (ADR-0076).
--
-- This migration stream owns exactly one ledger table name and never touches
-- the sqlx default ledger (`_sqlx_migrations`) or any other plugin ledger.
-- `workspace_deployment_binding` binds THIS database to exactly one
-- provisioned workspace identity: it is the deployment binding for the
-- private-beta silo adapter, not a universal single-workspace domain
-- invariant (a pooled-storage adapter would carry its own registry).
--
-- Foreign keys are composite: profiles and grants must reference a project
-- that is registered under the bound workspace, so ownership is proven by the
-- relationship rather than by two independent columns.

CREATE TABLE IF NOT EXISTS workspace_deployment_binding (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    workspace_id TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    provisioned_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS workspace_projects (
    workspace_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    registered_at TEXT NOT NULL,
    PRIMARY KEY (workspace_id, project_id),
    FOREIGN KEY (workspace_id) REFERENCES workspace_deployment_binding (workspace_id)
);

CREATE TABLE IF NOT EXISTS workspace_profiles (
    profile_id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    auth_mode TEXT NOT NULL,
    status TEXT NOT NULL,
    bound_project_id TEXT NOT NULL,
    service_subject TEXT NOT NULL,
    permission_ceiling_json TEXT NOT NULL,
    allowed_origins_json TEXT NOT NULL,
    resource_types_json TEXT NOT NULL,
    secret_reference TEXT,
    policy_digest TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (workspace_id, bound_project_id)
        REFERENCES workspace_projects (workspace_id, project_id)
);

CREATE TABLE IF NOT EXISTS workspace_grants (
    workspace_id TEXT NOT NULL,
    subject TEXT NOT NULL,
    project_id TEXT NOT NULL,
    granted_at TEXT NOT NULL,
    PRIMARY KEY (workspace_id, subject, project_id),
    FOREIGN KEY (workspace_id, project_id)
        REFERENCES workspace_projects (workspace_id, project_id)
);

CREATE INDEX IF NOT EXISTS workspace_profiles_workspace_idx
    ON workspace_profiles (workspace_id);
CREATE INDEX IF NOT EXISTS workspace_grants_subject_idx
    ON workspace_grants (subject);
