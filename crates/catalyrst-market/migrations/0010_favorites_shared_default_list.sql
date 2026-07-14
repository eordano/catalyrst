-- Server-side favorites parity for the shop (shop c0cc5df "persist favorites
-- server-side via the marketplace favorites service"), aligning our favorites
-- schema with upstream marketplace-server's favorites migrations:
--
-- 1. `favorites.acl` — per-list grants (upstream 1683320488882_acl). Read by
--    `check_non_editable_lists` (POST /v1/picks/{itemId}) and
--    `get_picks_by_list_id` (GET /v1/lists/{id}/picks). Upstream types
--    `permission` as an enum ('edit'|'view'); ours stays text to match
--    `favorites.lists.permission` — the queries compare against the same
--    literals either way.
--
-- 2. Picks identity widened to (item_id, user_address, list_id) — upstream's
--    primary key since 1677778846950_lists-and-picks. Two users favoriting the
--    same item in a SHARED list are two distinct picks; the old
--    (item_id, list_id) key made the second user's favorite silently collide
--    with the first's. Also the ON CONFLICT target `pick_in_lists` now uses.
--    Widening a key cannot fail on existing data (the old key was stricter).
--
-- 3. Seed the globally shared default list the shop frontend hardcodes
--    (upstream 1678303321034_default-list, renamed "Wishlist" by
--    1687172729802_change-default-list-name-and-description). Owned by the
--    zero address; every signed-in user picks into it, and each sees only
--    their own picks back (the visibility WHERE in get_picks_by_list_id).
--
-- Idempotent, no BEGIN/COMMIT (sqlx wraps each migration in a transaction).

CREATE TABLE IF NOT EXISTS favorites.acl (
    list_id    uuid NOT NULL REFERENCES favorites.lists(id) ON DELETE CASCADE,
    permission text NOT NULL,
    grantee    text NOT NULL,
    PRIMARY KEY (list_id, permission, grantee)
);

CREATE INDEX IF NOT EXISTS acl_list_id_idx ON favorites.acl (list_id);

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM pg_constraint c
        JOIN pg_class t ON t.oid = c.conrelid
        JOIN pg_namespace n ON n.oid = t.relnamespace
        WHERE n.nspname = 'favorites' AND t.relname = 'picks'
          AND c.conname = 'picks_pkey' AND c.contype = 'p'
          AND (
            SELECT array_agg(a.attname ORDER BY k.ord)
            FROM unnest(c.conkey) WITH ORDINALITY AS k(attnum, ord)
            JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = k.attnum
          ) = ARRAY['item_id', 'list_id']::name[]
    ) THEN
        ALTER TABLE favorites.picks DROP CONSTRAINT picks_pkey;
        ALTER TABLE favorites.picks
            ADD CONSTRAINT picks_pkey PRIMARY KEY (item_id, user_address, list_id);
    END IF;
END
$$;

INSERT INTO favorites.lists (id, name, description, user_address, is_private)
VALUES (
    '70ab6873-4a03-4eb2-b331-4b8be0e0b8af',
    'Wishlist',
    'Find all your wished items here',
    '0x0000000000000000000000000000000000000000',
    false
)
ON CONFLICT (id) DO NOTHING;
