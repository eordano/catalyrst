# Parity: catalyrst-economy (service "economy")

Upstream reference: `decentraland/transactions-server` (meta-transaction relay).
Our crate: `crates/catalyrst-economy`.
Client lib that consumes the API: `decentraland/decentraland-transactions`
(`src/sendMetaTransaction.ts`).

Live diff: not applicable (upstream transactions-server is not running here;
compared statically against source).

## What the explorer actually calls

The Unity net-catalog (`the Unity net-catalog`) shows the client
hits exactly one route on this service:

- `POST https://transactions-api.decentraland.{ENV}/v1/transactions` — body `TransactionData`.

It does **not** call `GET /v1/transactions/{user_address}` and does **not** call
`GET /v1/contracts/{address}`. The client lib (`sendMetaTransaction.ts:110-122`)
reads only `body.ok`, `body.message`, `body.code`, and `body.txHash` from the
POST response — nothing else.

## Per-endpoint table

| Endpoint | Shape | Efficiency | Severity | Notes |
|---|---|---|---|---|
| `GET /ping` | unknown | same | none | No upstream literal to diff; static `pong`. No client/spec pins it. |
| `POST /{api_version}/transactions` | match | better | none | Success + error envelopes + status codes + error codes all match exactly. Relayer-503 is a deployment gap, not a wire-shape divergence. |
| `GET /{api_version}/transactions/{user_address}` | divergent | same | minor | Real `created_at` format divergence (no `Z`), but no client reads this endpoint — explorer impact is nil. |
| `GET /{api_version}/contracts/{address}` | match | better | none | Bodies + status (200/404) match. Not exercised by the client. |

## Confirmed shape findings

### POST /v1/transactions — match (verified)
- Success body: ours `json!({ok:true, txHash})` (`handlers/transactions.rs:35`)
  == upstream `{ ok: true, txHash }` (`handlers.ts:56-58`).
- Error body: ours `{ok:false, message, code}` (`http/errors.rs:94`) == upstream
  `{ok:false, message, code}` (`handlers.ts`, `logic/transaction-middleware.ts`).
- HTTP status mapping verified against upstream `transaction-middleware.ts`
  (where checkData runs) + `HTTPResponse.ts` enum:
  - InvalidSchema 400, InvalidSalePrice 400, InvalidContractAddress 400,
    InvalidTransaction/SimulateTransaction 400, QuotaReached 429,
    HighCongestion 503 — all match `errors.rs::parts()`.
- Error `code` strings ported 1:1 from `decentraland-transactions` `ErrorCode`
  enum (`errors.ts:1-17`): `unknown`, `invalid_transaction`, `invalid_schema`,
  `invalid_contract_address`, `sale_price_too_low`, `quota_reached`,
  `high_congestion` — all present and identical in `errors.rs::code`.
- Request struct matches: `transactionData:{from, params:[string,string]}`
  (serde rename `transactionData`, `ports/transaction.rs:23-34`) == upstream
  `SendTransactionRequest` / `transactionSchema` (exactly 2 string params).

### GET /v1/transactions/{user_address} — divergent (verified real; no client impact)
- Both return a BARE array, no `{data:[...]}` wrapper: ours
  `Json<Vec<TransactionRow>>` (`handlers/transactions.rs:48`) == upstream
  `body: transactions` rows array (`handlers.ts:25-28`). Match.
- Field names snake_case on BOTH sides (`id, tx_hash, user_address, created_at`);
  Rust fields are not renamed, upstream `TransactionRow` is snake_case. No
  camelCase mismatch.
- **Real divergence — `created_at` serialization format.** Upstream column is
  `TIMESTAMP` (without time zone) (migration `1654574382488`). node-postgres
  parses it into a JS `Date` and `JSON.stringify` emits ISO-8601 with `Z` and
  millis, e.g. `2026-06-09T12:00:00.000Z`. Ours is chrono `NaiveDateTime`
  (`ports/transaction.rs:42`), whose serde default emits
  `%Y-%m-%dT%H:%M:%S%.f` with **no** `Z`/offset and millis only when nonzero,
  e.g. `2026-06-09T12:00:00`. A strict ISO-Date parser would treat ours as local
  time. The divergence is genuine at the wire level.
- **Severity downgraded to minor:** the Unity client never calls this endpoint
  (net-catalog) and `decentraland-transactions` has no consumer of it, so the
  format gap reaches no live reader. Listed for completeness, not as a blocker.
- `id` (i32 vs number) and string fields match. No pagination/ordering either
  side. Upstream `SELECT *` vs our explicit 4-column list = same columns.

### GET /v1/contracts/{address} — match (verified)
- 200 valid: ours `json!({ok:true})` (`handlers/contracts.rs:17`) == upstream
  `{ ok: true }` (`handlers.ts:111`).
- 404 invalid: ours `{ok:false, message:"Address is not valid", code:"unknown"}`
  (NotFound -> `errors.rs:75,94`) == upstream
  `{ok:false, message:'Address is not valid', code:ErrorCode.UNKNOWN}`
  (`handlers.ts:115-120`). `ErrorCode.UNKNOWN == "unknown"`.
- Status codes 200/404 match. No wrapper/casing/type differences.

## Confirmed efficiency wins (structural reason verified)

### POST /v1/transactions — better
- The contractAddress validation step is the structural win. Upstream
  `isCollectionAddress` (`ports/contracts/component.ts:67-82`) issues a
  `collectionsSubgraph.query(...)` GraphQL HTTP round-trip on cache miss. Ours
  (`ports/contracts.rs:66-77`) is a single indexed SQL
  `SELECT 1 FROM <squid>.collection WHERE id = $1 LIMIT 1` against the local
  squid table. Remote GraphQL replaced by local indexed lookup — a real
  structural change, not a language artifact.
- Whitelist is an in-process TTL cache over `addresses.json` on BOTH sides
  (ours: `parking_lot::Mutex` cache, `ports/contracts.rs:80-110`; upstream:
  module-level array + `lastFetch`, `component.ts:40-65`). No Redis either side.
  Equivalent.
- Quota is 1 SQL `COUNT(*)` on both. Equivalent.
- Same query count overall; one remote GraphQL replaced by local SQL.

### GET /v1/contracts/{address} — better
- Same structural win as above: `isValidAddress = isCollectionAddress OR
  isWhitelisted` on both. Ours runs the indexed SQL collection check; upstream
  runs the remote GraphQL collection check. Whitelist caching is equivalent.
- Minor non-structural difference: ours is sequential short-circuit
  (`is_collection_address` then `is_whitelisted`, `ports/contracts.rs:57-63`);
  upstream uses `Promise.all` (`component.ts:32-38`). The local-SQL vs
  remote-GraphQL latency gap dominates either way; this does not change the
  verdict.

## Efficiency: same (verified)

### GET /v1/transactions/{user_address}
Single SQL on both sides over the same predicate (`WHERE user_address = $1`);
ours lists explicit columns, upstream uses `SELECT *` — same 4 columns. No
cache/pagination/streaming either side; both buffer the full array. Identical.

## Rejected / qualified during verification

- **Rejected over-statement: "upstream does a GraphQL round-trip per request"
  (unqualified).** Upstream's `isCollectionAddress` keeps a growing in-process
  positive cache (`collectionAddresses` array, `component.ts:67-69`): a repeat
  lookup of an already-seen collection returns without any network call. The
  remote GraphQL hit happens on the FIRST (uncached) lookup of each address.
  Ours hits SQL every time (no positive cache), but on a PK-indexed
  `id = $1 LIMIT 1` lookup that cost is negligible. The "better" verdict still
  holds (cold path: remote HTTP vs local SQL), but the rationale is corrected:
  it is not "GraphQL every request," it is "GraphQL on cold lookups."

- **Downgraded, not rejected: `created_at` format divergence severity.** The
  diff is real, but the original "minor" rests on a non-obvious fact — the
  endpoint is dead to the explorer. Confirmed via net-catalog (no client call)
  and via `sendMetaTransaction.ts` (client lib reads no list endpoint). Kept as
  minor, explicitly noted as zero live-client impact rather than a parser hazard
  in practice.

- **Noted edge not in the original findings: missing-`transactionData` body.**
  Upstream's middleware returns 400 with a body that has NO `code` field
  (`transaction-middleware.ts:31-41`). Ours would reject a missing
  `transactionData` at axum's JSON deserialization layer with a different 400
  body. The client always sends `transactionData` (`sendMetaTransaction.ts:100`)
  and never structurally reads this error path, so impact is nil — recorded for
  completeness, not a parity blocker.

- **Confirmed (not rejected): relayer-503 behavioral gap.** With no relayer
  provisioned, our `send_meta_transaction` returns 503 `RelayerUnavailable`
  (`ports/transaction.rs:219-235`) instead of 200 `{ok:true, txHash}`. This is a
  deployment/provisioning gap, not a wire-shape divergence — both the success
  and error envelopes still match upstream. The 503 body carries `code:"unknown"`
  (`errors.rs:73`), a valid upstream error code.
