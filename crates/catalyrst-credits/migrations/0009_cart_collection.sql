-- catalyrst-credits: carry the collection contract address on cart lines and
-- fulfilment rows (Landiler BUG 1 fix).
--
-- A specific priced catalog item is resolved ONLY by (collection, itemId)
-- together: itemId alone matches the per-collection blockchain index and the
-- market returns the NEWEST item with that index (a free mint, price 0), so a
-- specific priced item was unreachable and every checkout debited 0. We now
-- thread the collection from add -> cart -> checkout re-price -> fulfilment
-- outbox -> broker buy so the SAME specific item is priced and bought.
--
-- Additive only: ADD COLUMN IF NOT EXISTS (nullable, no default) so this replays
-- cleanly and never rewrites existing rows wholesale. No BEGIN/COMMIT here —
-- sqlx wraps each migration in its own transaction.

ALTER TABLE cart_items         ADD COLUMN IF NOT EXISTS collection TEXT;
ALTER TABLE fulfillment_outbox ADD COLUMN IF NOT EXISTS collection TEXT;

-- Backfill any pre-existing row from its stored urn
-- (urn:decentraland:matic:collections-v2:0xCONTRACT:INDEX -> 0xCONTRACT, the
-- 5th ':'-delimited field), lowercased to match the market's contractAddress.
-- Reads also COALESCE this same derivation, so the backfill is belt-and-braces.
UPDATE cart_items
   SET collection = lower(split_part(urn, ':', 5))
 WHERE collection IS NULL
   AND urn LIKE 'urn:%:%:%:0x%:%';

UPDATE fulfillment_outbox
   SET collection = lower(split_part(urn, ':', 5))
 WHERE collection IS NULL
   AND urn LIKE 'urn:%:%:%:0x%:%';

-- Widen the cart-line identity to (cart_id, collection, item_id). itemId alone is
-- only a per-collection blockchain index, so nearly EVERY collection has an item
-- "0", "1", ... — the old UNIQUE(cart_id, item_id) made adding (0xBBB, 0) silently
-- morph an existing (0xAAA, 0) row (ON CONFLICT) and remove/clear hit the wrong
-- collection's line. The collection must distinguish the line. Backfill above runs
-- FIRST so pre-0009 rows carry a non-NULL collection before the key is widened.
ALTER TABLE cart_items DROP CONSTRAINT IF EXISTS cart_items_cart_id_item_id_key;
CREATE UNIQUE INDEX IF NOT EXISTS cart_items_cart_collection_item_key
    ON cart_items (cart_id, collection, item_id);
