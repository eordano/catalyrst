-- Lowercase every stored world_name (upstream 1782950500000_lowercase-world-names):
-- realm names are lowercased before they reach storage now, so mixed-case rows would
-- become unreachable. Upstream dedupes colliding groups to a deterministic survivor
-- (already-lowercase row wins, else most recently updated) and DELETEs the rest; we
-- refuse instead — abort the whole migration listing the colliding rows so an operator
-- can apply that rule by hand. No data is ever dropped here.

DO $$
DECLARE
    lines text[];
BEGIN
    SELECT array_agg(line ORDER BY line) INTO lines FROM (
        SELECT format('world_storage world_name=%L place_id=%s key=%L updated_at=%s',
                      world_name, place_id, key, updated_at) AS line
        FROM world_storage
        WHERE (lower(world_name), place_id, key) IN (
            SELECT lower(world_name), place_id, key
            FROM world_storage GROUP BY 1, 2, 3 HAVING count(*) > 1)
        UNION ALL
        SELECT format('player_storage world_name=%L place_id=%s player_address=%L key=%L updated_at=%s',
                      world_name, place_id, player_address, key, updated_at)
        FROM player_storage
        WHERE (lower(world_name), place_id, player_address, key) IN (
            SELECT lower(world_name), place_id, player_address, key
            FROM player_storage GROUP BY 1, 2, 3, 4 HAVING count(*) > 1)
        UNION ALL
        SELECT format('env_variables world_name=%L place_id=%s key=%L updated_at=%s',
                      world_name, place_id, key, updated_at)
        FROM env_variables
        WHERE (lower(world_name), place_id, key) IN (
            SELECT lower(world_name), place_id, key
            FROM env_variables GROUP BY 1, 2, 3 HAVING count(*) > 1)
    ) colliding;
    IF lines IS NOT NULL THEN
        RAISE EXCEPTION E'lowercase world_name collision: % row(s) would merge; resolve manually, no data was changed:\n%',
            array_length(lines, 1), array_to_string(lines[1:50], E'\n');
    END IF;
END $$;

UPDATE world_storage SET world_name = lower(world_name) WHERE world_name <> lower(world_name);

UPDATE player_storage SET world_name = lower(world_name) WHERE world_name <> lower(world_name);

UPDATE env_variables SET world_name = lower(world_name) WHERE world_name <> lower(world_name);
