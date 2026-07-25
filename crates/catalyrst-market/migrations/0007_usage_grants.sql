-- catalyrst-market usage_grants — the escrow "lease" overlay table (Phase 5 of
-- the Landiler marketplace).
--
-- During the 15-day escrow/return window a purchased NFT is held on-chain by the
-- Landiler escrow contract (owner = escrow), NOT by the buyer. So none of the six
-- ownership-resolution sites (server lambdas, profile read-time filter, deploy
-- validator, market backpack cluster) would surface it to the buyer. This table
-- is the off-chain overlay: each active row asserts that `grantee_address` may
-- USE `urn`/`token_id` (render it on the avatar AND see it in the backpack,
-- flagged leased) until `unlock_at`, even though the chain says the escrow owns
-- it. The six sites UNION / OR-EXISTS this table so a leased item is
-- indistinguishable from an owned one to the buyer.
--
-- It lives in the `marketplace` schema of the SAME database as
-- `squid_marketplace.nft`, so all six queries can JOIN/UNION it directly. The
-- credits outbox worker writes a row here (via USAGE_GRANTS_PG_CONNECTION_STRING)
-- when a broker buy confirms.
--
-- Additive only: CREATE ... IF NOT EXISTS, no BEGIN/COMMIT (sqlx wraps each
-- migration in its own transaction). When the table is empty (production today)
-- every overlaid query returns byte-identical results to before (UNION ALL of an
-- empty set adds nothing; OR EXISTS over an empty table is always false).
--
-- NOTE: the `marketplace` schema already exists (the market write pool sets
-- search_path=marketplace and migrations 0001-0006 create tables in it). We do
-- NOT `CREATE SCHEMA` here: the market DB role (mpa_*) has CREATE on the existing
-- schema but NOT CREATE-on-database, and `CREATE SCHEMA IF NOT EXISTS` still
-- requires the database privilege even when the schema is present — it would
-- fail the migration ("permission denied for database") at market startup.

CREATE TABLE IF NOT EXISTS marketplace.usage_grants (
    id              BIGSERIAL PRIMARY KEY,
    grantee_address TEXT NOT NULL,          -- buyer, lowercase
    urn             TEXT NOT NULL,
    token_id        TEXT,
    category        TEXT NOT NULL,          -- 'wearable' | 'emote'
    escrow_ref      TEXT,
    granted_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    unlock_at       TIMESTAMPTZ NOT NULL,
    status          TEXT NOT NULL DEFAULT 'active'
                        CHECK (status IN ('active', 'revoked', 'released'))
);

CREATE INDEX IF NOT EXISTS idx_usage_grants_grantee
    ON marketplace.usage_grants(grantee_address) WHERE status = 'active';
CREATE INDEX IF NOT EXISTS idx_usage_grants_urn
    ON marketplace.usage_grants(urn);

-- Idempotency natural key for the credits outbox worker's grant write: a broker
-- buy confirmation may be re-delivered (crash after broker mint, before the
-- 'confirmed' commit), so the worker re-runs the grant write. `token_id` is NULL
-- for a primary mint (the on-chain token id is not known yet), so the index is
-- NULLS NOT DISTINCT to dedupe those, and partial on escrow_ref IS NOT NULL so it
-- only constrains worker-written rows (manual/admin grants stay unconstrained).
CREATE UNIQUE INDEX IF NOT EXISTS usage_grants_natural_key
    ON marketplace.usage_grants (escrow_ref, urn, token_id)
    NULLS NOT DISTINCT
    WHERE escrow_ref IS NOT NULL;
