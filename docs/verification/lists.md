# catalyrst-lists (service "lists") — adversarial verification

Crate: `crates/catalyrst-lists`
Branch: `feat/service-plane-crates` (committed tree; only `docs/verification/`
is untracked, all `.rs` under the crate are committed — verified via
`git status`). Nothing run; analysis from code + net-catalog only.

Upstream: `dcl-lists.decentraland.org` — **closed-source, confirmed absent**
from every `github.com-decentraland*/` mirror. The wire contract is pinned only
by public *consumers* (places-website, marketplace-server/webapp, builder,
unity-explorer); all expect `{ "data": string[] }`.

Bundle: lists is a member of **explore** (5143). Standalone default bind
127.0.0.1:5151 (config.rs; HTTP_SERVER_HOST/PORT defaulted). Backing store:
`places_events` DB, tables `lists_poi` / `lists_banned_name`.

## Router surface (verified)

- `lib.rs:43-50` `api_router()`: `POST /pois`, `POST /banned-names` — merged in
  BOTH standalone and bundle mode.
- `main.rs:24-26` adds `GET /health`, `GET /ping`, `GET /status` — standalone
  ONLY; not in `api_router()`, so absent from the explore bundle.

Net-catalog (`the Unity net-catalog`): exactly one dcl-lists
call — `POST https://dcl-lists.decentraland.{ENV}/pois` from
`PlacesAPIClient.cs:427`. No Unity caller for `/banned-names` (only other
dcl-lists reference is the `/pois` URL constant at `DecentralandUrlsSource.cs:175`).

## Per-endpoint

| endpoint | shape | client-reaction | severity | failure-modes-ok | notes |
|---|---|---|---|---|---|
| `POST /pois` | match | ok | none | mostly (see gaps) | `ListResponse{data:Vec<String>}` (response.rs:3-12) -> `{"data":["x,y",...]}`. Unity DTO `PointsOfInterestCoordsAPIResponse{List<string> data}` (PointsOfInterestCoordsAPIResponse.cs:9) matches field/casing/nesting. `data` is non-`Option`, always serialized, empty -> `[]`, never null/omitted. |
| `POST /banned-names` | match (no Unity consumer) | n/a (server-side only) | none | yes | Same `{data:[...]}` envelope; `SELECT name FROM lists_banned_name ORDER BY name` (ports/lists.rs:23-26). Consumed by marketplace-server `src/ports/nfts/utils.ts:8-13` (reads `data.data:string[]`, try/catch -> `[]`), webapp, builder, and locally catalyrst-market. Not in original finding; not Unity-called. |
| `GET /health` | own body | n/a | none | yes | 200 `{"ok":true}` / 503 `{"ok":false,"message":"database unreachable"}` (health.rs) via `SELECT 1`. Standalone-only. |
| `GET /ping` | path echo | n/a | none | yes | Returns request path as plain text (ping.rs). Standalone-only. |
| `GET /status` | `{"commitHash":...}` | n/a | none | yes | status.rs. Standalone-only. |

## Verdict on the original finding (`POST /pois`)

**CONFIRMED ACCURATE** on the committed tree (shape match / client ok /
severity none):

- (a) Shape divergence is real and is *zero*. `ListResponse.data` is a plain
  `Vec<String>`; serde always emits the `data` key; empty -> `[]`. Field name,
  casing, nesting all match the Unity `[Serializable]` DTO. The 42P01
  empty-list guard (ports/lists.rs:34-37) means even an unseeded table yields a
  present, non-null `data`.
- (b) Client reaction is correct. The C# guard
  `if (response.data == null) throw new Exception("No POIs info retrieved")`
  (PlacesAPIClient.cs:433-434) is a genuine throw-on-null — but our `data` is
  never null, so it never fires. And the live consumer at
  `ScenesOfInterestMarkersController.cs:191-192` wraps the whole call in
  `.SuppressAnyExceptionWithFallback(Array.Empty<string>(), ReportCategory.UI)`,
  so any throw (null-guard, 500, parse failure, 404) is swallowed to an empty
  array. No null-crash, no propagated request-throw.

## Confirmed issues

None. No shape divergence, no client-crash, no live endpoint mishandled.
`/banned-names` is dead from the Unity client's perspective but shape-correct
for its server-side consumers, so it is harmless rather than an issue.

## Client-crash risks

None. Both required-field guards in the consumer chain are neutralized:
- `response.data == null` throw (PlacesAPIClient.cs:434) is unreachable because
  our `data` is never null/omitted.
- Any resulting exception is suppressed with an empty-array fallback
  (ScenesOfInterestMarkersController.cs:192).

## Failure-mode gaps

The original finding's failure-mode table is essentially right. Notes:

- **Transient runtime DB error -> 500** `{"ok":false,"message":"database error"}`
  (errors.rs:19-22). Finding marks `ok:false`. Confirmed we 500 where a closed
  upstream convention *might* prefer an empty list — but the Unity consumer's
  suppress-with-fallback (and places-website's try/catch -> `[]`,
  pois.ts:11-19) fully tolerate it. So this is at most a cosmetic divergence
  from a presumed convention, not a client-visible defect. Acceptable.
- **Missing table 42P01 -> 200 `{"data":[]}`** (ports/lists.rs:34-37).
  Confirmed: only the `42P01` code is swallowed; every other `sqlx::Error`
  falls through to 500. Deliberate, reasonable degrade.
- **Bundle-mode DB-down precision.** In the explore bundle, `mount()`
  (catalyrst-explore/main.rs:62-74) catches the `Err` from `build_lists()`,
  marks `lists` "down" in the bundle `/health`, and never merges
  `api_router()`. So `/pois` returns **404** (route absent), not 500, while the
  lists DB is unreachable *at boot*. A *runtime* (post-boot) outage does not
  unmount the route; it yields per-request 500s. Either way the Unity consumer
  tolerates it via suppress-with-fallback. No client crash.

## Startup / error model (crate-level, confirmed)

- Panic-free startup. `build_state` (lib.rs:24-41) parses the connection string
  and eagerly connects a pool (max 5, statement_timeout 60s,
  idle_in_transaction_session_timeout 30s); missing/malformed
  `PLACES_PG_COMPONENT_PSQL_CONNECTION_STRING` or unreachable DB -> `Err` with
  anyhow context, never panic.
- Standalone `main.rs:21` propagates that `Err` via `?` -> non-zero exit
  (fail-fast). Bundle catches it (degrade, route unmounted).
- Single `ApiError` enum (errors.rs): `Database` and `Internal`, both -> 500
  `{"ok":false,"message":...}`. No 4xx path exists because no handler
  reads/validates request input (POST body ignored, no path/query params, no
  auth — matching the unauthenticated upstream). Machine-stable, coherent.
