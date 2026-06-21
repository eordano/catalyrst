CREATE INDEX IF NOT EXISTS worlds_lower_name_idx ON worlds (lower(name));

CREATE INDEX IF NOT EXISTS world_scenes_lower_world_name_idx
    ON world_scenes (lower(world_name), created_at DESC);
