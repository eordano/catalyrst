-- Creator-authored text enters this archive unfiltered. Both writers sit
-- outside catalyrst-places: the deployment's world-places sync copies
-- scene.description and scene.thumbnail straight off our own worlds index, and
-- sync-archive-copies.sh TRUNCATE+reloads `place` from the upstream Genesis
-- City catalog. The crate can therefore only gate what it serves, and every
-- other reader of this database — search, feeds, exports — sees raw markup.
--
-- These columns are the write-time half of that gate: a STORED generated
-- column is computed as the row lands and recomputed by the data-only reload.
--
-- description_plain runs as many strip passes as src/sanitize.rs and then
-- drops every angle bracket that survived them — the sanitizer's fail-closed
-- end state, applied unconditionally because SQL cannot cheaply test for
-- convergence. The column therefore never contains `<` or `>`: no tag, no
-- dangling `<link` opener, and no benign bracket a description happened to
-- carry. It is not the served text either — it drops the safe `<link>` tags
-- the API preserves, and a word nested deeper than those passes survives here
-- as bare text exactly as it survives into a served description. Its
-- expression is mirrored by description_plain_sql() in
-- src/ports/places/query.rs, which pins the two against drift; change them
-- together.
--
-- image_url is accept-or-null, not the normalized form sanitize_image_url
-- returns — SQL cannot percent-encode — so a consumer must not expect the two
-- to be string-equal, only that this one carries no character that can break
-- out of an HTML attribute.
--
-- Column order matters: both legs of the place_indexed UNION must stay
-- `SELECT *`-compatible. Add description_plain then image_url to `place`, then
-- the same pair to place_world_local.
--
-- Single transaction on purpose: the recreate path below must DROP place_indexed
-- before it can drop the columns the view selects, so an abort between that drop
-- and the CREATE at the tail would otherwise leave the database with no
-- place_indexed at all. Both appliers invoke this file as its own `psql -f`, so
-- nothing wraps it for us.

BEGIN;

-- DROP VIEW discards the view's grants. bootstrap-places.sh re-grants SELECT to
-- the places API user afterwards, but nothing restores a grant held by any other
-- role, so capture them here and replay them once the view is back.
DO $$
BEGIN
    IF to_regclass(current_schema() || '.place_indexed') IS NOT NULL THEN
        CREATE TEMP TABLE place_indexed_grants ON COMMIT DROP AS
        SELECT grantee, privilege_type
        FROM information_schema.role_table_grants
        WHERE table_schema = current_schema()
          AND table_name = 'place_indexed';
    END IF;
END $$;

ALTER TABLE place
    ADD COLUMN IF NOT EXISTS description_plain text
    GENERATED ALWAYS AS (
        CASE WHEN strpos(coalesce(description, ''), '<') = 0
            AND strpos(coalesce(description, ''), '>') = 0
            THEN coalesce(description, '')
            ELSE regexp_replace(regexp_replace(regexp_replace(regexp_replace(regexp_replace(regexp_replace(coalesce(description, ''),
                '</?[a-zA-Z][^>]*>', '', 'g'),
                '</?[a-zA-Z][^>]*>', '', 'g'),
                '</?[a-zA-Z][^>]*>', '', 'g'),
                '</?[a-zA-Z][^>]*>', '', 'g'),
                '</?[a-zA-Z][^>]*>', '', 'g'),
                '[<>]', '', 'g')
        END
    ) STORED;

ALTER TABLE place
    ADD COLUMN IF NOT EXISTS image_url text
    GENERATED ALWAYS AS (
        CASE WHEN raw->>'image' ~* '^https?://[^[:space:]"''`<>]+$'
             THEN raw->>'image'
        END
    ) STORED;

-- 0002 builds place_world_local with `LIKE place`, which copies these columns
-- without their generation expressions; the ADD COLUMN IF NOT EXISTS below
-- would then leave them permanently NULL on a table recreated after this
-- migration. Re-add them generated instead.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = current_schema()
          AND table_name = 'place_world_local'
          AND column_name IN ('description_plain', 'image_url')
          AND is_generated = 'NEVER'
    ) THEN
        DROP VIEW IF EXISTS place_indexed;
        ALTER TABLE place_world_local DROP COLUMN IF EXISTS description_plain;
        ALTER TABLE place_world_local DROP COLUMN IF EXISTS image_url;
    END IF;
END $$;

ALTER TABLE place_world_local
    ADD COLUMN IF NOT EXISTS description_plain text
    GENERATED ALWAYS AS (
        CASE WHEN strpos(coalesce(description, ''), '<') = 0
            AND strpos(coalesce(description, ''), '>') = 0
            THEN coalesce(description, '')
            ELSE regexp_replace(regexp_replace(regexp_replace(regexp_replace(regexp_replace(regexp_replace(coalesce(description, ''),
                '</?[a-zA-Z][^>]*>', '', 'g'),
                '</?[a-zA-Z][^>]*>', '', 'g'),
                '</?[a-zA-Z][^>]*>', '', 'g'),
                '</?[a-zA-Z][^>]*>', '', 'g'),
                '</?[a-zA-Z][^>]*>', '', 'g'),
                '[<>]', '', 'g')
        END
    ) STORED;

ALTER TABLE place_world_local
    ADD COLUMN IF NOT EXISTS image_url text
    GENERATED ALWAYS AS (
        CASE WHEN raw->>'image' ~* '^https?://[^[:space:]"''`<>]+$'
             THEN raw->>'image'
        END
    ) STORED;

CREATE OR REPLACE VIEW place_indexed AS
    SELECT * FROM place
    UNION ALL
    SELECT * FROM place_world_local;

DO $$
DECLARE
    g record;
BEGIN
    IF to_regclass('pg_temp.place_indexed_grants') IS NULL THEN
        RETURN;
    END IF;
    FOR g IN SELECT DISTINCT grantee, privilege_type FROM place_indexed_grants LOOP
        IF g.grantee = 'PUBLIC' THEN
            EXECUTE format('GRANT %s ON place_indexed TO PUBLIC', g.privilege_type);
        ELSIF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = g.grantee) THEN
            EXECUTE format('GRANT %s ON place_indexed TO %I', g.privilege_type, g.grantee);
        END IF;
    END LOOP;
END $$;

COMMIT;
