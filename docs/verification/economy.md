# Verification — catalyrst-economy (service "economy")

Upstream: `decentraland/transactions-server` (transactions-api.decentraland.org).
Crate: `crates/catalyrst-economy`, branch `feat/service-plane-crates` (committed tree).
Bundle: `data` (5146).

Verdict: the re-check findings are **substantially correct**. Every flagged claim was
re-derived from the committed Rust, the upstream TS, and the Unity C# consumer. No
client null-crash risk exists. The single material behavioural gap is the documented
no-relayer 503, which is real and client-visible. A few small message/shape
divergences are cosmetic (client uses Newtonsoft + null checks, never parses message
bodies).

## Endpoints

The Unity client (`unity-net-catalog`) touches **only** `POST /v1/transactions`
(`DecentralandUrl.MetaTransactionServer`, called from
`ThirdWebMetaTxService.SendMetaTransactionAsync` via `ThirdWebEthereumApi`).
`GET /v1/transactions/:userAddress` and `GET /v1/contracts/:address` are **not called
by the Unity client** (catalog has zero hits for either path; the `/contracts/*` hits
in the catalog are `market.decentraland`/`lambdas/contracts/servers`, unrelated).

| Endpoint | Shape | Client reaction | Severity | Failure-modes OK | Notes |
|---|---|---|---|---|---|
| POST `/v1/transactions` | OK on success: `{ok:true, txHash}` matches upstream `{ok:true, txHash}` and C# `TransactionsServerResponse.txHash`. Error envelope `{ok:false, message, code}` matches upstream. | C# Newtonsoft deserialize, nullable result, explicit null/empty-txHash guard. Non-2xx -> `throw Web3Exception` (intentional, caught upstream as exception). NO null-deref. | none (shape) / medium (no-relayer 503) | yes | Only client-exercised route. With no relayer provisioned, every broadcast attempt returns 503 -> client throws `Web3Exception`. |
| GET `/v1/transactions/:userAddress` | Returns JSON array of `{id, tx_hash, user_address, created_at}`. Upstream returns `SELECT *` rows (same columns; `created_at` upstream is a JS `Date` serialized as ISO, ours hand-formats `%Y-%m-%dT%H:%M:%S%.3fZ`). | Not called by Unity client. | none | yes | Lowercased lookup, matches upstream. |
| GET `/v1/contracts/:address` | `{ok:true}` on valid, else 404 `{ok:false, message:"Address is not valid", code:"unknown"}`. Matches upstream exactly. | Not called by Unity client. | none | yes | collection SQL OR whitelist; same as upstream. |
| GET `/ping` | `"pong"` (text) | n/a (liveness) | none | yes | Not upstream-defined; harness only. |
| GET `/health` | `{status, database, relayer}` + 503 when DB down | n/a (ops) | none | yes | Not upstream-defined. |

## Confirmed issues

1. **No-relayer broadcast 503 (RelayerUnavailable) — confirmed, intentional, client-visible.**
   `transaction.rs:202-213` `send_meta_transaction` returns `ApiError::RelayerUnavailable`
   when `relayer` is `None`; `errors.rs:57` maps it to `503 {code:"unknown"}`.
   `Relayer::from_config` returns `None` unless the OZ trio is set (`relayer.rs:46-49`,
   gated by `config.rs:72-75 has_relayer`), and a startup `warn` is logged (`lib.rs:83`).
   Upstream always has a relayer wired, so it has no such path; with a relayer it would
   return `{ok:true, txHash}`. The C# consumer (`ThirdWebMetaTxService.cs:271-272`)
   does `if (!response.IsSuccessStatusCode) throw new Web3Exception(...)`, so the client
   throws on the 503. This is a genuine functional degradation (meta-tx broadcast
   unavailable) for the ThirdWeb in-client wallet path, but it surfaces as a clean caught
   `Web3Exception`, not a crash. Severity: medium (validation runs, broadcast does not).

2. **Quota check `created_at >= NOW()` — confirmed faithful copy of the upstream bug.**
   Ours: `transaction.rs:114-115`. Upstream: `checkQuota.ts:17-20` uses the identical
   predicate. The window is effectively empty (nothing is `>= NOW()` after insert), so
   per-day quota never fires on either side. Faithful, not a regression. Severity: none
   (parity with upstream's documented off-by-design behaviour).

## Startup / hard-DB-dependency claim — confirmed

`build_state` (`lib.rs:48-93`) does `PgPoolOptions::connect_with(...).await?`
(lines 55-60, fails -> `Err` -> `main` exits non-zero) AND runs
`sqlx::migrate!("./migrations").run(&pool).await` (lines 67-70, fails -> `Err`).
There is no lazy/degraded DB mode; the writable `marketplace` Postgres is hard-required
at boot. `set_search_path` failure is swallowed (`lib.rs:65 .ok()`) — matches the claim.
Missing `DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING` -> `config.rs:37 required()` ->
`anyhow` error from `Config::from_env()?` in `main.rs:21` -> exits non-zero. All other
knobs degrade gracefully and are panic-free, as claimed:
- `RPC_URL` absent -> `has_rpc()` false (`config.rs:77-79`) -> gas-price/simulate skipped
  in `check_data` (`transaction.rs:88-91`). No warning emitted (matches claim).
- `MAX_GAS_PRICE_ALLOWED_IN_WEI` absent -> ceiling check returns `Ok` early
  (`transaction.rs:155-157`).
- OZ trio absent -> `Relayer::from_config` -> `None`, startup `warn` (`lib.rs:80-84`).
- `CONTRACT_ADDRESSES_URL` unreachable at request time -> serves stale whitelist cache
  if present, else `ApiError::Internal` (500) (`contracts.rs:77-94`, `97-110`).
- `COLLECTIONS_CHAIN_ID` with no `DclContracts` mapping -> sale-price check skipped
  (`transaction.rs:232-234`).
- No LiveKit involvement (confirmed: no references in crate).
- Present-but-unparseable numeric/port envs abort startup with a context error
  (`config.rs:93-119`), acceptable fail-fast.

The two `.expect("has_rpc gated")` (`transaction.rs:130`, `158`) are provably
unreachable: both `check_gas_price`/`check_transaction` are only called inside the
`if cfg.has_rpc()` block (`transaction.rs:88-91`), which guarantees `rpc_url` is `Some`
and non-empty. No request-reachable unwrap/expect.

## Error model — confirmed coherent and upstream-faithful

`ApiError -> IntoResponse` (`errors.rs:67-82`) emits uniform `{ok:false, message, code}`
+ HTTP status. Status/code mapping (`errors.rs:48-64`):
- InvalidSchema/InvalidSalePrice/InvalidContractAddress/InvalidTransaction/RelayReverted
  -> 400; QuotaReached -> 429; HighCongestion -> 503; RelayerTimeout -> 504;
  NotFound -> 404; Database/RelayerFailed/Internal -> 500.
This matches upstream `transaction-middleware.ts` (400/429/503 paths) and
`handlers.ts` (404 for not-found, 504 for RelayerTimeout, 500 fallback). The `code`
strings match the upstream `decentraland-transactions` `ErrorCode` enum exactly
(invalid_schema, sale_price_too_low, invalid_contract_address, invalid_transaction,
quota_reached, high_congestion, unknown) — cross-checked against
`types/transactions/errors.ts`. DB errors are logged server-side and scrubbed to
"Unknown error" client-side (`errors.rs:71-74`), matching upstream's
`isErrorWithMessage` fallback (`handlers.ts:86`).

## Client-crash risks

None. The only client-exercised endpoint is POST `/v1/transactions`, and its C#
consumer (`ThirdWebMetaTxService.cs:269-280`) uses Newtonsoft `DeserializeObject` into a
nullable `TransactionsServerResponse?`, guards `result == null || IsNullOrEmpty(txHash)`,
and throws `Web3Exception` on any non-2xx. No non-null assertion, no unconditional field
access. The error envelope's extra fields (`ok`, `message`, `code`) are ignored by
Newtonsoft. The two unused GET endpoints have no C# consumer.

## Failure-mode gaps (minor, non-crashing)

- **Missing-transactionData 400 has a `code` where upstream omits it / different body.**
  Upstream `transaction-middleware.ts:33-41` returns `{ok:false, message}` with NO `code`
  when `transactionData` is absent. In our crate this case cannot arise as a distinct
  branch: serde rejects a missing/malformed `transactionData` with axum's default body
  (422/400, not the `{ok:false}` envelope). Divergent body shape on malformed input, but
  the C# client only ever sends a well-formed `transactionData` and reacts to any non-2xx
  by throwing — so unobservable. Cosmetic.
- **Oversized body -> 413 with axum's default body**, not the `{ok:false}` envelope.
  Client throws on non-2xx regardless. Cosmetic.
- **InvalidSchema message text differs** (ours: "missing `from`" / "must contain exactly
  2 strings"; upstream: "Invalid transaction data. Errors: {ajv json}"). Same `code`
  (`invalid_schema`); client never parses the message. Cosmetic.
- **Schema leniency**: upstream AJV enforces `additionalProperties:false` and string-typed
  `params` items; ours relies on serde (rejects type mismatch, silently ignores extra
  fields). No functional impact on the client's fixed payload shape.
- **No 500-vs-degrade gap** found beyond the intended hard-DB-at-boot requirement: at
  request time, whitelist-fetch failure degrades to stale cache (good) or 500 (matches
  upstream's thrown-error -> 500); RPC failures map to InvalidTransaction(400)/Internal(500)
  consistent with upstream.

## Rejected / downgraded

- "Status mapping divergence" — none found; mapping is faithful to upstream.
- Any GET-endpoint client-crash — rejected: neither GET is called by the Unity client.
- "DB optional" assumption — rejected: economy hard-requires its `marketplace` Postgres
  at boot (confirmed), unlike a typical optional-DB service.
