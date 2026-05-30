# E2E Test Plan — catalyrst-archipelago (key=`archipelago`)

Reimplementation of `archipelago-ea-stats.decentraland.org`.

| | |
|---|---|
| Crate | `catalyrst-archipelago` |
| Port | **5139** (the deployment's assigned port; `5139` is used in the examples below) |
| Workspace | `<WORKSPACE>` |
| Shared DB | yes — reuses the catalyst `content` DB (`deployments` + `content_files`) on the shared database cluster; no migrations |
| Upstream host | `https://archipelago-ea-stats.decentraland.org` |
| Unity enums for this host | `ArchipelagoStatus`, `ArchipelagoHotScenes`, `RemotePeers` |

---

## 1. Unity config — how to repoint

All three enums are **hardcoded in the `RawUrl(...)` switch**, NOT realm/`/about`-discovered.
They are plain literals built from the `{ENV}` token, so changing our `/about` response does
**nothing** for these — they must be edited in Unity (or overridden via `GatewayUrlsSource` /
a build flavor). The realm `/about` only carries `comms.adapter`, `content`, `lambdas`, etc.,
none of which feed these three URLs.

File: `Explorer/Assets/DCL/NetworkDefinitions/Browser/DecentralandUrlsSource.cs`
(enum members defined in `Explorer/Assets/DCL/Infrastructure/Utility/DecentralandUrls/DecentralandUrl.cs`:
`ArchipelagoStatus = 2`, `ArchipelagoHotScenes = 4`, `RemotePeers = 32`).

Exact lines in the `RawUrl` switch to repoint:

```csharp
// line 189
DecentralandUrl.RemotePeers       => $"https://archipelago-ea-stats.decentraland.{ENV}/comms/peers",
// line 198
DecentralandUrl.ArchipelagoStatus => $"https://archipelago-ea-stats.decentraland.{ENV}/status",
// line 199
DecentralandUrl.ArchipelagoHotScenes => $"https://archipelago-ea-stats.decentraland.{ENV}/hot-scenes",
```

To point a local build at our service, replace the three RHS literals (drop the `{ENV}`
substitution since our host is fixed):

```csharp
DecentralandUrl.RemotePeers          => "http://localhost:5139/comms/peers",
DecentralandUrl.ArchipelagoStatus    => "http://localhost:5139/status",
DecentralandUrl.ArchipelagoHotScenes => "http://localhost:5139/hot-scenes",
```

Notes:
- `RemotePeers` is consumed at `DynamicWorldContainer.cs:256` and listed in
  `GatewayUrlsSource.cs:38`. `ArchipelagoStatus`/`ArchipelagoHotScenes` are read by the
  minimap/`/goto` flows. All three are also pre-warmed in the `Today` env block
  (lines 64-68) — harmless, but be aware the `Today` env force-rewrites the domain to `org`.
- Cleaner alternative to editing the switch: override in `GatewayUrlsSource` (the gateway
  flavor of `DecentralandUrlsSource`) which already enumerates `RemotePeers`, or inject a
  `cache[...] = new UrlData(CacheBehaviour.STATIC, "http://localhost:5139/...")` for each of
  the three in the constructor — same pattern as the existing `gatekeeperBaseOverride` block
  at lines 73-77. This avoids touching the upstream switch and survives merges.
- Caching: these three are `CacheBehaviour.STATIC`, so a single edit is picked up on next
  process start; no realm-change invalidation needed.

**`/about`-discovered?** No. These URLs are static literals in Unity; they cannot be changed
by editing our `/about` response.

---

## 2. Service bring-up

The service is not currently listening on its port. Start it first.

```bash
# Build (verifies clean compile)
cargo run -p catalyrst-archipelago >/tmp/archipelago.log 2>&1 &

# Optional: enable hot-scenes content enrichment (otherwise /hot-scenes returns [])
#   point at a PostgreSQL instance holding the content DB (creds in the service's environment file)
#   export CONTENT_PG_CONNECTION_STRING=... CONTENT_URL=https://peer.decentraland.org/content/
# Optional: export COMMIT_HASH=$(git -C <WORKSPACE> rev-parse --short HEAD)

# Confirm listening
ss -ltnp | grep 5139
```

For the curl checks below, `BASE=http://localhost:5139`.

---

## 3. Concrete e2e checks (curl / wscat)

### 3.1 `GET /status` — `ArchipelagoStatus`
```bash
curl -sS -i http://localhost:5139/status
```
Expect: `200`, `Access-Control-Allow-Origin: *` header present, JSON body
`{"version":"<crate ver>","currentTime":<ms epoch>,"commitHash":"<COMMIT_HASH or empty>"}`.
`currentTime` must be milliseconds (13 digits), not seconds.
```bash
curl -sS http://localhost:5139/status | jq -e '.version and (.currentTime>1e12) and has("commitHash")'
curl -sS -D- -o/dev/null http://localhost:5139/status | grep -i 'access-control-allow-origin: \*'
```

### 3.2 `GET /hot-scenes` — `ArchipelagoHotScenes`
```bash
curl -sS http://localhost:5139/hot-scenes
```
Expect: `200`, a **bare JSON array** (NOT wrapped in an object), length <= 100, sorted
descending by `usersTotalCount`. With no content DB configured it is `[]`.
```bash
# top-level is an array, <=100 entries
curl -sS http://localhost:5139/hot-scenes | jq -e 'type=="array" and length<=100'
# monotonic non-increasing usersTotalCount
curl -sS http://localhost:5139/hot-scenes | jq -e '[.[].usersTotalCount] as $u | ($u==($u|sort|reverse))'
# element shape (only when non-empty)
curl -sS http://localhost:5139/hot-scenes | jq -e '(length==0) or (.[0] | has("id") and has("name") and has("basePosition") and has("usersTotalCount") and has("parcels"))'
```

### 3.3 `GET /comms/peers` (+ bare `/peers`) — `RemotePeers`
```bash
curl -sS http://localhost:5139/comms/peers
curl -sS http://localhost:5139/peers          # bare alias must match
```
Expect: `200`, `{"ok":true,"peers":[{id,address,lastPing,parcel,position}, ...]}`.
`parcel` is DERIVED from `position` (floor(x/16), floor(z/16)) — verify it matches.
```bash
curl -sS http://localhost:5139/comms/peers | jq -e '.ok==true and (.peers|type=="array")'
# derived-parcel invariant (skip if no peers connected)
curl -sS http://localhost:5139/comms/peers | jq -e '.peers|all((.position[0]|floor/16|floor)==.parcel.x and (.position[2]|floor/16|floor)==.parcel.y) // true'
# repeated ?id= filter is honored
curl -sS "http://localhost:5139/comms/peers?id=0xAAA&id=0xBBB" | jq -e '.peers|all(.address|ascii_downcase|. == "0xaaa" or . == "0xbbb") // true'
```

### 3.4 `GET /comms/peers/:id` (+ bare)
```bash
curl -sS -o/dev/null -w '%{http_code}\n' http://localhost:5139/comms/peers/0xdoesnotexist   # -> 404
```
Expect `404` for unknown; `200` `{"ok":true,"peer":{...}}` for a connected address.

### 3.5 `GET /comms/parcels` (+ bare `/parcels`)
```bash
curl -sS http://localhost:5139/comms/parcels | jq -e 'has("parcels") and (.parcels|type=="array")'
curl -sS http://localhost:5139/comms/parcels | jq -e '.parcels|all(has("peersCount") and (.parcel|has("x") and has("y"))) // true'
```
Expect `200`, `{"parcels":[{peersCount,parcel:{x,y}}]}`.

### 3.6 `GET /comms/islands` (+ bare) and `/comms/islands/:id`
```bash
curl -sS http://localhost:5139/comms/islands | jq -e '.ok==true and (.islands|type=="array")'
curl -sS -o/dev/null -w '%{http_code}\n' http://localhost:5139/comms/islands/no-such-island   # -> 404
```
Expect `200` `{"ok":true,"islands":[IslandResult]}`; `:id` returns bare `IslandResult` or `404`.

### 3.7 `GET /core-status` (parity)
```bash
curl -sS http://localhost:5139/core-status | jq -e 'has("healthy") and has("userCount")'
```
Expect `200`, `{"healthy":<bool>,"userCount":<int>}`.

### 3.8 Untouched core surface — regression guard
These were left intact; confirm the reshape did not break them.
```bash
curl -sS -o/dev/null -w '%{http_code}\n' http://localhost:5139/ping            # 200
curl -sS -o/dev/null -w '%{http_code}\n' http://localhost:5139/stats/health    # 200
curl -sS -o/dev/null -w '%{http_code}\n' http://localhost:5139/stats/peers     # 200
curl -sS -o/dev/null -w '%{http_code}\n' http://localhost:5139/gossip/info     # 200
```

### 3.9 WebSocket comms plane (cluster join) — `wscat`
The crate keeps the LiveKit/WS cluster core. Confirm the auth-gated WS endpoint accepts a
connection (auth challenge first via `/auth/challenge`). Populating a real peer is what makes
3.3/3.5 return non-empty data.
```bash
# inspect the WS route path
grep -n 'WebSocketUpgrade\|\.route(.*ws\|on_upgrade' \
  crates/catalyrst-archipelago/src/ws.rs
# then, e.g.:
# wscat -c "ws://localhost:5139/<ws-path>?token=<jwt-from-/auth/livekit-token>"
```
Expect: upgrade to `101`; after a `/heartbeat` POST with a position, the peer appears in
`/comms/peers` and its derived parcel in `/comms/parcels`.

---

## 4. Real-client smoke step

Goal: a real explorer hits our `/status`, `/hot-scenes`, `/comms/peers` and renders the
minimap + nearby-players without errors.

**bevy-explorer (preferred — fastest, native):** bevy reads peer/archipelago endpoints from
the realm `/about` + its own config, so the cleanest path is to point it at a local realm whose
`/about` advertises our comms; but the three Archipelago URLs in *Unity* are static, so for a
Unity smoke you must rebuild with the repointed switch (section 1).

1. Unity path (authoritative for these enums):
   - Apply section 1 edit (prefer the constructor-cache override variant).
   - Launch the explorer and complete auth sign-in.
   - Open the map/minimap; trigger `/goto` (hot-scenes) and check nearby-players (remote peers).
   - Watch the player log for requests to `localhost:5139/status`, `/hot-scenes`,
     `/comms/peers` and confirm `200`s (no parse errors — the bare-array shape of `/hot-scenes`
     and the `{ok,peers}` envelope are the common break points).
   - `dcl-walk` screenshot the minimap to confirm hot-scene heat overlays render.

2. bevy cross-check (shape sanity without a Unity rebuild):
   - `dcl-bevy up`, teleport to a populated parcel, confirm nearby-player avatars appear —
     validates the peers/position->parcel derivation end-to-end against a second runtime.

**Pass criteria:** all section 3 checks green; Unity client logs show `200` for all three
Archipelago URLs against `localhost:5139`; minimap heat + nearby players render with no
deserialization errors in the player log.

---

## 5. Known parity gotchas to watch in testing
- `/hot-scenes` is a **bare array**, not `{...}`. A regression to an object wrapper silently
  breaks the Unity `/goto` deserializer.
- `parcel` everywhere is **derived from world position** (`floor(coord/16)`), not the stored
  heartbeat parcel. Test data must set `position` for the derivation to be meaningful.
- `/status.currentTime` is **milliseconds**.
- `/status` must carry `Access-Control-Allow-Origin: *`.
- `thumbnail` in hot-scenes: absolute `http(s)` passthrough, else content-file hash rewritten
  to `{CONTENT_URL}/contents/{hash}`. Verify against a scene with a relative thumbnail.
- With `CONTENT_PG_CONNECTION_STRING` unset, `/hot-scenes` is `[]` by design — set it (creds
  in the service's environment file) to exercise enrichment.
