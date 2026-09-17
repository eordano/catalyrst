-- 0006: per-entity retry backoff state on failed_deployments.
-- retry_count holds the attempts already spent, next_retry_at the earliest
-- instant the entry may be tried again: the worker skips entries that are not
-- due yet and evicts the ones that reached MAX_FAILED_DEPLOYMENT_RETRIES.
--
-- timestamptz, unlike the failure_time column beside it: next_retry_at is an
-- absolute deadline, written from an epoch and read back as one, and a
-- timezone-less column would shift it by the session's UTC offset on every
-- read and write.
--
-- Apply like 0001/0002: psql -f against the content DB, never sqlx::migrate!
-- (catalyrst-media owns the shared _sqlx_migrations table). Idempotent.

ALTER TABLE public.failed_deployments
    ADD COLUMN IF NOT EXISTS retry_count integer NOT NULL DEFAULT 0;

ALTER TABLE public.failed_deployments
    ADD COLUMN IF NOT EXISTS next_retry_at timestamptz NOT NULL DEFAULT NOW();
