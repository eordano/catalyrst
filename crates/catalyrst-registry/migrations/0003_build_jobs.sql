-- catalyrst-ab-registry owned build-job queue (docs/admin-console.md §4 LATER).
-- This crate does not itself run abgen, but POST /queues/retry must produce a
-- REAL effect: reset the target entity's per-platform build status to Pending
-- and re-enqueue it so a build runner polling this table picks it up, and so
-- GET /queues/status reports it as pending. Status is DERIVED from abgen's
-- on-disk manifests; this table is the catalyrst-owned *control-plane* overlay
-- that lets an operator force-requeue an already-Complete (or stuck) build.
-- epoch-ms bigints match the 0001/0002 convention.
--
-- A worker claims rows WHERE status = 'pending' (one per entity+platform),
-- builds, then marks them 'complete' (or deletes the row). The overlay is
-- additive: GET /queues/status unions these pending rows with the disk-derived
-- pending set, never removing a job the disk manifests already expose.
CREATE TABLE IF NOT EXISTS build_jobs (
    entity_id    TEXT NOT NULL,
    platform     TEXT NOT NULL,
    status       TEXT NOT NULL DEFAULT 'pending',
    requested_by TEXT,
    enqueued_at  BIGINT NOT NULL DEFAULT (extract(epoch from now()) * 1000)::bigint,
    updated_at   BIGINT NOT NULL DEFAULT (extract(epoch from now()) * 1000)::bigint,
    PRIMARY KEY (entity_id, platform)
);

-- Worker scan path: cheaply find all pending jobs for a platform.
CREATE INDEX IF NOT EXISTS build_jobs_status_platform_idx
    ON build_jobs (status, platform);
