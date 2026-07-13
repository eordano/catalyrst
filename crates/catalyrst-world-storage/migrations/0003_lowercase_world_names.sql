-- Lowercase every stored world_name (upstream 1782950500000_lowercase-world-names):
-- realm names are lowercased before they reach storage now, so mixed-case rows would
-- become unreachable. Groups that collapse to the same lowercase identity keep one
-- deterministic survivor: an already-lowercase row wins, else the most recently updated.

DELETE FROM world_storage
WHERE ctid IN (
    SELECT ctid FROM (
        SELECT ctid,
            ROW_NUMBER() OVER (
                PARTITION BY lower(world_name), place_id, key
                ORDER BY (world_name = lower(world_name)) DESC, updated_at DESC, ctid
            ) AS rn
        FROM world_storage
    ) ranked
    WHERE ranked.rn > 1
);

UPDATE world_storage SET world_name = lower(world_name) WHERE world_name <> lower(world_name);

DELETE FROM player_storage
WHERE ctid IN (
    SELECT ctid FROM (
        SELECT ctid,
            ROW_NUMBER() OVER (
                PARTITION BY lower(world_name), place_id, player_address, key
                ORDER BY (world_name = lower(world_name)) DESC, updated_at DESC, ctid
            ) AS rn
        FROM player_storage
    ) ranked
    WHERE ranked.rn > 1
);

UPDATE player_storage SET world_name = lower(world_name) WHERE world_name <> lower(world_name);

DELETE FROM env_variables
WHERE ctid IN (
    SELECT ctid FROM (
        SELECT ctid,
            ROW_NUMBER() OVER (
                PARTITION BY lower(world_name), place_id, key
                ORDER BY (world_name = lower(world_name)) DESC, updated_at DESC, ctid
            ) AS rn
        FROM env_variables
    ) ranked
    WHERE ranked.rn > 1
);

UPDATE env_variables SET world_name = lower(world_name) WHERE world_name <> lower(world_name);
