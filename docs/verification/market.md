# Verification — catalyrst-market (service "market", port 5133)

Adversarial re-check of the market findings against the committed tree
(`feat/service-plane-crates`), upstream `decentraland/marketplace-server`, and the
Unity net catalog. Crate: `crates/catalyrst-market`.

## Verdict at a glance

- The flagged `GET /v1/bids` **shape divergence is CONFIRMED real on the committed tree** and is
  genuinely the only `{ok:true,data}`-wrapper break in the crate.
- The **"no client impact" claim is CONFIRMED** — the Unity client makes zero API calls to
  marketplace-server (every `marketplace` row in the net-catalog is an `OPEN_URL` browser link).
- The crate-level **startup + error-model descriptions are accurate**.
- **REJECTED / CORRECTED:** the failure-mode claim that an invalid param yields "our 400 / upstream 400".
  For `/v1/bids` (and several sibling list endpoints) our handler **silently drops** an out-of-range
  enum value and returns **200**, while upstream **throws -> 400**. The prior report's blanket line
  "malformed enum/value param -> silently defaulted (matches upstream)" is wrong: upstream does NOT
  silently default for value-list params, it 400s.

## Per-endpoint table

| endpoint | shape | client-reaction | severity | failure-modes-ok | notes |
|---|---|---|---|---|---|
| GET /v1/bids | DIVERGENT (confirmed) | none (no client) | minor | NO | Ours flat `{results,total,page,pages,limit}`; upstream `{ok:true,data:{...}}` (bids-handler.ts:28-40). Plus `pages`-on-empty divergence AND invalid-enum 400-vs-200 gap. |
| GET /v1/orders | match `{data,total}` | none | none | partial | orders-handler.ts:21-27 vs orders.rs:14. Upstream catch->400; ours DB->500. |
| GET /v1/sales | match `{data,total}` | none | none | partial | same 400-vs-500 on-error skew. |
| GET /v1/accounts | match `{data,total}` | none | none | partial | |
| GET /v1/owners | match `{data,total}` | none | none | partial | missing contractAddress/itemId -> 400 byte-identical msg. |
| GET /v1/contracts | match `{data,total}` | none | none | partial | upstream contracts catch->500. |
| GET /v1/collections | match `{data,total}` | none | none | partial | |
| GET /v1/items | match `{data,total}` | none | none | partial | |
| GET /v1/nfts | match `{data,total}` | none | none | partial | |
| GET /v1/catalog, /v2/catalog | match `{data,total}` | none | none | partial | upstream `asJSON` emits bare-string error body. |
| GET /v1/prices | match `{data}` | none | none | partial | |
| GET /v1/trendings | match `{data}` | none | none | partial | |
| GET /v1/volume/{tf} | match `{data}` | none | none | partial | |
| GET /v1/rankings/{e}/{tf} | match `{data}` | none | none | partial | unsupported entity -> 400 same msg. |
| GET /v1/stats/{cat}/{stat} | match | none | none | n/a | not re-audited in depth. |
| GET /v1/activity | catalyrst-only | none | none | YES | auth-chain gated; `{data,total}`. |
| GET /v1/trades, /{id}, /{sig}/accept | match | none | none | partial | trade-not-found -> 404. |
| POST/GET /v1/federation/*, /federation/market/* | catalyrst-native | none | none | YES | uniform `{ok:false,message}`/`{ok:true,signature_hash}`, panic-free (federation.rs:23-101). |

"partial" = success shape matches but our DB-outage status (500) diverges from upstream handlers that 400-on-anything; immaterial with no client.

## Confirmed issues

1. **`GET /v1/bids` structural wrapper divergence — CONFIRMED, minor.**
   - Ours: `handlers/bids.rs:16` returns `PaginatedResponse<Bid>`, which serializes FLAT
     (`catalyrst-types/src/pagination.rs:9-16`: `results,total,page,pages,limit`, no envelope).
   - Upstream: `bids-handler.ts:28-40` returns `{ok:true, data:{results,total,page,pages,limit}}`.
   - Inner field names/types match. I surveyed all 14+ list handlers: every other endpoint returns bare
     `{data,...}`/`{data}` upstream, which our crate mirrors exactly (orders/sales/accounts/owners/
     contracts/collections/items/nfts/catalog -> `{data,total}`; prices/trendings/volume/rankings -> `{data}`).
     So "only real shape break in the crate" is **accurate**. (Note: upstream user-assets and trades DO use
     `{ok:true,data}` too, and ours matches them there — bids is the lone mismatch.)

2. **`GET /v1/bids` `pages`-on-empty divergence — CONFIRMED, cosmetic.**
   - Upstream `pages: data.length>0 ? ceil(count/limit) : 0` (bids-handler.ts:36); ours always
     `ceil(total/limit)` (pagination.rs:21-25). Differs only on a page past the end where `total>0`
     (upstream 0, ours `ceil`). Real but trivial; no consumer.

3. **`GET /v1/bids` invalid-enum failure mode — NEW GAP; REJECTS the finding's `ok:true` row.**
   - `parse_filters` (ports/bids.rs:232-267) reads `sortBy`/`network` via `Params::get_value`
     (http/params.rs:42-55), which on an unrecognized value falls back to default/None and **does not
     error**. `status` is read with `get_string` — no validation at all, bound straight into SQL.
   - Upstream `getParameter(name, params, validValues)` (logic/http/pagination.ts:25-33) **throws
     `InvalidParameterError`** when the value is outside `validValues`, yielding **400
     `{ok:false,message:"The value of the sortBy parameter is invalid: <v>"}`**.
   - Net: bad `sortBy`/`network`/`status` -> **upstream 400, ours 200** (filter silently dropped).
   - A correct `get_parameter` helper that DOES throw exists in the crate (http/pagination.rs:42-58) but
     `parse_filters` does not use it. The `parse_filters -> Result<_, InvalidParameterError>` signature
     and the `?` at `bids.rs:14` are effectively dead — no path constructs an Err. So the "400 on invalid
     param" the finding credits us with does not exist for bids.

## Client-crash risks

**None.** Verified against `the Unity net-catalog`: unity-explorer issues no HTTP
request to marketplace-server. All `marketplace`-matching rows are `OPEN_URL`/`OpenURL` browser deeplinks
(`market.decentraland.{ENV}`, `/marketplace/names/claim`, `/marketplace/browse`, credits blog). The only
`/v1/` API calls in the catalog target `builder-api`, `transactions-api`, and `social-api`. No C#
DTO/converter deserializes a catalog/bids/orders response, so the wrapper divergence and the silent-filter
behavior cannot null-crash or throw. `client_reaction: unknown` and "no impact" are correct.

## Failure-mode gaps

- **Invalid enum param on `/v1/bids` (and any handler routing `sortBy`/`network`/`status` through
  `Params::get_value`/`get_string`): ours 200-with-filter-dropped vs upstream 400.** Substantive gap; the
  finding's failure-mode row (and the prior report's "matches upstream" line) overstate parity.
  Informational-only given no client.
- **DB-outage status skew (finding noted this): ours 500 `{ok:false,message:"database error"}`**
  (`catalyrst-types/src/error.rs:140-143`). Upstream is inconsistent — `asJSON` catalog handlers emit a
  bare-string body; `contracts/collections/accounts` return 500-on-any-failure; `sales/orders/items/
  owners/prices/trendings/rankings/stats/volume` return **400**-on-anything. Our 500 matches the former
  group, diverges from the latter. Confirmed by reading the catch blocks. Immaterial with no client.
- **No degradation surface.** `build_state` (lib.rs:236-245) eagerly connects three required pools
  (dapps-write/10, dapps_read/20 = the actual query pool, favorites/10 connected-but-unused at
  lib.rs:243,247), runs `sqlx::migrate!` against dapps_write (lib.rs:264-267, abort on failure), and loads
  federation `Replay` (lib.rs:269-271). Missing env or unreachable DB -> clean `Err` exit via `?`/`.context`,
  no panic — confirmed fail-fast, panic-free. "Degradation-tolerant" is N/A: nothing is optional.
  Per-request sqlx errors are caught (`#[from] sqlx::Error`) -> 500; server stays up.

## Items rejected / corrected

- **REJECT** the `/v1/bids` failure-mode assertion "invalid param -> our 400 (ok:true)". Corrected:
  ours 200 (silent drop), upstream 400. See confirmed-issue #3.
- The shape-divergence and "only real shape break" claims are **upheld**.
- The crate-level startup, error-model, and federation descriptions are **upheld**.
