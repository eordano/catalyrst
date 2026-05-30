# E2E test plan — `catalyrst-lists` (dcl-lists.decentraland.org)

- **Key:** `dcl-lists`
- **Crate:** `catalyrst-lists`
- **Workspace:** `<WORKSPACE>` (e.g. `/path/to/catalyrst`)
- **Local port:** `5143` (the deployment's assigned port for this service)
- **Backing store:** existing `places_events` DB on the shared database cluster
  (connect via the cluster's socket dir, `<SOCKET_DIR>`), tables `lists_poi` /
  `lists_banned_name`, read-only `pla_*` role. Seeded/refreshed by
  `deploy/sync-lists.sh` (daily 04:15 UTC timer).
- **Routes implemented:** `POST /pois`, `POST /banned-names`, `GET /status`, `GET /ping`.
- **Auth:** all four routes unauthenticated, matching the live upstream catalog.

---

## 1. Unity client config — how to repoint

The explorer reaches this host through **one** enum: `DecentralandUrl.POI`.
`banned-names` is **not** consumed by unity-explorer (it is a marketplace /
DAO-curation surface, used by `catalyrst-market` to filter ENS listings) — there
is nothing to repoint in Unity for it.

### Files
- **Enum:** `unity-explorer/Explorer/Assets/DCL/Infrastructure/Utility/DecentralandUrls/DecentralandUrl.cs`
  - `POI = 19,` (line 35)
- **URL registry (RawUrl switch):** `unity-explorer/Explorer/Assets/DCL/NetworkDefinitions/Browser/DecentralandUrlsSource.cs`
  - **Line 175:**
    ```csharp
    DecentralandUrl.POI => $"https://dcl-lists.decentraland.{ENV}/pois",
    ```

### Is this `/about`-discovered?
**No.** This URL is **NOT** realm/`/about`-discovered. It is a hardcoded host
template in the Unity `RawUrl(...)` switch. The only runtime substitution is
`{ENV}` → `decentralandDomain`, which is derived purely from the launch
environment enum (`DecentralandUrlsSource` ctor, line 43:
`decentralandDomain = environment.ToString()!.ToLower()` → `org` / `zone` /
`today`, with `Today` forced to `org` at line 69). It does **not** come from the
realm `/about` response. Therefore changing our `/about` will **not** redirect
this call — you must edit the Unity source line.

### How to repoint (line 175)
Replace the hardcoded host with the local service. The `{ENV}` token is consumed
by `.Replace(ENV, decentralandDomain)` at runtime, so to point at a fixed local
host drop the `{ENV}` interpolation entirely:

```csharp
// FROM:
DecentralandUrl.POI => $"https://dcl-lists.decentraland.{ENV}/pois",
// TO (local catalyrst-lists on :5143):
DecentralandUrl.POI => $"http://localhost:5143/pois",
```

(If you keep `{ENV}` for env-switching, it will be string-replaced with
`org`/`zone`; a literal localhost URL with no `{ENV}` token is left untouched by
the `.Replace`, which is why the fixed form above is safe.)

### Unity config pointer (verbatim)
`DecentralandUrlsSource.cs:175  DecentralandUrl.POI => $"https://dcl-lists.decentraland.{ENV}/pois"`
→ repoint to `http://localhost:5143/pois` (hardcoded RawUrl switch arm; NOT
/about-discovered, so editing /about does nothing — must edit this Unity line).
`banned-names` has no Unity enum and needs no Unity change.

---

## 2. E2E curl / wscat checks (local service on :5143)

Assume the service is up (`catalyrst-lists.service`) and the bootstrap
has seeded the tables. Live upstream counts at implementation time: **51 POIs**,
**11 banned names** (used as sanity floors, not exact asserts, since the daily
sync tracks upstream).

```bash
# 1. POST /pois — PRIMARY explorer route. Empty body. HTTP 200.
#    Bare {"data":[...]} envelope (NO {ok,...} wrapper). Each item is "x,y".
#    Unity's PointsOfInterestCoordsAPIResponse throws on null .data, so data MUST be present.
curl -sS -i -X POST http://localhost:5143/pois \
  | tee /dev/stderr | grep -q '^HTTP/.* 200'
curl -sS -X POST http://localhost:5143/pois \
  | jq -e '.data | type == "array" and length >= 1' >/dev/null \
  && echo "OK /pois shape"
# shape assert: top-level key is exactly "data" (no "ok"), items look like coords
curl -sS -X POST http://localhost:5143/pois \
  | jq -e 'keys == ["data"] and (.data[0] | test("^-?[0-9]+,-?[0-9]+$"))' >/dev/null \
  && echo "OK /pois coord format + no ok-wrapper"

# 2. POST /banned-names — empty body. HTTP 200. Bare {"data":[...]} of name strings.
curl -sS -i -X POST http://localhost:5143/banned-names \
  | grep -q '^HTTP/.* 200'
curl -sS -X POST http://localhost:5143/banned-names \
  | jq -e 'keys == ["data"] and (.data | type == "array")' >/dev/null \
  && echo "OK /banned-names shape"

# 3. GET /status — status-page healthcheck. HTTP 200. {"commitHash":"<sha>"}.
curl -sS -i http://localhost:5143/status \
  | grep -q '^HTTP/.* 200'
curl -sS http://localhost:5143/status \
  | jq -e 'has("commitHash") and (.commitHash | type == "string") and (.commitHash | length > 0)' >/dev/null \
  && echo "OK /status commitHash"

# 4. GET /ping — liveness; echoes the request path. HTTP 200, body == "/ping".
test "$(curl -sS http://localhost:5143/ping)" = "/ping" && echo "OK /ping echo"

# 5. Method/negative checks: GET on /pois should NOT be 200 (route is POST-only).
curl -sS -o /dev/null -w '%{http_code}\n' http://localhost:5143/pois   # expect 405

# 6. DB-vs-API consistency: API count must equal active rows in the seeded table.
psql "host=<SOCKET_DIR> port=5433 dbname=places_events user=<DB_USER>" \
  -tAc 'SELECT count(*) FROM lists_poi'   # compare to: curl .../pois | jq '.data|length'
```

Expected summary:

| Check | Method/Path | Expected status | Expected shape |
|---|---|---|---|
| 1 | POST /pois | 200 | `{"data":["x,y",…]}`, key set == `["data"]`, len ≥ 1 |
| 2 | POST /banned-names | 200 | `{"data":[…strings]}`, key set == `["data"]` |
| 3 | GET /status | 200 | `{"commitHash":"<non-empty sha>"}` |
| 4 | GET /ping | 200 | body literal `/ping` |
| 5 | GET /pois | 405 | method not allowed (POST-only) |
| 6 | DB consistency | — | `len(.data)` == `count(*)` in `lists_poi` |

---

## 3. Real-client smoke step (dcl-walk — upstream Unity client)

POI is consumed by unity-explorer only, so the real-client smoke uses the Unity
refclient via `dcl-walk`. bevy/godot do not exercise this exact enum, so dcl-bevy
is not the right driver here.

1. Apply the line-175 repoint above to a checkout of the Unity client, editing
   `Explorer/Assets/DCL/NetworkDefinitions/Browser/DecentralandUrlsSource.cs:175`
   to `=> $"http://localhost:5143/pois",`.
2. Ensure `catalyrst-lists` is up and `/pois` returns ≥ 1 coord (check 1 above).
3. Launch the client and confirm POIs render:
   ```bash
   dcl-walk launch
   dcl-walk auth-sign
   # Open the world map / navigation overlay; POIs are the highlighted named parcels.
   dcl-walk shot   # capture a screenshot of the map overlay
   ```
   **Pass criterion:** the map/genesis-city overlay shows named POI markers at the
   coordinates returned by `/pois` (cross-check a couple of coords from the curl
   output against marker positions). A blank/empty POI layer or a client error
   means the parse failed (most likely a null/`ok`-wrapped `.data`).
4. Negative guard: temporarily stop the service and relaunch — the client should
   degrade gracefully (no POI markers), not crash, since `.data` is only read on a
   200.

---

## 4. Notes / deferred surface (out of scope for this e2e pass)

- **Curation write endpoints** (add/remove POI + banned name) are admin-managed
  upstream and deferred to a `catalyrst-fed` signed-write stage — no write tests
  here.
- **On-chain L2 POI contract read** is replaced by an upstream pull+cache
  (`deploy/sync-lists.sh`); validate freshness by re-running the timer and
  re-checking the DB-consistency assert (check 6), not by reading the contract.
- The service connects as the read-only `pla_*` role and does **not** run
  `sqlx::migrate!` at startup; table creation/seed/grants are the bootstrap's job
  (`deploy/bootstrap-catalyrst-lists.sh`). A pre-flight for the e2e run is: tables
  exist and are non-empty — otherwise checks 1/2 will return `{"data":[]}` (still
  200, but Unity shows no POIs).
