# Parity report: catalyrst-lists (`lists`) vs. `dcl-lists`

Upstream service: `dcl-lists` (dcl-lists.decentraland.{ENV}). **Source is NOT cloned** on this
machine (`ls github.com-decentraland*/ | grep -i list` yields only `decentraland-lists-graph`
and `whitelist-sale`, neither of which is this service). All shape verification is therefore done
against the **consumers** of the API (Unity explorer + marketplace/builder webapps), and all
efficiency comparison is structurally impossible (no upstream implementation to read).

Live diff: not-applicable (dcl-lists not running locally).

## Per-endpoint table

| Endpoint | Shape | Efficiency | Severity | Notes |
|---|---|---|---|---|
| `POST /pois` | match | unknown | none | `{data: Vec<String>}` -> bare `{"data":[...]}`; matches Unity `PointsOfInterestCoordsAPIResponse.data` (List<string>). Never null. |
| `POST /banned-names` | match | unknown | none | Same `{"data":[...]}` shape; matches marketplace + builder `{ data: string[] }` clients. Absence of `ok` envelope is load-bearing (see below). |
| `GET /ping` | unknown | unknown | none | Local liveness route; raw path string text/plain. Not in dcl-lists contract; nothing to diff. |
| `GET /status` | unknown | unknown | none | `{"commitHash": <sha\|version>}` build-metadata route. Not in dcl-lists contract; nothing to diff. |

## Confirmed shape findings

All shape verdicts in the input set are **confirmed correct**. No shape issues found.

- **`POST /pois` — match (verified).** Our `ListResponse { data: Vec<String> }`
  (`crates/catalyrst-lists/src/http/response.rs:7-10`) serializes as `{"data":[...]}` with no
  `#[serde(rename_all)]`, so the key is literally `data`. The Unity consumer
  (`unity-explorer/.../PointsOfInterestCoordsAPIResponse.cs:9`) is `public List<string> data;`,
  parsed via Unity JSON in `PlacesAPIClient.GetPointsOfInterestCoordsAsync`
  (`PlacesAPIClient.cs:430`) and null-guarded at line 433 (`if (response.data == null) throw`).
  We always emit a JSON array (empty Vec -> `[]`, never null), so the guard is satisfied. The
  net-catalog confirms `POST https://dcl-lists.decentraland.{ENV}/pois` with empty body. Element
  type `string` ("x,y" coord) matches `List<string>`. No `ok` envelope (correct — dcl-lists has
  no wrapper, unlike catalyrst-places).

- **`POST /banned-names` — match (verified, and the no-envelope claim is stronger than stated).**
  Both upstream clients read `{ data: string[] }` and return `response.data`:
  `marketplace/webapp/src/modules/vendor/decentraland/lists/api.ts:7-10` and
  `builder/src/lib/api/lists.ts:7-10`. Both go through `decentraland-dapps` `BaseAPI.request`
  (`decentraland-dapps/src/lib/api.ts:33-84`). Its `parseResponse` (lines 76-84) does:
  if the body has a boolean `ok` field, return the body as-is; **otherwise wrap** as
  `{ ok: true, data: <whole body>, error: '' }`, and `request` returns that `.data`. Our body is
  `{"data":[...]}` with **no** `ok` field -> it takes the wrap branch -> `request` returns the
  whole `{"data":[...]}` object -> `fetchBannedNames` reads `.data` -> the array. Correct.
  Critically, if we *had* added an `ok` wrapper (`{ok:true, data:[...]}`), `parseResponse` would
  return it directly, `request` would return `.data` = the bare array, and `fetchBannedNames`
  reading `response.data` on an array would yield `undefined`. So the absence of the `ok`
  envelope here is **load-bearing for correctness**, not cosmetic.

- **`GET /ping`, `GET /status` — unknown (verified, correctly classified).** Both are
  catalyrst-local util/health routes, registered only in `main.rs:24-25` and deliberately excluded
  from `api_router()` (`lib.rs:48-55`) to avoid duplicate-path panics on shared-router merge — as
  the finding states; confirmed in source. Neither is part of the dcl-lists public contract nor in
  the net-catalog, so there is no upstream handler to diff against. `unknown` is the right verdict.

## Confirmed efficiency wins

**None.** No "better" claim is made or warranted. All four efficiency verdicts are `unknown`,
which is correct:

- `pois()` and `banned_names()` (`ports/lists.rs:19-34`) are each a single
  `SELECT ... ORDER BY ...` on a read-only `places_events` pool (`max_connections(5)`,
  `statement_timeout=60000`, confirmed in `lib.rs:27-38`), with no cache, no pagination, full Vec
  materialized per request.
- `dcl-lists` source is not on disk, so its query count / ORM / redis cache / data backing cannot
  be inspected. No structural better-or-worse comparison is possible, and none was claimed.
- `/ping` and `/status` do no DB/cache work; no upstream equivalent exists.

The findings correctly note that the lack of caching on the two list endpoints is a *latent*
optimization (the banned-name denylist sits in the marketplace ENS-listing hot filter path and a
moka/in-memory cache would help), but both tables are small and refreshed out-of-band by
`bootstrap-catalyrst-lists.sh`, so it is not a correctness or material-performance concern, and it
cannot be scored as a "worse" without the upstream to compare against. Severity: none.

## Rejected during verification

- Nothing rejected. Every shape and efficiency verdict in the input set survived adversarial
  re-checking. (The only refinement: the `POST /banned-names` no-`ok`-envelope claim was upgraded
  from "stylistically correct" to "load-bearing for correctness" after reading `BaseAPI.parseResponse`.)
