# catalyrst-economy routes

Rust port of `decentraland/transactions-server` (transactions-api.decentraland.org).
Listens on the deployment's assigned port (`5155`; see
`umbrella/env/catalyrst-economy.env`). All routes unauthenticated at
the HTTP layer — authn/authz lives in the EIP-712 calldata, verified on-chain.

| Method | Path | Status | Notes |
|---|---|---|---|
| POST | `/v1/transactions` | implemented (validate + persist; relay 503) | checkData pipeline + persistence ship; broadcast returns 503 until a relayer is provisioned |
| GET  | `/v1/transactions/:userAddress` | implemented | lowercased lookup; not exercised by the Unity client |
| GET  | `/v1/contracts/:address` | implemented | collection (SQL) OR whitelist; not exercised by the Unity client |
| GET  | `/ping` | implemented | liveness |

## POST /v1/transactions pipeline

`checkData` runs in upstream order:
`schema -> gasPrice -> simulate -> salePrice -> contractAddress -> quota`.

- **gasPrice / simulate** are gated behind `RPC_URL` being set (raw JSON-RPC
  `eth_gasPrice` / `eth_estimateGas`). gasPrice additionally needs
  `MAX_GAS_PRICE_ALLOWED_IN_WEI` (mirrors the upstream FF gate).
- **salePrice** decodes `executeMetaTransaction` -> inner `buy` / `executeOrder`
  / `placeBid` via alloy `sol!`, compares against `MIN_SALE_VALUE_IN_WEI`.
  Embedded DCL contract addresses are Polygon mainnet (137).
- **contractAddress** = collection membership (`squid_marketplace.collection`
  SQL lookup) OR whitelist (`addresses.json`, TTL-cached in process).
- **quota** = `COUNT(*) WHERE user_address = lower(from) AND created_at >= NOW()`
  (faithful port of the upstream off-by-design window) vs `MAX_TRANSACTIONS_PER_DAY`.

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

> _Re-verified against code 2026-07-03 (docs-stale-audit); corrections applied._
