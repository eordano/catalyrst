-- Goal progress tracking: how a goal advances (kind) + idempotent event dedup.
--
-- `kind` picks which real user event advances a goal, mirroring the upstream
-- platform-event families (@dcl/schemas events):
--   login       <- client 'logged-in'      (here: authenticated signed fetch seen)
--   scene_visit <- client 'move-to-parcel' (dedup on scene/parcel)
--   purchase    <- blockchain 'item-sold'  (here: credits checkout fulfilled)
--   manual      <- no automatic source (admin-driven progress only)
ALTER TABLE credits_goals
    ADD COLUMN kind TEXT NOT NULL DEFAULT 'manual';

ALTER TABLE credits_goals
    ADD CONSTRAINT credits_goals_kind_check
    CHECK (kind IN ('manual', 'login', 'scene_visit', 'purchase'));

-- One row per (user, goal, dedup_key) event occurrence; completed_steps is
-- derived as count(*) capped at total_steps, so replays/rescans are no-ops.
-- dedup_key per kind: login = UTC date, scene_visit = scene id/parcel,
-- purchase = checkout id.
CREATE TABLE IF NOT EXISTS user_goal_events (
    address    TEXT NOT NULL,
    goal_id    INT NOT NULL REFERENCES credits_goals(id) ON DELETE CASCADE,
    dedup_key  TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (address, goal_id, dedup_key)
);

CREATE INDEX IF NOT EXISTS idx_user_goal_events_goal ON user_goal_events(goal_id);
