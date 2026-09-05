-- Place-aware covering indexes for the per-write quota aggregation
-- (upstream 1782950400000_scope-size-indexes-by-place). place_id last so the
-- world-scoped *.dcl.eth aggregation still matches on the prefix; key and
-- value_size as INCLUDE payload keep both query shapes index-only.

CREATE INDEX IF NOT EXISTS world_storage_size_by_place_idx
    ON world_storage (world_name, place_id) INCLUDE (key, value_size);

CREATE INDEX IF NOT EXISTS player_storage_size_by_place_idx
    ON player_storage (world_name, player_address, place_id) INCLUDE (key, value_size);

CREATE INDEX IF NOT EXISTS env_variables_size_by_place_idx
    ON env_variables (world_name, place_id) INCLUDE (key, value_size);
