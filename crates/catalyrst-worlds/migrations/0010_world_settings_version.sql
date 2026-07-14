-- Monotonic per-world settings version (upstream 0025_add_world_settings_version).
-- Every settings-affecting write (deploy refresh, PUT /settings, access changes,
-- mirror ingest) bumps it under the worlds row lock, ordering settings writes by
-- commit order; consumers mirroring settings compare it to reject out-of-order
-- updates. spawn_coordinates is not a versioned settings column on any path.

ALTER TABLE worlds ADD COLUMN IF NOT EXISTS settings_version BIGINT NOT NULL DEFAULT 0;
