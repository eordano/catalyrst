-- Marketplace v3 (off-chain signed trade) charge-basis pinning.
--
-- `trade_id` records the marketplace.trades uuid of the specific signed trade
-- a mode='trade' line was priced from. The outbox worker re-fetches the FULL
-- trade JSON (signature + checks + assets + mv status) from catalyrst-market
-- GET /v1/trades/{id} at fulfilment time, fail-closed pre-flights that it is
-- still status='open' in the unified mv, and forwards the payload to the
-- economy broker's mode "trade" (the economy service has no market-DB access
-- by design). NULL for primary/secondary lines.
ALTER TABLE fulfillment_outbox ADD COLUMN IF NOT EXISTS trade_id TEXT;
