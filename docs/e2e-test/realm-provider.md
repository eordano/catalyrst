# E2E test plan — `realm-provider` (catalyrst)

Reimplementation of `realm-provider-ea.decentraland.org` — the realm-discovery
endpoint the explorer loads first.

- **key**: `realm-provider`
- **crate**: `catalyrst-explorer-api`
- **port**: `5137` (`HTTP_SERVER_HOST` / `HTTP_SERVER_PORT`)
- **workspace**: `<WORKSPACE>` (your checkout of catalyrst)
- **upstream host being replaced**: `https://realm-provider-ea.decentraland.org/main`

## 0. How the explorer consumes this host (read this first)

`realm-provider-ea.../main` is a **realm root**, not a leaf API. The Unity client
treats it as the starting realm and self-discovers everything else from it:

1. Unity resolves `DecentralandUrl.Genesis` → `https://realm-provider-ea.decentraland.org/main`
   (`RealmUrls.StartingRealmAsync` for `InitialRealm.GenesisCity`).
2. The realm layer appends `/about` and fetches `…/main/about`.
3. The `/about` JSON body tells the client where the **catalyst content**,
   **lambdas**, **comms adapter / fixedAdapter**, **bff publicUrl**, and
   **realmName** live. Those become the realm-dependent URLs
   (`DecentralandUrl.Content`, `.Lambdas`, `.EntitiesDeployment`, comms, etc.).

**Consequence for repointing:** there is exactly **one** Unity-side line to
change — the `Genesis` enum mapping. Everything reachable *through* the realm
(content/lambdas/comms/bff/realmName) is **`/about-discovered`** — you change it
by editing this service's `/about` (`main_about` in
`crates/catalyrst-explorer-api/src/modules/realm_provider.rs`) and its config env
(`CATALYST_URL`, `LAMBDAS_URL`, `COMMS_ADAPTER`, `COMMS_FIXED_ADAPTER`, `BFF_URL`,
`REALM_NAME`, `PUBLIC_REALM_URL`), **NOT** in Unity.

## 1. Unity config — exact repoint

### Repoint in Unity (one line)

File (in your unity-explorer checkout):
`Explorer/Assets/DCL/NetworkDefinitions/Browser/DecentralandUrlsSource.cs`

Inside the `RawUrl(...)` switch, **line 201**:

```csharp
DecentralandUrl.Genesis => $"https://realm-provider-ea.decentraland.{ENV}/main",
```

Repoint to the local catalyrst service (path suffix `/main` must be kept — the
client appends `/about` to it):

```csharp
DecentralandUrl.Genesis => "http://127.0.0.1:5137/main",
```

Notes:
- `{ENV}` is the `STATIC` placeholder replaced with `org`/`zone`/`today` at
  resolve time (`Probe`/`Url`). The local override is a literal — drop `{ENV}`.
- This enum is also re-cached for the `Today` environment branch (line 66) and
  read by `RealmUrls`, `RealmNavigator` (line 301), `ChatTeleporter`, and
  `RealUserInAppInitializationFlow`; all of them route through the same
  `Url(DecentralandUrl.Genesis)`, so the single switch edit covers every caller.
- Enum value: `DecentralandUrl.Genesis = 1`
  (`Explorer/Assets/DCL/Infrastructure/Utility/DecentralandUrls/DecentralandUrl.cs:12`).
- Alternative to editing source: launch with a **Custom realm**
  (`InitialRealm.Custom` / `customRealm = http://127.0.0.1:5137/main`) via
  `RealmLaunchSettings` — no code change, but does not repoint chat-teleport
  "genesis" or the default fallback.

### `/about-discovered` — do NOT touch Unity for these

These are NOT in `DecentralandUrlsSource`; the client learns them from our
`/about` body. To repoint them, edit the service config / `main_about` handler:

| Field in `/about` | Becomes Unity URL | Set via |
|---|---|---|
| `content.publicUrl` | `DecentralandUrl.Content` / `.EntitiesDeployment` | `CATALYST_URL` (+ `?catalyst=` query) |
| `lambdas.publicUrl` | `DecentralandUrl.Lambdas` | `LAMBDAS_URL` |
| `comms.adapter` / `comms.fixedAdapter` | comms transport | `COMMS_ADAPTER` / `COMMS_FIXED_ADAPTER` |
| `bff.publicUrl` | bff | `BFF_URL` |
| `configurations.realmName` | realm name | `REALM_NAME` |

## 2. Start the service

```bash
cd <WORKSPACE>
HTTP_SERVER_HOST=127.0.0.1 HTTP_SERVER_PORT=5137 \
REALM_NAME=catalyrst \
CATALYST_URL=http://127.0.0.1:5140 \
LAMBDAS_URL=http://127.0.0.1:5142/lambdas \
COMMS_ADAPTER=offline:offline COMMS_FIXED_ADAPTER=offline:offline \
ARCHIPELAGO_URL=http://127.0.0.1:5139 \
CONTENT_PG_CONNECTION_STRING='postgres://<DB_USER>@<SOCKET_DIR>:5433/content' \
cargo run -p catalyrst-explorer-api
```

(`CONTENT_PG_CONNECTION_STRING` and `ARCHIPELAGO_URL` are optional — without them
`synchronizationStatus` falls back to `"Syncing"` and `usersCount` to `0`.)

## 3. E2E curl checks

Run against `http://127.0.0.1:5137`. Each line states expected status + shape.

1. **`GET /ping`** — liveness. Expect `200`, body exactly `/ping`.
   `curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:5137/ping`  → `200`

2. **`GET /main/about`** — the realm-discovery doc the client actually loads.
   Expect `200`, JSON with `healthy:true`, `acceptingUsers:true`, and the four
   blocks `content` / `lambdas` / `configurations` / `comms` / `bff`.
   `curl -s http://127.0.0.1:5137/main/about | jq '{healthy, acceptingUsers, content:.content.publicUrl, lambdas:.lambdas.publicUrl, sync:.content.synchronizationStatus, realm:.configurations.realmName, adapter:.comms.adapter, users:.comms.usersCount, bff:.bff.publicUrl}'`
   - assert `.content.publicUrl == "http://127.0.0.1:5140/content"`
   - assert `.content.synchronizationStatus` ∈ {`"Synced"`,`"Syncing"`}
   - assert `.configurations.realmName == "catalyrst"`
   - assert `.comms.usersCount` is an integer ≥ 0

3. **`GET /about`** — alias of `/main/about` (same handler). Expect identical
   shape to check #2.
   `curl -s http://127.0.0.1:5137/about | jq -e '.configurations.realmName and .comms and .bff'`  → exit 0

4. **`GET /main/about?catalyst=http://127.0.0.1:5140`** — verify the
   `catalyst` query override flows into `content.publicUrl`/`lambdas.publicUrl`.
   `curl -s 'http://127.0.0.1:5137/main/about?catalyst=http://127.0.0.1:5140' | jq -r '.content.publicUrl, .lambdas.publicUrl'`
   → `http://127.0.0.1:5140/content` and `http://127.0.0.1:5140/lambdas`

5. **`GET /realms`** — realm list for the lobby/jump UI. Expect `200`, JSON
   array of length 1 with `serverName`, `url`, `usersCount`.
   `curl -s http://127.0.0.1:5137/realms | jq -e 'length==1 and .[0].serverName and (.[0].usersCount|type=="number")'`  → exit 0

6. **`GET /status`** — health/version probe. Expect `200`, `healthy:true`,
   `name`, `env`, `currentTime` (ms epoch integer), `lastUpdate` (RFC3339).
   `curl -s http://127.0.0.1:5137/status | jq -e '.healthy==true and .name and (.currentTime|type=="number")'`  → exit 0

7. **`GET /hot-scenes`** — DEFERRED. Currently returns `[]` (empty array).
   Expect `200`, body `[]`. No known Unity net-catalog consumer; this check just
   guards the placeholder until a real impl lands.
   `curl -s http://127.0.0.1:5137/hot-scenes | jq -e '.==[]'`  → exit 0

8. **Live `usersCount` cross-check (optional, archipelago up)** — the
   `comms.usersCount` in `/about` should equal `peers_total` from archipelago,
   within the 5s TTL cache.
   `diff <(curl -s http://127.0.0.1:5137/main/about | jq .comms.usersCount) <(curl -s http://127.0.0.1:5139/status | jq .peers_total)`  → no diff (after cache settle)

9. **Live `synchronizationStatus` cross-check (optional, content DB up)** — when
   the freshest active deployment is within 600s of now, expect `"Synced"`,
   else `"Syncing"`. Validate against the DB directly:
   `psql "host=<SOCKET_DIR> port=5433 dbname=content" -tAc "SELECT now()-max(local_timestamp) < interval '600 seconds' FROM deployments WHERE deleter_deployment IS NULL"`
   → `t` should correspond to `/about` reporting `"Synced"`.

### Negative / robustness

10. **Unknown route** — `curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:5137/nope`  → `404`.
11. **Graceful degrade** — start with `ARCHIPELAGO_URL` pointed at a dead port
    and no `CONTENT_PG_CONNECTION_STRING`; `/about` still returns `200` with
    `usersCount:0` and `synchronizationStatus:"Syncing"` (never 5xx).

## 4. Real-client smoke

### Option A — bevy-explorer (fastest, headless-capable)

`bevy-explorer` reads `/about` the same way. Point it at our realm and confirm it
loads without falling back to prod.

```bash
dcl-bevy up
# launch against the local realm root (client appends /about):
dcl-bevy launch --realm http://127.0.0.1:5137/main      # or set in dcl-bevy env/config
dcl-bevy shot                                            # capture frame after load
dcl-bevy logs | grep -iE 'realm|about|catalyst|comms'   # confirm it parsed our /about
```
PASS criteria: logs show the realm resolved to `catalyrst` / our content+lambdas
URLs (not `realm-provider-ea.decentraland.org`), client reaches the spawn flow,
no realm-fetch error. (Comms is `offline:offline`, so expect a single-player
session — that is correct for this host.)

### Option B — upstream Unity client (full discovery path)

1. Apply the line-201 repoint above (or launch with a Custom realm
   `http://127.0.0.1:5137/main`).
2. Build/launch headlessly:
   ```bash
   dcl-walk launch
   dcl-walk auth-sign            # complete auth
   dcl-walk shot                 # screenshot after realm load
   ```
   (Use whatever headless-drive tooling you have for launching the Unity client.)
3. Confirm in `dcl-walk logs` (or editor inspect) that the client resolved
   `DecentralandUrl.Genesis` → `http://127.0.0.1:5137/main`, fetched
   `/main/about`, and adopted the discovered content/lambdas/comms URLs.

PASS criteria: client enters Genesis spawn (or single-player offline session)
sourcing scenes from our catalyst content server, with realm name `catalyrst`
shown, and no requests to `realm-provider-ea.decentraland.org`.

## 5. Coverage summary

| Route | Check(s) | Status |
|---|---|---|
| `GET /ping` | 1 | implemented |
| `GET /main/about` | 2, 4, 8, 9, 11 | implemented (live best-effort sync + usersCount) |
| `GET /about` | 3 | implemented (alias) |
| `GET /realms` | 5 | implemented |
| `GET /status` | 6 | implemented |
| `GET /hot-scenes` | 7 | DEFERRED — returns `[]`; no confirmed Unity consumer |

Intentionally dropped upstream features (do not test, will not return prod
behaviour): DAO discovery, geolocation, blacklist-based realm filtering.
