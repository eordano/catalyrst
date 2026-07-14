-- E2E `place` table — the exact schema sites/app/lib/catalyst/db-query.ts queries.
--
-- Derived from PLACE_COLUMNS / buildWhere / buildListQuery / CATEGORIES_SQL in
-- db-query.ts (itself a faithful port of catalyrst-places/src/ports/places.rs).
--
-- Rule: any identifier referenced WITHOUT `raw->` is a top-level column; every
-- `raw->>'X'` / `raw->'X'` field lives inside the `raw` jsonb. The FTS bits the
-- query relies on (to_tsvector / plainto_tsquery / ts_rank_cd) are computed
-- INLINE in the query against the default `english` config — no generated column
-- or extension is needed. unnest(categories) + jsonb_array_elements_text are
-- built-in. So this single table is everything the app's SQL touches.

CREATE TABLE place (
  id              text PRIMARY KEY,
  title           text,
  description     text,
  creator_address text,                                 -- SELECTed AS owner and raw
  base_position   text NOT NULL,                        -- schema.ts: z.string()
  content_rating  text,
  disabled        boolean NOT NULL DEFAULT false,       -- `disabled IS FALSE` in every WHERE
  favorites       integer NOT NULL DEFAULT 0,
  likes           integer NOT NULL DEFAULT 0,
  dislikes        integer NOT NULL DEFAULT 0,
  categories      text[]  NOT NULL DEFAULT '{}',        -- `categories && $n`, unnest() in CATEGORIES_SQL
  highlighted     boolean NOT NULL DEFAULT false,
  deployed_at     timestamptz NOT NULL,                 -- ORDER BY deployed_at DESC (deterministic)
  raw             jsonb   NOT NULL DEFAULT '{}'::jsonb  -- all raw->> / raw-> fields below
);

-- Fields that live INSIDE `raw` (every distinct raw->> / raw-> key in db-query.ts):
--   image, positions(array), contact_name, contact_email, disabled_at,
--   disabled_reason, created_at, updated_at, tags(array), highlighted_image,
--   ranking, sdk, world(bool), world_name, world_id, is_private(bool),
--   user_count, user_visits, like_rate, like_score.
-- The seed stores each fixture entry's whole object as `raw`, so positions/tags
-- are real JSON arrays (for jsonb_array_elements_text) and numeric/boolean
-- scalars stay JSON numbers/booleans (for NULLIF(...)::float8 / ::bool).
