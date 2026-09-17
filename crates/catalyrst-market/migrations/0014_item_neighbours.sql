-- Precomputed item-item neighbours behind /v3/catalog/suggested (port of marketplace-server
-- src/migrations/dapps/1789300000000_item-neighbors.ts). One row per (anchor, source,
-- neighbour), where source is the co-ownership generator ('cf') or the content one
-- ('content'). Recomputed in full every few hours and swapped in, so the table is a cache
-- with a known shape rather than something to maintain incrementally.
--
-- Created empty. The endpoint answers with the trending fallback until the first rebuild
-- populates it, which is the same answer it gives a wallet with no signal, so deploying this
-- ahead of the job is safe.

CREATE TABLE IF NOT EXISTS marketplace.item_neighbors (
  item_id     text NOT NULL,
  source      text NOT NULL,
  neighbor_id text NOT NULL,
  sim         real NOT NULL,
  support     integer NOT NULL DEFAULT 0,
  rank        smallint NOT NULL,
  PRIMARY KEY (item_id, source, neighbor_id)
);

CREATE INDEX IF NOT EXISTS idx_item_neighbors_item_id ON marketplace.item_neighbors (item_id);

-- One row, rewritten by every successful rebuild. It exists so "the job stopped running" is a
-- query rather than a log search: `built_at` going stale is the alertable condition.
CREATE TABLE IF NOT EXISTS marketplace.item_neighbors_meta (
  id            boolean PRIMARY KEY DEFAULT true,
  built_at      timestamptz NOT NULL,
  duration_ms   integer NOT NULL,
  cf_rows       integer NOT NULL,
  content_rows  integer NOT NULL,
  items_covered integer NOT NULL,
  algorithm     text NOT NULL,
  CONSTRAINT item_neighbors_meta_singleton CHECK (id)
);
