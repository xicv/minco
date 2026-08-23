CREATE TABLE IF NOT EXISTS minco_jobs (
    job_id UUID PRIMARY KEY,
    worker_profile TEXT NOT NULL,
    envelope JSONB NOT NULL,
    fingerprint TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'running', 'succeeded', 'failed_permanently', 'cancelled')),
    revision BIGINT NOT NULL CHECK (revision > 0),
    available_at TIMESTAMPTZ NOT NULL,
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    lease_id UUID,
    lease_expires_at TIMESTAMPTZ,
    attempts JSONB NOT NULL DEFAULT '[]'::jsonb,
    dedupe_key TEXT,
    failure_code TEXT,
    completed_at TIMESTAMPTZ
);

CREATE UNIQUE INDEX IF NOT EXISTS minco_jobs_dedupe_key
    ON minco_jobs (dedupe_key) WHERE dedupe_key IS NOT NULL;

CREATE INDEX IF NOT EXISTS minco_jobs_failed
    ON minco_jobs (status, completed_at, job_id) WHERE status = 'failed_permanently';

CREATE TABLE IF NOT EXISTS minco_job_publications (
    publication_id UUID PRIMARY KEY,
    job_id UUID NOT NULL REFERENCES minco_jobs (job_id) ON DELETE CASCADE,
    generation INTEGER NOT NULL CHECK (generation > 0),
    worker_profile TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'claimed', 'published', 'failed')),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    available_at TIMESTAMPTZ NOT NULL,
    claimed_by TEXT,
    claim_expires_at TIMESTAMPTZ,
    lease_id UUID,
    last_error TEXT,
    UNIQUE (job_id, generation)
);

CREATE INDEX IF NOT EXISTS minco_job_publications_dispatch
    ON minco_job_publications (status, available_at, job_id);

CREATE TABLE IF NOT EXISTS minco_job_locks (
    overlap_key TEXT PRIMARY KEY,
    owner TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL
);
