-- The read surface for /api/events. Populated by src/mirror (upstream mirror of
-- events.decentraland.org) or, on a lore-hosted node, by the retired external
-- bootstrap — the sqlx migrator only owns the federation overlay in 0001, so a
-- node without either has no `event` table and every read 500s. IF NOT EXISTS so
-- a node that already carries the externally-bootstrapped table is a no-op.
CREATE TABLE IF NOT EXISTS event (
  id              TEXT PRIMARY KEY,
  name            TEXT NOT NULL,
  start_at        TIMESTAMPTZ,
  finish_at       TIMESTAMPTZ,
  next_start_at   TIMESTAMPTZ,
  next_finish_at  TIMESTAMPTZ,
  duration_ms     BIGINT,
  recurrent       BOOLEAN NOT NULL DEFAULT false,
  highlighted     BOOLEAN NOT NULL DEFAULT false,
  trending        BOOLEAN NOT NULL DEFAULT false,
  approved        BOOLEAN NOT NULL DEFAULT false,
  attending       BOOLEAN,
  community_id    TEXT,
  user_creator    TEXT,
  coordinates_x   INTEGER,
  coordinates_y   INTEGER,
  description     TEXT,
  raw             JSONB NOT NULL,
  fetched_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS event_coords_idx ON event (coordinates_x, coordinates_y);
CREATE INDEX IF NOT EXISTS event_next_start_idx ON event (next_start_at);
CREATE INDEX IF NOT EXISTS event_approved_live_idx ON event (approved, next_finish_at);
