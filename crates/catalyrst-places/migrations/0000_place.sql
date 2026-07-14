-- Base catalog table. Federation-native nodes populate it from their own
-- content deployments (src/catalog); the retired upstream scrape wrote the same
-- shape. 0002-0004 layer place_world_local, the generated world/plain-text
-- columns and the place_indexed view over both legs on top of this.
CREATE TABLE IF NOT EXISTS place (
  id              TEXT PRIMARY KEY,
  base_position   TEXT NOT NULL,
  title           TEXT,
  description     TEXT,
  creator_address TEXT,
  content_rating  TEXT,
  categories      TEXT[] NOT NULL DEFAULT '{}',
  likes           INTEGER NOT NULL DEFAULT 0,
  dislikes        INTEGER NOT NULL DEFAULT 0,
  favorites       INTEGER NOT NULL DEFAULT 0,
  deployed_at     TIMESTAMPTZ,
  disabled        BOOLEAN NOT NULL DEFAULT false,
  highlighted     BOOLEAN NOT NULL DEFAULT false,
  raw             JSONB NOT NULL,
  fetched_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS place_base_position_idx ON place (base_position);
