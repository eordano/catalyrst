-- Per-server sync cursors: each peer content server's own confirmed resume point,
-- advanced GREATEST-monotonically at the same durable points as the global
-- 'sync_frontier' scalar (pointer-changes poll boundaries and snapshot-bootstrap
-- completion) but for that server only. Bootstrap resumes each server from its own
-- row, so a server that never completed bootstrap is no longer fast-forwarded past
-- its undeployed entities by the max-over-servers frontier (which stays exactly
-- as-is, feeding the freshness gauge and external consumers). Rows outlive
-- server-set changes on purpose: removing a server from sync leaves its cursor,
-- and re-adding the server resumes correctly. Until this migration is applied the
-- code degrades to the old global-frontier resume (a missing table reads as
-- "no cursor").
--
-- Apply like 0001/0002: psql -f against the content DB, never sqlx::migrate!
-- (catalyrst-media owns the shared _sqlx_migrations table). Idempotent.

CREATE TABLE IF NOT EXISTS server_sync_cursors (
    server_url text NOT NULL,
    cursor_ms  bigint NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT server_sync_cursors_pkey PRIMARY KEY (server_url)
);
