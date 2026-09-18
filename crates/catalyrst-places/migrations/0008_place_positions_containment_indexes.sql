-- Places by position: the list query tests raw->'positions' @> ANY($n::jsonb[]) so
-- both legs of place_indexed walk a containment-only GIN (jsonb_path_ops is smaller and
-- cheaper for @> than jsonb_ops; place_raw_positions_gin stays for the ?| road sync).
CREATE INDEX IF NOT EXISTS place_positions_path_gin
    ON place USING gin ((raw->'positions') jsonb_path_ops);

CREATE INDEX IF NOT EXISTS place_world_local_positions_path_gin
    ON place_world_local USING gin ((raw->'positions') jsonb_path_ops);
