# catalyrst-economy routes

Rust port of `decentraland/transactions-server` (transactions-api.decentraland.org). Listens on the
deployment's assigned port (`5155`; see `umbrella/env/catalyrst-economy.env`). All routes are
unauthenticated at the HTTP layer - authn/authz lives in the EIP-712 calldata, verified on-chain.

| Method | Path | Status | Notes |
|---|---|---|---|
| POST | `/v1/transactions` | implemented (validate + persist; relay 503) | checkData pipeline + persistence ship; broadcast returns 503 until a relayer is provisioned |
| GET  | `/v1/transactions/:userAddress` | implemented | lowercased lookup; not exercised by the Unity client |
| GET  | `/v1/contracts/:address` | implemented | collection (SQL) OR whitelist; not exercised by the Unity client |
| GET  | `/ping` | implemented | liveness |

## POST /v1/transactions pipeline

`checkData` runs in upstream order: `schema -> gasPrice -> simulate -> salePrice -> contractAddress -> quota`.

- **gasPrice / simulate** gated behind `RPC_URL` being set (raw JSON-RPC `eth_gasPrice` / `eth_estimateGas`); gasPrice additionally needs `MAX_GAS_PRICE_ALLOWED_IN_WEI` (mirrors the upstream FF gate).
- **salePrice** decodes `executeMetaTransaction` -> inner `buy` / `executeOrder` / `placeBid` via alloy `sol!`, compares against `MIN_SALE_VALUE_IN_WEI`. Embedded DCL contract addresses are Polygon mainnet (137).
- **contractAddress** = collection membership (`squid_marketplace.collection` SQL lookup) OR whitelist (`addresses.json`, TTL-cached in process).
- **quota** = `COUNT(*) WHERE user_address = lower(from) AND created_at >= NOW()` (faithful port of the upstream off-by-design window) vs `MAX_TRANSACTIONS_PER_DAY`.

## USD-pegged (assetType 2) trades on POST /v1/broker/buy

Marketplace v3 trades priced as `USD_PEGGED_MANA` (assetType 2) carry a **USD amount**
(18 decimals) that the on-chain `DecentralandMarketplacePolygon` converts to MANA at its
Chainlink MANA/USD aggregator when the accept mines (`value * 1e18 / rate`, floor). The
broker executes these with three guards:

- **Pinned USD amount** — for assetType 2, the body's `priceWei` pins the trade's signed
  USD-wei amount (for assetType 1 it pins the exact MANA amount, unchanged).
- **Staleness bound** — the broker reads the same aggregator (`latestRoundData`) over the
  relayer RPC just before broadcast and refuses if the round is older than
  `USD_PEGGED_ORACLE_MAX_AGE_SECS` (409).
- **Slippage bound** — the optional body field `quoteManaWei` carries the listing-time MANA
  quote; when present, the broker refuses (409) if the execution-time conversion drifted
  more than `USD_PEGGED_SLIPPAGE_BPS` from it. When absent, no slippage bound applies.

**Charge-basis policy:** the ledger is charged the USD amount converted at the
**execution-time** rate — `broker_purchases.price_wei` records that MANA figure (with
`usd_amount_wei` + `mana_usd_rate_wei` kept for audit), and the response reports it as
`chargeBasisWei` plus a `usdPegged` block (`usdAmountWei`, `manaUsdRateWei`,
`rateUpdatedAt`). The signed trade goes on-chain untouched (assetType 2, USD value); the
MANA the contract actually moves can differ from the recorded basis by the rate drift
between the broker's read and the mined block, bounded by the guards above plus the
contract's own 27s aggregator tolerance.

Note: an idempotent replay of a USD-pegged buy re-reads the oracle and re-applies both
bounds before resuming; if the rate has since moved beyond them the replay is refused
(409) — retry with a re-quoted `quoteManaWei` to resume (the background reconciler keeps
advancing the on-chain receipt states meanwhile; funds safety does not depend on the
replay).

## Error status mapping

| Status | Body `code` | Cause |
|---|---|---|
| 400 | `invalid_schema` | params not exactly 2 strings / missing from |
| 400 | `sale_price_too_low` | sale price < min |
| 400 | `invalid_contract_address` | not collection and not whitelisted |
| 400 | `invalid_transaction` | eth_estimateGas simulate failure |
| 429 | `quota_reached` | per-window quota hit |
| 503 | `high_congestion` | gas price > max allowed |
| 503 | `unknown` | relayer not provisioned |
| 504 | `unknown` | relayer timeout |
| 500 | `unknown` | db / internal |
