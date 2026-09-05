-- catalyrst-price: the dynamic price-override config store.
--
-- The price bundle normally serves a read-only projection of the `mana_price`
-- archive DB (latest CoinGecko snapshot). This table is the operator escape
-- hatch from the admin console (docs/admin-console.md §4, "Price override"):
-- a small KV of manual overrides, keyed by the lowercased CoinGecko token id
-- (e.g. `decentraland`) and the lowercased vs-currency (e.g. `usd`).
--
-- It is intentionally additive: the public price route is unchanged when no
-- override row exists. Set/clear is bearer-gated (CATALYRST_PRICE_ADMIN_TOKEN);
-- the store itself is read-only to unauthenticated callers.
--
-- Lives in the same pool as the price reads. The archive ingester only ever
-- writes `price_snapshots`, so this table never collides with it.

CREATE TABLE IF NOT EXISTS price_overrides (
    token_id     TEXT             NOT NULL,
    vs_currency  TEXT             NOT NULL,
    value        DOUBLE PRECISION NOT NULL,
    note         TEXT,
    updated_by   TEXT,
    updated_at   TIMESTAMPTZ      NOT NULL DEFAULT NOW(),
    PRIMARY KEY (token_id, vs_currency)
);
