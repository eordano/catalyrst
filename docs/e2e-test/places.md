# E2E test plan — catalyrst-places (key=places)

End-to-end test plan for the catalyrst reimplementation of `places.decentraland.org`.

- Crate: `catalyrst-places`
- Local port: **5134** (`HTTP_SERVER_HOST=127.0.0.1`, `HTTP_SERVER_PORT=5134`)
- Backing store: a PostgreSQL instance, DB `places_events`, table `place`
  (read-only). No migrations.
- Env file: `<ENV_FILE>`
- systemd unit: `catalyrst-places.service`

Upstream contract reference: `decentraland/places` (`src/server.ts`).
Response envelopes confirmed in the port:
- lists: `{ "ok": true, "data": [ ... ], "total": <n> }`
- `/api/map`: `{ "ok": true, "data": { "<base_position>": {place}, ... }, "total": <n> }`
- `/api/categories`: `{ "ok": true, "data": [ { "name", "count", "i18n": { "en" } }, ... ] }`

---

## 1. Unity client config — how to repoint the explorer at our host

All five Places-related URLs the Unity client consumes are produced by the
`RawUrl(...)` switch in:

`Explorer/Assets/DCL/NetworkDefinitions/Browser/DecentralandUrlsSource.cs`

The enum members are defined in:

`Explorer/Assets/DCL/Infrastructure/Utility/DecentralandUrls/DecentralandUrl.cs`
(`ApiPlaces = 16`, `ApiWorlds = 17`, `ApiDestinations = 18`, `Map = 20`, `ContentModerationReport = 21`).

### These are NOT `/about`-discovered

These five URLs are **static, `{ENV}`-templated constants** built in `RawUrl`, with
`CacheBehaviour.STATIC`. They are not realm-dependent and are **not** sourced from the
realm `/about` document (unlike `Lambdas`, `Content`, `EntitiesActive`, etc., which are
`UrlData.RealmDependent(...)` and resolved from `realmData.Ipfs.*`). Therefore they must
be repointed by **editing Unity source**, not by editing our `/about` response.

### Exact lines to change in `DecentralandUrlsSource.RawUrl(...)`

Replace these five arms (line numbers from the current file):

```csharp
// line 165
DecentralandUrl.ApiPlaces => $"https://places.decentraland.{ENV}/api/places",
// line 166
DecentralandUrl.ApiWorlds => $"https://places.decentraland.{ENV}/api/worlds",
// line 167
DecentralandUrl.ApiDestinations => $"https://places.decentraland.{ENV}/api/destinations",
// line 176
DecentralandUrl.Map => $"https://places.decentraland.{ENV}/api/map",
// line 177
DecentralandUrl.ContentModerationReport => $"https://places.decentraland.{ENV}/api/report",
```

with (for a local catalyrst-places on 5134):

```csharp
DecentralandUrl.ApiPlaces        => "http://127.0.0.1:5134/api/places",
DecentralandUrl.ApiWorlds        => "http://127.0.0.1:5134/api/worlds",
DecentralandUrl.ApiDestinations  => "http://127.0.0.1:5134/api/destinations",
DecentralandUrl.Map              => "http://127.0.0.1:5134/api/map",
DecentralandUrl.ContentModerationReport => "http://127.0.0.1:5134/api/report",
```

Drop the `$".../{ENV}/..."` interpolation — there's no `{ENV}` token in a hard host,
so a plain string literal is correct. If you need to keep the host configurable per env,
pass it through a constructor override instead (mirror the `gatekeeperBaseOverride`
pattern at lines 73-77 that injects directly into `cache[...]`).

### Gateway wrapper caveat

`GatewayUrlsSource` (subclass, same dir) lists all five enums in its `SUPPORTED_URLS`
set (lines 23-27) and rewrites them to `https://gateway.decentraland.{env}/places/...`
**only when** the `USE_GATEWAY` feature flag is enabled AND the env is `Org`/`Zone`.
For local testing this flag is off, so the base `RawUrl` value is used verbatim — the
edit above is sufficient. If you test with the gateway flag on, also disable
`USE_GATEWAY` or the request will be re-hosted to `gateway.decentraland.*`.

The consumer that reads these is `PlacesAPIClient.cs`
(`Explorer/Assets/DCL/PlacesAPIService/PlacesAPIClient.cs`, lines 36-42).

---

## 2. Concrete e2e checks (curl)

Bring the service up first:

```bash
systemctl --user start catalyrst-places.service
systemctl --user status catalyrst-places.service --no-pager
```

Sample fixtures from the `places_events.place` table:
- place id `8d670e2d-44eb-48ec-b6d9-36849ebd97af` (base_position `-147,92`, "The HEX Club")
- place id `6cf6d0fb-5902-4f0b-bc8c-dda6b794a957` (base_position `143,-100`, "DCL Family Tree")

### 2.1 Liveness / status

```bash
# ping — expect HTTP 200, body "pong" (or 200 OK marker)
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5134/ping        # expect 200

# status — expect 200 + JSON liveness object
curl -s http://127.0.0.1:5134/api/status | jq .                            # expect {"...":...} 200
```

### 2.2 Single place (GET)

```bash
# known id — expect 200, {"ok":true,"data":{...,"id":"8d670e...","base_position":"-147,92"}}
curl -s http://127.0.0.1:5134/api/places/8d670e2d-44eb-48ec-b6d9-36849ebd97af \
  | jq '{ok, id: .data.id, base_position: .data.base_position}'
# expect ok=true, id matches, base_position="-147,92"

# unknown id — expect 404, {"ok":false,...}
curl -s -o /dev/null -w '%{http_code}\n' \
  http://127.0.0.1:5134/api/places/00000000-0000-0000-0000-000000000000      # expect 404
```

### 2.3 Place list (GET) — envelope + paging + full Unity param set

```bash
# default list — expect 200, {"ok":true,"data":[...],"total":N}; data length<=limit
curl -s 'http://127.0.0.1:5134/api/places?limit=5' \
  | jq '{ok, count: (.data|length), total}'
# expect ok=true, count<=5, total>=count (total reflects whole filtered set)

# offset/limit paging — page 2 differs from page 1
curl -s 'http://127.0.0.1:5134/api/places?limit=3&offset=0' | jq -r '.data[].id' > /tmp/p1
curl -s 'http://127.0.0.1:5134/api/places?limit=3&offset=3' | jq -r '.data[].id' > /tmp/p2
diff /tmp/p1 /tmp/p2 && echo "BUG: pages identical" || echo "OK: pages differ"

# search by name fragment — expect 200, data titles contain the term
curl -s 'http://127.0.0.1:5134/api/places?search=HEX&limit=5' \
  | jq '{ok, count: (.data|length)}'                                          # expect ok=true

# positions filter — expect only that base_position back
curl -s 'http://127.0.0.1:5134/api/places?positions=-147,92' \
  | jq '[.data[].base_position]'                                              # expect ["-147,92"]

# order_by + order — expect 200, ordering applied (e.g. most_active desc)
curl -s 'http://127.0.0.1:5134/api/places?order_by=most_active&order=desc&limit=5' \
  | jq '{ok, count: (.data|length)}'                                          # expect ok=true

# categories filter — expect 200, every row carries the category
curl -s 'http://127.0.0.1:5134/api/places?categories=game&limit=5' \
  | jq '{ok, count: (.data|length)}'                                          # expect ok=true

# only_favorites WITHOUT auth — must short-circuit to empty (upstream parity)
curl -s 'http://127.0.0.1:5134/api/places?only_favorites=true' \
  | jq '{ok, count: (.data|length), total}'
# expect ok=true, count=0, total=0  (OptionalSigner returns empty envelope when unsigned)

# with_realms_detail / sdk accepted as no-ops — must NOT error
curl -s -o /dev/null -w '%{http_code}\n' \
  'http://127.0.0.1:5134/api/places?with_realms_detail=true&sdk=7&limit=1'    # expect 200
```

### 2.4 Place list by ids (POST)

```bash
# expect 200, data only contains the requested ids
curl -s -X POST http://127.0.0.1:5134/api/places \
  -H 'content-type: application/json' \
  -d '{"ids":["8d670e2d-44eb-48ec-b6d9-36849ebd97af","6cf6d0fb-5902-4f0b-bc8c-dda6b794a957"]}' \
  | jq '{ok, ids: [.data[].id], total}'
# expect ok=true, two ids present, total=2

# empty ids — expect 200 empty envelope (or upstream-matching 400; record actual)
curl -s -X POST http://127.0.0.1:5134/api/places \
  -H 'content-type: application/json' -d '{"ids":[]}' \
  | jq '{ok, count: (.data|length)}'                                          # expect count=0
```

### 2.5 Place status by ids (POST)

```bash
# expect 200, status rows for requested ids
curl -s -X POST http://127.0.0.1:5134/api/places/status \
  -H 'content-type: application/json' \
  -d '{"ids":["8d670e2d-44eb-48ec-b6d9-36849ebd97af"]}' \
  | jq '{ok, count: (.data|length)}'                                          # expect ok=true
```

### 2.6 Categories (GET)

```bash
# active catalog with counts + i18n.en — expect 200, data is array of {name,count,i18n.en}
curl -s http://127.0.0.1:5134/api/categories \
  | jq '{ok, count: (.data|length), sample: .data[0]}'
# expect ok=true, count>0, sample has .name, .count (int), .i18n.en (string)

# target filter — expect 200, no error
curl -s -o /dev/null -w '%{http_code}\n' \
  'http://127.0.0.1:5134/api/categories?target=places'                        # expect 200
```

### 2.7 Place categories (GET)

```bash
# categories for a known place — expect 200, {"ok":true,"data":[...]}
curl -s http://127.0.0.1:5134/api/places/8d670e2d-44eb-48ec-b6d9-36849ebd97af/categories \
  | jq '{ok, data}'                                                           # expect ok=true, data array
```

### 2.8 Map overlay (GET)

```bash
# base_position-keyed overlay, 500-row cap — expect 200, data is OBJECT keyed by "x,y"
curl -s http://127.0.0.1:5134/api/map \
  | jq '{ok, total, keys: (.data|keys|length), sample_key: (.data|keys|.[0]), capped: ((.data|keys|length)<=500)}'
# expect ok=true; data is an object (not array); keys<=500; sample_key looks like "x,y"
```

### 2.9 Write endpoints — auth gate then 501 (federation-owned)

All 14 writes + `/api/report` are gated on `RequiredSigner` (withAuth) BEFORE the
federation 501. Unsigned writes must return **401**, not 501.

```bash
# unsigned report — expect 401 (RequiredSigner rejects before stub)
curl -s -o /dev/null -w '%{http_code}\n' -X POST http://127.0.0.1:5134/api/report \
  -H 'content-type: application/json' -d '{}'                                 # expect 401

# unsigned favorite toggle — expect 401
curl -s -o /dev/null -w '%{http_code}\n' -X PATCH \
  http://127.0.0.1:5134/api/places/8d670e2d-44eb-48ec-b6d9-36849ebd97af/favorites \
  -H 'content-type: application/json' -d '{"favorites":true}'                 # expect 401

# unsigned like — expect 401
curl -s -o /dev/null -w '%{http_code}\n' -X PATCH \
  http://127.0.0.1:5134/api/places/8d670e2d-44eb-48ec-b6d9-36849ebd97af/likes \
  -H 'content-type: application/json' -d '{"like":true}'                      # expect 401

# unsigned rating (PUT) — expect 401
curl -s -o /dev/null -w '%{http_code}\n' -X PUT \
  http://127.0.0.1:5134/api/places/8d670e2d-44eb-48ec-b6d9-36849ebd97af/rating \
  -H 'content-type: application/json' -d '{}'                                 # expect 401
```

A *signed* request (valid AuthChain headers `x-identity-auth-chain-*`, signed via
a signed-fetch identity) should pass the gate and then return **501** with a body
pointing at the federation specification. Capture once a signing helper is wired:

```bash
# with valid signed-fetch headers — expect 501 (gate passed, federation stub)
# (headers produced by a signed-fetch helper with a valid identity)
curl -s -o /dev/null -w '%{http_code}\n' -X POST http://127.0.0.1:5134/api/report \
  -H 'content-type: application/json' \
  -H "x-identity-auth-chain-0: ..." -H "x-identity-auth-chain-1: ..." \
  -H "x-identity-auth-chain-2: ..." -d '{...}'                                # expect 501
```

### 2.10 Deferred read STUBs — empty-shape, no 500s

```bash
# worlds list STUB — expect 200, empty upstream-shape envelope (no 500)
curl -s -o /dev/null -w '%{http_code}\n' 'http://127.0.0.1:5134/api/worlds?limit=5'   # expect 200
curl -s 'http://127.0.0.1:5134/api/worlds?limit=5' | jq '{ok, count: (.data|length)}' # expect ok=true,count=0

# destinations STUB — expect 200 empty envelope
curl -s 'http://127.0.0.1:5134/api/destinations' | jq '{ok, data}'            # expect ok=true

# map/places STUB — expect 200
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5134/api/map/places # expect 200

# social HTML STUB — expect 200 text/html
curl -s -o /dev/null -w '%{http_code} %{content_type}\n' \
  'http://127.0.0.1:5134/places/place/'                                       # expect 200 text/html
```

### 2.11 Parity cross-check against production (optional, manual)

For a row present both locally and upstream, diff the JSON shapes (field names/types,
not volatile values like `user_count`):

```bash
curl -s https://places.decentraland.org/api/places/8d670e2d-44eb-48ec-b6d9-36849ebd97af \
  | jq -S '.data|keys' > /tmp/up.keys
curl -s http://127.0.0.1:5134/api/places/8d670e2d-44eb-48ec-b6d9-36849ebd97af \
  | jq -S '.data|keys' > /tmp/local.keys
diff /tmp/up.keys /tmp/local.keys && echo "OK: place key set matches upstream"
```

---

## 3. Real-client smoke step

Goal: confirm a running explorer actually fetches places/map from our host and renders
the discovery/map UI without errors.

### 3.1 Apply the Unity repoint (see section 1)

Edit the five `RawUrl` arms in `DecentralandUrlsSource.cs` to point at
`http://127.0.0.1:5134/...`. Do this in a checkout of the Unity explorer source:

```bash
# edit Explorer/Assets/DCL/NetworkDefinitions/Browser/DecentralandUrlsSource.cs lines 165,166,167,176,177
```

Because these arms are STATIC (not realm-discovered), no `/about` edit is needed and the
Unity build must be rebuilt/relaunched to pick up the change.

### 3.2 Bevy / Godot note

`bevy-explorer` and `godot-explorer` resolve places through their own URL config (the
runtimes do not share Unity's `DecentralandUrlsSource`). For a quick API-level smoke
that does not require a Unity rebuild, prefer the curl suite above. If driving a real
client is required, use the repointed Unity build:

```bash
# launch the explorer
# establish a signed identity (for only_favorites / writes)
# open the Discover / Places panel and the world map; confirm tiles + place cards populate
# capture a screenshot for the artifact
```

Expected: Discover panel lists places, map overlay renders place markers, no red error
toasts. Check the explorer log / `catalyrst-places` log for inbound `GET /api/places`,
`GET /api/map`, `GET /api/categories` hits originating from the client:

```bash
journalctl --user -u catalyrst-places.service -f
```

### 3.3 Acceptance

- All section-2 curl checks return the stated status/shape.
- Unsigned writes return 401 (not 501, not 200).
- `only_favorites=true` unsigned returns an empty envelope.
- `/api/map` data is an object keyed by `base_position`, capped at 500 entries.
- Real client (Unity, repointed) renders Discover + map against 127.0.0.1:5134 with the
  service log showing the inbound requests.

---

## 4. Known gaps / not-yet-testable

- Worlds, destinations, `/api/map/places`, social HTML are STUBs (empty/skeleton) — assert
  they return 200 with the empty upstream shape, not real data.
- All writes + `/api/report` are 501 behind a 401 auth gate — federation owns the write
  path (see the federation specification). Signed-request 501 verification is blocked
  on a signed-fetch test helper.
- No DB migrations to verify; reads hit the `places_events.place` table only.
