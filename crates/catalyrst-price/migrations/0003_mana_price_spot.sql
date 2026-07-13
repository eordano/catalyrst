-- catalyrst-price: the MANA spot-price tables (poll mode).
--
-- Historically these tables were created out-of-band by the personal umbrella
-- schema (scripts/mana-price-archive-schema.sql) and this crate only ever read
-- from `price_snapshots`. Now that catalyrst-price can run its own poller
-- (PRICE_POLL_ENABLED=true, see src/poller.rs), it owns the spot tables too.
--
-- These definitions are a faithful copy of mana-price-archive-schema.sql and
-- match exactly the columns SELECTed by src/ports/prices.rs:
--   mana_usd, mana_eth, mana_btc, mana_market_cap_usd, mana_volume_24h_usd,
--   mana_price_change_24h_pct, source_updated_at, taken_at (filtered on source).
--
-- IF NOT EXISTS everywhere so this is a no-op when the umbrella schema (or a
-- prior run) already created them — the serve path is unchanged.

CREATE TABLE IF NOT EXISTS price_snapshots (
    id                          BIGSERIAL PRIMARY KEY,
    taken_at                    TIMESTAMPTZ NOT NULL DEFAULT now(),
    source                      TEXT NOT NULL DEFAULT 'coingecko',
    source_updated_at           TIMESTAMPTZ,
    mana_usd                    NUMERIC(20, 8),
    mana_eth                    NUMERIC(28, 18),
    mana_btc                    NUMERIC(28, 18),
    mana_matic                  NUMERIC(28, 18),
    matic_usd                   NUMERIC(20, 8),
    mana_market_cap_usd         NUMERIC(20, 2),
    mana_volume_24h_usd         NUMERIC(20, 2),
    mana_price_change_24h_pct   NUMERIC(10, 4)
);
CREATE INDEX IF NOT EXISTS idx_price_snapshots_taken_at
    ON price_snapshots (taken_at DESC);

CREATE TABLE IF NOT EXISTS daily_stats (
    date            DATE PRIMARY KEY,
    samples         INTEGER NOT NULL DEFAULT 0,
    open_usd        NUMERIC(20, 8),
    high_usd        NUMERIC(20, 8),
    low_usd         NUMERIC(20, 8),
    close_usd       NUMERIC(20, 8),
    avg_usd         NUMERIC(20, 8),
    avg_eth         NUMERIC(28, 18),
    avg_btc         NUMERIC(28, 18),
    avg_matic       NUMERIC(28, 18)
);
