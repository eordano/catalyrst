-- catalyrst-market usage_grants: add the `collection` column (Landiler Phase 6).
--
-- The Phase-6 credits ReleaseWorker settles an escrowed NFT to the buyer at/after
-- unlock by calling economy `release(collection, tokenId, buyer)`. That on-chain
-- call needs the NFT's COLLECTION contract address, which Phase 5's usage_grants
-- row did not store (it kept urn + token_id only). The broker buy DOES know the
-- collection (the catalog item's contract_address), so the outbox worker now
-- populates this column when it writes the grant.
--
-- Additive + idempotent: ADD COLUMN IF NOT EXISTS, no BEGIN/COMMIT (sqlx wraps
-- each migration in its own transaction). NULLable — pre-existing rows (and any
-- manual/admin grant) simply leave it NULL; the ReleaseWorker logs a warn and
-- skips a grant whose collection (or token_id) is unknown.
--
-- NOTE: we do NOT `CREATE SCHEMA` — the market role (mpa_*) has CREATE on the
-- existing `marketplace` schema but NOT CREATE-on-database (see 0007).

ALTER TABLE marketplace.usage_grants ADD COLUMN IF NOT EXISTS collection TEXT;
