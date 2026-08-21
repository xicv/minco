CREATE TABLE IF NOT EXISTS minco_jobs (
    job_id TEXT PRIMARY KEY NOT NULL,
    worker_profile TEXT NOT NULL,
    envelope TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'running', 'succeeded', 'failed_permanently', 'cancelled')),
    revision INTEGER NOT NULL CHECK (revision > 0),
    available_at TEXT NOT NULL,
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    lease_owner TEXT,
    lease_expires_at TEXT,
    attempts TEXT NOT NULL DEFAULT '[]',
    dedupe_key TEXT,
    failure_code TEXT,
    completed_at TEXT
);

CREATE UNIQUE INDEX IF NOT EXISTS minco_jobs_dedupe_key
    ON minco_jobs (dedupe_key) WHERE dedupe_key IS NOT NULL;

CREATE INDEX IF NOT EXISTS minco_jobs_failed
    ON minco_jobs (status, completed_at, job_id) WHERE status = 'failed_permanently';

CREATE TABLE IF NOT EXISTS minco_job_publications (
    job_id TEXT PRIMARY KEY NOT NULL REFERENCES minco_jobs (job_id) ON DELETE CASCADE,
    worker_profile TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'claimed', 'published', 'failed')),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    available_at TEXT NOT NULL,
    claimed_by TEXT,
    claim_expires_at TEXT,
    last_error TEXT
);

CREATE INDEX IF NOT EXISTS minco_job_publications_dispatch
    ON minco_job_publications (status, available_at, job_id);

CREATE TABLE IF NOT EXISTS minco_job_locks (
    overlap_key TEXT PRIMARY KEY NOT NULL,
    owner TEXT NOT NULL,
    expires_at TEXT NOT NULL
);
