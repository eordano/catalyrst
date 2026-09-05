-- catalyrst-economy: USD-pegged (assetType 2) trade execution audit fields.
--
-- CHARGE-BASIS POLICY: a USD-pegged trade is charged the MANA equivalent of its
-- signed USD amount converted at the EXECUTION-time Chainlink MANA/USD rate —
-- the same aggregator the DecentralandMarketplacePolygon contract settles the
-- transfer at (value * 1e18 / rate, floor). `price_wei` on the row therefore
-- records the execution-time MANA charge basis, NOT the signed trade amount
-- (which is USD wei for assetType 2). The two columns below preserve the
-- signed USD amount and the oracle rate used, so every conversion is
-- auditable after the fact:
--
--   usd_amount_wei    — the trade's signed USD amount (18 decimals), verbatim.
--   mana_usd_rate_wei — the MANA/USD rate (normalized to 18 decimals) read
--                       from the aggregator at broadcast decision time.
--
-- price_wei ~= usd_amount_wei * 1e18 / mana_usd_rate_wei (floor). The exact
-- MANA the contract moves may differ by the rate drift between our read and
-- the block that mines the accept; the slippage bound (USD_PEGGED_SLIPPAGE_BPS
-- vs the caller's listing-time quote) and the contract's own staleness
-- tolerance bound that drift. Both columns are NULL for MANA-priced trades
-- and non-trade modes. Pure-additive; safe to re-run.

ALTER TABLE broker_purchases ADD COLUMN IF NOT EXISTS usd_amount_wei NUMERIC;
ALTER TABLE broker_purchases ADD COLUMN IF NOT EXISTS mana_usd_rate_wei NUMERIC;
