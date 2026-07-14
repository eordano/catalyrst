-- A world's identity is its name: a row flagged world=true with no world_name
-- is unreachable and unfindable, so nothing may produce one. This promotes
-- world/world_name out of `raw` into real, indexed columns on both legs of
-- place_indexed and ties the pair together.
--
-- The two legs are enforced differently because we own only one of them.
-- place_world_local is ours: plain columns plus a CHECK, so an unnamed world
-- is rejected at write time and the sync fails loudly. `place` is the
-- TRUNCATE+reload mirror of the upstream catalog (sync-archive-copies.sh
-- reloads it data-only, with an explicit column list, so generated columns
-- survive and recompute); a CHECK there would let an upstream row abort the
-- reload, so its columns are GENERATED and world folds in the name test —
-- an unnamed upstream row reads as a place, never as a nameless world.
--
-- Column order matters: both tables must stay `SELECT *`-compatible or the
-- place_indexed UNION in 0002 stops replaying. Add world_name then world.

ALTER TABLE place
    ADD COLUMN IF NOT EXISTS world_name text
    GENERATED ALWAYS AS (NULLIF(btrim(raw->>'world_name'), '')) STORED;

ALTER TABLE place
    ADD COLUMN IF NOT EXISTS world boolean NOT NULL
    GENERATED ALWAYS AS (
        CASE lower(btrim(raw->>'world'))
            WHEN 'true' THEN true
            WHEN 't' THEN true
            WHEN 'yes' THEN true
            WHEN 'on' THEN true
            WHEN '1' THEN true
            ELSE false
        END
        AND NULLIF(btrim(raw->>'world_name'), '') IS NOT NULL
    ) STORED;

CREATE INDEX IF NOT EXISTS place_world_name_col_idx
    ON place (lower(world_name));

CREATE INDEX IF NOT EXISTS place_like_score_place_only_idx
    ON place ((NULLIF(raw->>'like_score', '')::float8) DESC NULLS LAST, deployed_at DESC)
    WHERE disabled IS FALSE AND world IS FALSE;

ALTER TABLE place_world_local
    ADD COLUMN IF NOT EXISTS world_name text;

ALTER TABLE place_world_local
    ADD COLUMN IF NOT EXISTS world boolean;

UPDATE place_world_local
    SET world_name = NULLIF(btrim(raw->>'world_name'), '')
    WHERE world_name IS NULL;

UPDATE place_world_local
    SET world = COALESCE((raw->>'world')::boolean, false)
    WHERE world IS NULL;

ALTER TABLE place_world_local
    ALTER COLUMN world SET DEFAULT false;

ALTER TABLE place_world_local
    ALTER COLUMN world SET NOT NULL;

ALTER TABLE place_world_local
    DROP CONSTRAINT IF EXISTS place_world_local_world_named;

ALTER TABLE place_world_local
    ADD CONSTRAINT place_world_local_world_named
    CHECK (world IS FALSE OR (world_name IS NOT NULL AND btrim(world_name) <> ''));

CREATE INDEX IF NOT EXISTS place_world_local_world_name_col_idx
    ON place_world_local (lower(world_name));

CREATE OR REPLACE VIEW place_indexed AS
    SELECT * FROM place
    UNION ALL
    SELECT * FROM place_world_local;
