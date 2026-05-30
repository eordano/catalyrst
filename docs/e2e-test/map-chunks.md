# E2E test plan — `map-chunks` (catalyrst-map)

Reimplementation of `api.decentraland.org` map-chunk PNG rendering
(`DecentralandUrl.ApiChunks`). Crate: `catalyrst-map`. Port: **5143**.

- Workspace: `<WORKSPACE>`
- Upstream parity target: `decentraland/atlas-server`
  (`src/modules/render/{viewport,map,tile}.ts` + `image/component.ts`).
- Auth: **none**. No AuthChain, no signed fetch — this host serves a public PNG.
- Data: read-only on the `atlas` DB (`atlas.tiles`) reached over a unix socket
  at `<SOCKET_DIR>` port `5433`. No new tables, no migration.

Routes implemented:
- `GET /v1/map.png`
- `GET /v2/map.png` (alias, same handler)
- `GET /v2/ping`
- `GET /v2/ready`

Deferred (return 404 today; out of scope for this lane — Unity never calls them):
`/v1/parcels/:x/:y/map.png`, `/v1/estates/:estateId/map.png`,
`/v1/minimap.png`, `/v1/estatemap.png`, `/v1/tiles`, `/v2/tiles`, `/v2/districts`.

---

## 1. Unity config — how to repoint `ApiChunks`

### What the client actually requests

`Explorer/Assets/DCL/MapRenderer/MapLayers/Atlas/ParcelAtlas/ParcelChunkController.cs`
builds the request as:

```
{Url(DecentralandUrl.ApiChunks)}?center={x},{y}&width={chunkSize}&height={chunkSize}&size={parcelSize}
```

and consumes the response via `webRequestController.GetTextureAsync(...)`, i.e. it
expects a decodable image (our `image/png`). The query-param contract
(`center`, `width`, `height`, `size`) is exactly what `catalyrst-map`'s
`extract_params` parses, so no shape change is needed — only the base URL.

### The line to change (NOT realm-discovered)

`ApiChunks` is a **hardcoded `RawUrl` switch template**, resolved entirely
inside Unity. It is **not** sourced from the realm `/about` response, so editing
our `/about` will NOT move it — you must edit Unity.

File: `Explorer/Assets/DCL/NetworkDefinitions/Browser/DecentralandUrlsSource.cs`

Line **187** (the `RawUrl(DecentralandUrl url)` switch arm):

```csharp
DecentralandUrl.ApiChunks => $"https://api.decentraland.{ENV}/v1/map.png",
```

Repoint to the local service:

```csharp
DecentralandUrl.ApiChunks => "http://127.0.0.1:5143/v1/map.png",
```

(Enum member: `DecentralandUrl.ApiChunks = 15` in
`Explorer/Assets/DCL/Infrastructure/Utility/DecentralandUrls/DecentralandUrl.cs`.)

### Gateway caveat

`Explorer/Assets/DCL/NetworkDefinitions/Browser/GatewayUrlsSource.cs` lists
`DecentralandUrl.ApiChunks` in `SUPPORTED_URLS`. When the `USE_GATEWAY` feature
flag is enabled on Org/Zone, `RawUrl` output for `ApiChunks` gets its subdomain
rewritten to `https://gateway.decentraland.<env>/...`. Since we are hardcoding a
`127.0.0.1` URL (no `.decentraland.<env>` suffix), the gateway rewrite is a
no-op and our override stands. If you instead point at a `*.decentraland.org`
host, disable `USE_GATEWAY` or the request will be re-hosted at `gateway.`.

**Summary: edit DecentralandUrlsSource.cs line 187 (`RawUrl` ApiChunks arm).
This is hardcoded in Unity, NOT /about-discovered.**

---

## 2. Bring the service up

Install the env and (optionally) a systemd unit (`catalyrst-map.service`), then
run:

```bash
# one-time: install env
cp <WORKSPACE>/crates/catalyrst-map/catalyrst-map.env.example <ENV_FILE>

# build + run
cargo run -p catalyrst-map

# confirm it bound
ss -tln | grep 5143
```

---

## 3. E2E checks (curl)

Run after the service is listening on `5143`. Each line states the expected
status/shape.

```bash
# (1) liveness — 200, body "/v2/ping"
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5143/v2/ping
# expect: 200

# (2) readiness — 200 "ready" once ~92k tiles are loaded (503 "Not ready" while loading)
curl -s -w ' [%{http_code}]\n' http://127.0.0.1:5143/v2/ready
# expect: ready [200]

# (3) default map render — 200, image/png, a real PNG (\x89PNG magic)
curl -s -D - -o /tmp/map_default.png http://127.0.0.1:5143/v1/map.png | grep -i '^content-type'
file /tmp/map_default.png
# expect: Content-Type: image/png ; file => "PNG image data"

# (4) v2 alias is byte-identical to v1 for the same params
curl -s -o /tmp/map_v1.png 'http://127.0.0.1:5143/v1/map.png?center=0,0&width=512&height=512&size=20'
curl -s -o /tmp/map_v2.png 'http://127.0.0.1:5143/v2/map.png?center=0,0&width=512&height=512&size=20'
cmp /tmp/map_v1.png /tmp/map_v2.png && echo "v1==v2 OK"
# expect: v1==v2 OK

# (5) Unity-shaped request (matches ParcelChunkController.LoadImageAsync) — 200 PNG, dims 512x512
curl -s -o /tmp/map_unity.png 'http://127.0.0.1:5143/v1/map.png?center=0,0&width=512&height=512&size=20'
file /tmp/map_unity.png
# expect: "PNG image data, 512 x 512"

# (6) Genesis Plaza center sanity — non-trivial PNG (> a few hundred bytes, not the empty checkerboard)
curl -s -o /tmp/map_genesis.png 'http://127.0.0.1:5143/v1/map.png?center=0,0&width=1024&height=1024&size=20'
test "$(stat -c%s /tmp/map_genesis.png)" -gt 1000 && echo "non-empty render OK"
# expect: non-empty render OK

# (7) cache headers present — Last-Modified, Cache-Control, ETag
curl -s -D - -o /dev/null http://127.0.0.1:5143/v1/map.png \
  | grep -iE '^(last-modified|cache-control|etag):'
# expect: Last-Modified: <RFC1123>
#         Cache-Control: max-age=120, s-maxage=120, stale-while-revalidate=180, public
#         ETag: "<hex>-<len>"

# (8) conditional GET — echo Last-Modified back, expect 304
LM=$(curl -s -D - -o /dev/null http://127.0.0.1:5143/v1/map.png | grep -i '^last-modified:' | sed 's/^[Ll]ast-[Mm]odified: //; s/\r//')
curl -s -o /dev/null -w '%{http_code}\n' -H "If-Modified-Since: $LM" http://127.0.0.1:5143/v1/map.png
# expect: 304

# (9) selection overlay — `selected` param renders (red highlight); valid PNG, differs from unselected
curl -s -o /tmp/map_sel.png   'http://127.0.0.1:5143/v1/map.png?center=0,0&width=512&height=512&size=20&selected=0,0'
curl -s -o /tmp/map_nosel.png 'http://127.0.0.1:5143/v1/map.png?center=0,0&width=512&height=512&size=20'
file /tmp/map_sel.png; cmp -s /tmp/map_sel.png /tmp/map_nosel.png && echo "SAME (bug)" || echo "overlay differs OK"
# expect: PNG image data ; overlay differs OK

# (10) param clamping (atlas-server extractParams parity): size clamps to [5,50], w/h to [100,4096]
curl -s -o /tmp/map_clamp_lo.png 'http://127.0.0.1:5143/v1/map.png?width=1&height=1&size=1'      # -> 100x100, size 5
curl -s -o /tmp/map_clamp_hi.png 'http://127.0.0.1:5143/v1/map.png?width=99999&height=99999&size=999' # -> 4096x4096, size 50
file /tmp/map_clamp_lo.png /tmp/map_clamp_hi.png
# expect: "PNG image data, 100 x 100" and "PNG image data, 4096 x 4096"

# (11) garbage params tolerated (JS parseInt-style prefix parse / defaults), still 200 PNG
curl -s -o /dev/null -w '%{http_code}\n' 'http://127.0.0.1:5143/v1/map.png?center=abc&width=xyz&size=&height='
# expect: 200

# (12) deferred routes are not (yet) served — 404
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5143/v1/tiles
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5143/v2/districts
# expect: 404 (both) — documented as deferred, not a regression
```

> No WebSocket surface on this host — `wscat` is N/A. All traffic is plain
> HTTP GET for a PNG.

---

## 4. Real-client smoke (dcl-bevy / dcl-walk)

The map-chunk PNGs are the parcel atlas tiles rendered in the in-client map UI.
Use the upstream Unity client, since `ApiChunks` is a Unity-only enum.

1. **Repoint Unity** (section 1): edit line 187 to
   `"http://127.0.0.1:5143/v1/map.png"`, ensure `USE_GATEWAY` is off (or
   irrelevant since the URL has no `.decentraland.<env>` suffix). Rebuild via
   `dcl-editor build` if running the native editor.
2. **Start the service** (section 2) and confirm `curl` check (3) returns a PNG.
3. **Launch + open the map**:
   ```bash
   dcl-walk launch
   dcl-walk auth-sign
   ```
   Then open the in-world Map/Minimap UI (use your preferred headless client
   driver for navigation / OCR clicks).
4. **Observe**: the parcel atlas chunks should render the familiar atlas —
   Genesis Plaza green plaza ringed by gray roads, dark owned/estate parcels,
   stitched estate connectors — matching production. Capture a screenshot
   (`dcl-walk shot` / `dcl-rig shot`) and compare against a known-good atlas.
5. **Negative check**: stop `catalyrst-map`, pan the map, and confirm the
   chunks fail to load (loading sprite persists) — proving the client is
   actually hitting :5143 and not a stale cache or the real api host.
6. **Network confirmation** (optional): tail the service logs
   (`RUST_LOG=catalyrst_map=info`) while panning; each chunk pan should emit a
   `GET /v1/map.png?center=...` request with the Unity-shaped query params from
   `ParcelChunkController`.

Bevy/Godot note: bevy-explorer and godot-explorer use their own map sourcing and
do not share this Unity enum, so `dcl-bevy` is not the right smoke vehicle for
`ApiChunks` — prefer `dcl-walk` (Unity).

---

## 5. Pass criteria

- curl checks (1)-(11) pass as annotated; (12) returns 404 (expected, deferred).
- v1 and v2 byte-identical (4).
- Conditional GET returns 304 (8); cache headers present (7).
- Real-client map renders the correct atlas via :5143 (4.4) and breaks when the
  service is down (4.5).
