# Verification: catalyrst-price (service "price")

Branch verified: `feat/service-plane-crates` (current committed tree).
Crate: `crates/catalyrst-price`
Upstream: CoinGecko `GET /api/v3/simple/price` (third-party public API; no
Decentraland-owned TS service).
Unity consumer: `DonationsService.GetCurrentManaConversionAsync`
(`decentraland/unity-explorer/Explorer/Assets/DCL/Donations/DonationsService.cs:250-265`).
Caller of the consumer: `DonationsPanelController.LoadDataAsync`
(`decentraland/unity-explorer/Explorer/Assets/DCL/Donations/UI/DonationsPanelController.cs:143-205`).

Nothing run; static analysis + net-catalog (`the Unity net-catalog`).

## Per-endpoint table

| endpoint | shape | client-reaction | severity | failure-modes-ok | notes |
|---|---|---|---|---|---|
| `GET /api/v3/simple/price` | match (shape-only; omit-vs-null divergence under empty/NULL data, confirmed real) | request-throws-but-CAUGHT (NOT a crash) | minor (downgraded from "major") | happy path OK; error-axis divergences confirmed (no 400, 500-empty-body, populated-vs-empty) | URL hardcoded to `api.coingecko.com`, `env_aware=0` — Unity client NEVER calls catalyrst-price. Drop-in mock surface only. |
| `GET /health` | n/a (200/503, bodyless) | not called by client | n/a | by `SELECT 1` | standalone-only; absent in `data` bundle (bundle serves aggregate /health). |

## Shape verification (committed tree)

CONFIRMED real in `handlers/simple_price.rs`:

- Top-level object keyed by id; loop only emits `"decentraland"`, all other ids are
  `continue`-skipped (`:69-72`). Empty/absent `ids` -> `{}`.
- Inner object keyed by currency; values are bare JSON numbers via `Number::from_f64`
  (`:126-135`), snake_case, no envelope. Matches CoinGecko exactly on the happy path:
  `{"decentraland":{"usd":<number>}}`.
- Optional flags named per CoinGecko convention (`<cur>_market_cap`, `<cur>_24h_vol`,
  `<cur>_24h_change`, `last_updated_at` unix epoch int), all usd-only
  (`usd_only` returns None for non-usd, `:119-124`; `last_updated_at` at `:95-100`).
- `precision` clamped to <=18 (`:46`).
- DIVERGENCE (confirmed): fields are OMITTED, not present-with-null, when the source
  value is NULL. `mana_usd` NULL -> `usd` key absent; snapshot `None` ->
  `{"decentraland":{}}`; `eth`/`btc` only emitted when `mana_eth`/`mana_btc` columns are
  non-NULL (`spot`, `:109-116`). CoinGecko, given a valid id + `vs_currencies`, always
  returns the id key populated with the requested currency.

shape_verdict "match" is ACCURATE for the happy path; the absence-vs-presence divergence
under empty/NULL data is REAL.

## Confirmed issues

1. `GET /api/v3/simple/price` — error-axis incoherence vs upstream (MINOR).
   - No 400/422 path. `parse_query` is total/infallible (`:23-55`): unknown keys ignored,
     bad `precision` -> None (full precision), missing `ids` -> empty result, missing
     `vs_currencies` -> defaults to `["usd"]`. CoinGecko 400s on missing `vs_currencies`
     and some invalid params. CONFIRMED.
   - Successful-but-empty (no DB row / NULL columns / `ids` without `decentraland`)
     returns `200 {}` or `200 {"decentraland":{}}` instead of upstream's populated body.
     CONFIRMED.
   - DB query failure in `latest_coingecko()` -> `500 INTERNAL_SERVER_ERROR` with EMPTY
     body, no JSON error envelope; error only logged via `tracing` (`:63-66`). CoinGecko
     returns 5xx/429 with a JSON/text body + `Retry-After`. CONFIRMED.
   - Severity MINOR (not major): these only matter to a real consumer, and none exists
     (see crash-risk section).

## Client-crash risks

NONE. The finding's `client_reaction: "null-crash"` / `severity: "major"` is OVERSTATED
and REJECTED, on two independent grounds:

- Wrong exception class, AND it is caught. The consumer is
  `lastManaRate = response["decentraland"]["usd"];` (`DonationsService.cs:261`) where
  `response` is `Dictionary<string, Dictionary<string, decimal>>` (Newtonsoft). On the
  divergent cases (`{}`, `{"decentraland":{}}`, missing `usd`) the C# Dictionary indexer
  throws `KeyNotFoundException` — not a null-deref. The only caller,
  `DonationsPanelController.LoadDataAsync` (`:143-205`), wraps the
  `UniTask.WhenAll(... GetCurrentManaConversionAsync(ct) ...)` at line 170 in
  `try { } catch (OperationCanceledException) {...} catch (Exception e) { ReportHub.LogException(e); CloseController(); } finally { viewInstance!.SetDefaultLoadingState(false); }`.
  Any throw (KeyNotFound, JSON parse error from a 500/empty body, HTTP error from
  `CreateFromJson`) is caught: logs + closes the donations panel. The process does NOT
  crash. Graceful degradation, not a null-crash.

- The endpoint is never reached. net-catalog (`endpoints` + `url_constants`) shows
  `DecentralandUrl.ManaUsdRateApiUrl` =
  `https://api.coingecko.com/api/v3/simple/price?ids=decentraland&vs_currencies=usd` with
  `env_aware = 0` (`DecentralandUrlsSource.cs:230`; consumed `DonationsService.cs:75`).
  The URL is hardcoded to CoinGecko and is not routed through any catalyst/catalyrst
  override, so the Unity client never calls catalyrst-price. catalyrst-price is a
  drop-in mock surface only.

## Failure-mode gaps

Confirmed against actual error paths:

- `500 INTERNAL_SERVER_ERROR` with EMPTY body on DB failure (no JSON error envelope) vs
  CoinGecko's JSON/text 5xx + 429/`Retry-After` (`simple_price.rs:63-66`). Divergent but
  harmless (no client consumes it).
- No 400 for missing/invalid required params (`vs_currencies`, bad `precision`, unknown
  id) — all yield `200` with possibly-empty JSON (`parse_query` infallible, `:23-55`).
- `200 {"decentraland":{}}` / `200 {}` under empty-snapshot / NULL-column /
  non-`decentraland`-id cases where CoinGecko returns a populated body. Would throw
  `KeyNotFoundException` in the C# consumer, but that throw is caught (above).
- Standalone startup is NOT degrade-tolerant: `Config::from_env()` hard-requires
  `PRICE_PG_COMPONENT_PSQL_CONNECTION_STRING` (`config.rs:15`), and `build_state` eagerly
  `connect_with().await`s the pool (`lib.rs:30-46`); DB down at boot -> clean `anyhow`
  error -> non-zero exit (no panic). In the `data` bundle (5146) the member is
  mount()-wrapped, so a missing price DB leaves `/api/v3/simple/price` unregistered (404
  from bundle) and `/health` reports `degraded` / `members.price:down`. Bundle startup is
  panic-free. CONFIRMED.
- `/health` (standalone only) is `200`/`503` by `SELECT 1`, bodyless
  (`handlers/health.rs:6-17`); not registered in `api_router()`, so shadowed by the
  bundle's aggregate `/health` under `data`. CONFIRMED.

## Verdict on the finding

- shape_verdict "match" + omit-vs-null divergence: ACCURATE.
- error-axis divergences (no 400, 500-empty-body, populated-vs-empty): ACCURATE.
- `client_reaction: "null-crash"` + `severity: "major"`: OVERSTATED / REJECTED. The
  consuming exception is a caught `KeyNotFoundException` (graceful panel-close), and the
  endpoint is never called by the Unity client (URL hardcoded to CoinGecko, non-env-aware).
  Effective severity: MINOR.
