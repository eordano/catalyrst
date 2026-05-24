-- catalyrst-db: indexes on the marketplace_squid DB (NOT the content DB).
--
-- These cover the hot lookups behind:
--   /lambdas/explorer/{addr}/wearables     (and /emotes)
--   /lambdas/profiles/{addr}               (ownership validation)
--   /lambdas/users/{addr}/wearables        (and /emotes)
--
-- All three families filter by `owner_address = lower($addr)` and either
-- `category` (explorer/users) or `urn = ANY(...)` (profile ownership-check).
--
-- Without these indexes the first call after a cache miss can take
-- 100-300 ms per request. With them it drops to <10 ms.
--
-- Run on the squid DB (NOT the content DB):
--   psql "$SQUID_DATABASE_URL" -f 0002_squid_lookup_indexes.sql

-- Covers /explorer/{addr}/wearables, /users/{addr}/wearables, /users/{addr}/emotes:
--   WHERE n.category=$1 AND n.urn IS NOT NULL AND n.owner_address=lower($2)
--   ORDER BY n.transferred_at DESC
CREATE INDEX CONCURRENTLY IF NOT EXISTS
  nft_category_owner_transferred
  ON squid_marketplace.nft (category, owner_address, transferred_at DESC)
  WHERE urn IS NOT NULL;

-- Covers the exact-urn-match leg of profile ownership validation:
--   WHERE owner_address=lower($1) AND urn = ANY($2)
CREATE INDEX CONCURRENTLY IF NOT EXISTS
  nft_owner_urn
  ON squid_marketplace.nft (owner_address, urn)
  WHERE urn IS NOT NULL;

-- The prefix-match leg uses `left(n.urn, length(p)) = p` which a regular
-- B-tree index cannot directly serve; but with the above (owner_address, urn)
-- index in place the planner narrows to the owner's NFTs first, then the
-- per-row `left()` evaluation is bounded by the owner's NFT count (typically
-- < 1000). No third-party-style trigram index needed at this scale.
