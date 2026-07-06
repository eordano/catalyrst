-- catalyrst-ab-registry owned schema. Minimal, catalyrst-only state — the
-- registry build status / versions / bundles stay DERIVED from the content DB
-- and abgen's on-disk manifests and are NOT stored here.

-- Entities excluded from /entities/active. Columns mirror upstream
-- `Denylist.DbEntity` ({entity_id, reason, created_by, created_at, updated_at})
-- so admin/moderation tooling sees the same row shape. created_at/updated_at are
-- epoch-ms bigints, matching upstream's Date.now() storage.
CREATE TABLE IF NOT EXISTS denylist (
    entity_id  TEXT PRIMARY KEY,
    reason     TEXT,
    created_by TEXT,
    created_at BIGINT NOT NULL DEFAULT (extract(epoch from now()) * 1000)::bigint,
    updated_at BIGINT NOT NULL DEFAULT (extract(epoch from now()) * 1000)::bigint
);

-- Optional per-world spawn coordinate override. When is_user_set is true the
-- world manifest reports (x, y) instead of the scene base parcel.
CREATE TABLE IF NOT EXISTS world_spawn_coordinates (
    world_name  TEXT PRIMARY KEY,
    x           BIGINT NOT NULL,
    y           BIGINT NOT NULL,
    is_user_set BOOLEAN NOT NULL DEFAULT false,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
