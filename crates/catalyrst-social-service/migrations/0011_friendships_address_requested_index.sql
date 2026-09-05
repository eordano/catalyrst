-- no-transaction
-- Ported from decentraland/social-service-ea:
--   1786500000000_add-friendships-address-requested-index (#475)
--
-- Every friendship read is `(address_requester = $me OR address_requested = $me)`. Only the
-- requester arm was indexed (0008), and the composite unique cannot serve the other arm because
-- address_requester leads it, so Postgres could not build a bitmap union and scanned the whole
-- table. getMutualFriends is the worst caller: the shape appears twice and rows+count run
-- separately, so one listing is four scans.
--
-- Built CONCURRENTLY so the build never holds a lock that blocks writes to the hot friendships
-- table; CONCURRENTLY cannot run inside a transaction, hence the no-transaction directive above
-- (sqlx wraps every other migration in one). Plain btree, matching upstream's added arm — not
-- hash, not lower-folded.
CREATE INDEX CONCURRENTLY IF NOT EXISTS friendships_address_requested
    ON friendships (address_requested);
