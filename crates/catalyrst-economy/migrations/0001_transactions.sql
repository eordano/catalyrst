-- catalyrst-economy: the meta-transaction relay log.
--
-- Faithfully ports the upstream transactions-server table
-- (src/migrations/1654574382488_create-transactions.ts). It lives in the
-- WRITABLE `marketplace` schema (owned by mpa_*), alongside the other
-- catalyrst-market local tables. The read-only `squid_marketplace` schema
-- (owned by the indexer) is queried separately for collection membership.
--
-- One row per successfully relayed meta-transaction. `tx_hash` is the
-- on-chain hash returned by the relayer; `user_address` is the lowercased
-- `from` of the meta-transaction. The per-day quota check counts rows here.

CREATE TABLE IF NOT EXISTS transactions (
    id           SERIAL      PRIMARY KEY,
    tx_hash      TEXT        UNIQUE NOT NULL,
    user_address TEXT        NOT NULL,
    created_at   TIMESTAMP   NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS transactions_user_address_idx
    ON transactions (user_address);
