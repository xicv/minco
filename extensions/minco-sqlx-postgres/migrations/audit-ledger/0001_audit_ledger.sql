CREATE TABLE IF NOT EXISTS minco_audit_records (
    event_id UUID PRIMARY KEY,
    tenant_scope TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL,
    encoded_bytes INTEGER NOT NULL CHECK (encoded_bytes > 0),
    record JSONB NOT NULL
);

CREATE INDEX IF NOT EXISTS minco_audit_records_resource_history
    ON minco_audit_records
    (tenant_scope, resource_type, resource_id, occurred_at DESC, event_id DESC);

CREATE TABLE IF NOT EXISTS minco_audit_related_resources (
    event_id UUID NOT NULL,
    tenant_scope TEXT NOT NULL,
    relation TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (event_id, relation, resource_type, resource_id)
);

CREATE INDEX IF NOT EXISTS minco_audit_related_resource_history
    ON minco_audit_related_resources
    (tenant_scope, resource_type, resource_id, relation, occurred_at DESC, event_id DESC);
