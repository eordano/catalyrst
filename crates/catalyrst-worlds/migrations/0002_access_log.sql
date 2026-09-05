-- catalyrst-worlds: world access log.
--
-- The LiveKit webhook (POST /livekit-webhook) already receives
-- participant_joined / participant_left events for every world/scene room. Until
-- now those events only updated the in-memory presence registry. This table
-- persists them so operators have a durable access log of who entered/left which
-- world, queryable through the bearer-gated GET /admin/access-log admin view.

CREATE TABLE IF NOT EXISTS world_access_log (
    id         BIGSERIAL PRIMARY KEY,
    world_name VARCHAR NOT NULL,
    address    VARCHAR NOT NULL,
    -- "join" | "leave"
    action     VARCHAR NOT NULL,
    -- the LiveKit room the event was observed on (world or scene room)
    room       VARCHAR NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS world_access_log_world_idx
    ON world_access_log (lower(world_name), created_at DESC);
CREATE INDEX IF NOT EXISTS world_access_log_address_idx
    ON world_access_log (lower(address), created_at DESC);
CREATE INDEX IF NOT EXISTS world_access_log_created_idx
    ON world_access_log (created_at DESC);
