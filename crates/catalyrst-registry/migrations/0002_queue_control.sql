-- catalyrst-ab-registry admin-console controls (docs/admin-console.md §4 LATER).
-- This crate does not run a build worker — queue status is DERIVED from the
-- content DB + abgen's on-disk manifests. These tables hold the catalyrst-owned
-- *control plane*: an operator-set pause flag and recorded retry intents that a
-- build runner (abgen) can consult. epoch-ms bigints match the 0001 convention.

-- Singleton queue pause flag. Row id is pinned to 1 (single-row table).
CREATE TABLE IF NOT EXISTS queue_control (
    id         INTEGER PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    paused     BOOLEAN NOT NULL DEFAULT false,
    updated_by TEXT,
    updated_at BIGINT NOT NULL DEFAULT (extract(epoch from now()) * 1000)::bigint
);

INSERT INTO queue_control (id, paused) VALUES (1, false)
ON CONFLICT (id) DO NOTHING;

-- Recorded retry requests for failed/stuck builds. One row per (entity_id),
-- upserted on re-request; a build runner clears or re-reads these out-of-band.
CREATE TABLE IF NOT EXISTS build_retries (
    entity_id    TEXT PRIMARY KEY,
    requested_by TEXT,
    requested_at BIGINT NOT NULL DEFAULT (extract(epoch from now()) * 1000)::bigint,
    attempts     INTEGER NOT NULL DEFAULT 1
);
