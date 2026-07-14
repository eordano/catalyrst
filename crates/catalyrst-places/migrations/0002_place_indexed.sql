-- place_indexed is the read surface for every place/world query in this crate.
--
-- `place` is a TRUNCATE+reload mirror of the upstream Genesis City catalog
-- and upstream carries no worlds at
-- all, so world-scoped queries over it match nothing. place_world_local holds
-- the worlds our own catalyrst-worlds serves; it sits outside that reload's
-- table list and survives it. Refilled by the deployment's own world-places sync.

CREATE TABLE IF NOT EXISTS place_world_local (LIKE place INCLUDING DEFAULTS);

CREATE UNIQUE INDEX IF NOT EXISTS place_world_local_pkey
    ON place_world_local (id);

CREATE INDEX IF NOT EXISTS place_world_local_world_name_idx
    ON place_world_local (lower(raw->>'world_name'));

CREATE OR REPLACE VIEW place_indexed AS
    SELECT * FROM place
    UNION ALL
    SELECT * FROM place_world_local;
