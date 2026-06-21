-- Provenance for entities published directly against this node's write API
-- (POST /content/entities on this node) as opposed to rows mirrored from
-- upstream Genesis City by the sync process. Additive only: shared tables
-- (deployments/content_files/active_pointers) are untouched.
--
-- Apply like 0001/0002: psql -f against the content DB, never sqlx::migrate!
-- (catalyrst-media owns the shared _sqlx_migrations table). Idempotent.

CREATE TABLE IF NOT EXISTS local_entities (
    entity_id     text PRIMARY KEY,
    signer        text NOT NULL,
    origin        text NOT NULL DEFAULT 'land-publish',
    published_at  timestamptz NOT NULL DEFAULT now(),
    tombstoned_at timestamptz
);

CREATE INDEX IF NOT EXISTS local_entities_tombstoned_idx
    ON local_entities (tombstoned_at) WHERE tombstoned_at IS NOT NULL;
