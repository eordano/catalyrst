# E2E Test Plan — `assets-cdn` (community thumbnail origin)

Reimplementation of `assets-cdn.decentraland.org` for the community-thumbnail
surface, served by **`catalyrst-communities`** on its assigned port (`5136`).

- Crate: `catalyrst-communities` (extended; not a new crate/port)
- Workspace: `<WORKSPACE>`
- Route shipped: `GET /social/communities/{id}/raw-thumbnail.png`
- Route deferred: thumbnail INGEST (write path) — behind the federation signed-write
  path (501), like other communities writes. Serve route 404s until bytes exist on disk.
- Storage: filesystem under `{COMMUNITIES_CONTENT_DIR}/thumbnails/{id}/raw-thumbnail.png`
  (default `COMMUNITIES_CONTENT_DIR=<CONTENT_ROOT>/communities/content`).
  `{id}` must be a strict UUID (`is_valid_community_id`), bytes must start with the
  PNG magic signature, capped at 256 KiB.

---

## 1. Unity config — how to repoint this host

The thumbnail URL is **NOT realm-discovered** (`/about` plays no part here). It is a
**hardcoded template** in the Unity URL registry, so it is changed in Unity, not in
our `/about` response.

**File:** `Explorer/Assets/DCL/NetworkDefinitions/Browser/DecentralandUrlsSource.cs`
(in the upstream `unity-explorer` checkout)

**Line 222** (inside the `RawUrl(DecentralandUrl)` switch):

```csharp
DecentralandUrl.CommunityThumbnail => $"https://assets-cdn.decentraland.{ENV}/social/communities/{{0}}/raw-thumbnail.png",
```

`{ENV}` is the literal `"{ENV}"` placeholder (const at line 33) that
`Url()`/`Probe()` later replace with the realm domain (`org` / `zone`). `{0}` is the
community id. The enum member is `DecentralandUrl.CommunityThumbnail`
(defined in `DCL/NetworkDefinitions/Browser/DecentralandUrls/DecentralandUrl.cs`).

**To repoint to your local service**, replace the template so it resolves to
`http://127.0.0.1:5136`:

```csharp
DecentralandUrl.CommunityThumbnail => $"http://127.0.0.1:5136/social/communities/{{0}}/raw-thumbnail.png",
```

Notes / gotchas:
- Drop `{ENV}` from this one line (we are a single fixed host, not env-templated). The
  `.Replace(ENV, decentralandDomain)` in `Url()`/`Probe()` becomes a no-op since the
  literal `{ENV}` no longer appears.
- **Gateway interception:** `CommunityThumbnail` is also listed in
  `GatewayUrlsSource.SUPPORTED_URLS` (line 64 of
  `Explorer/Assets/DCL/NetworkDefinitions/Browser/GatewayUrlsSource.cs`). When the
  `USE_GATEWAY` feature flag is on AND the env is `Org`/`Zone`, `RawUrl` rewrites the
  URL to `https://gateway.decentraland.{env}/assets-cdn/...`. A `127.0.0.1` URL has no
  `subdomain.domain` shape, so `TransformToGateway` will mangle it. For local testing,
  either keep `USE_GATEWAY` disabled, or remove `DecentralandUrl.CommunityThumbnail`
  from `SUPPORTED_URLS` so the raw `127.0.0.1` URL passes through untouched.
- This route is unauthenticated (listed in `GatewayUrlsSource` above the signed-fetch
  block), matching the impl's no-AuthChain handler.

---

## 2. Service-level e2e checks (curl against `127.0.0.1:5136`)

### Prereqs / fixtures

```bash
# Start the service (reads its environment file and binds 5136)
systemctl --user start catalyrst-communities.service
systemctl --user is-active catalyrst-communities.service

# Resolve content dir (defaults shown)
CONTENT_DIR=<CONTENT_ROOT>/communities/content
ID=123e4567-e89b-12d3-a456-426614174000   # valid UUID fixture

# Seed a thumbnail on disk (INGEST route is deferred/501, so seed directly).
# A valid PNG is required (handler/store enforce the PNG magic-byte signature).
mkdir -p "$CONTENT_DIR/thumbnails/$ID"
printf '\x89PNG\r\n\x1a\n' > /tmp/seed_head.png
# Generate a real 1x1 PNG if available, else the magic header above is enough for serve
( command -v convert >/dev/null && convert -size 1x1 xc:white "$CONTENT_DIR/thumbnails/$ID/raw-thumbnail.png" ) \
  || cp /tmp/seed_head.png "$CONTENT_DIR/thumbnails/$ID/raw-thumbnail.png"
```

### Check A — served thumbnail returns 200 image/png with cache headers

```bash
curl -sS -D - -o /tmp/thumb.png \
  http://127.0.0.1:5136/social/communities/123e4567-e89b-12d3-a456-426614174000/raw-thumbnail.png
```
Expected:
- `HTTP/1.1 200 OK`
- `content-type: image/png`
- `cache-control: public, max-age=600`
- `content-length` equal to the file size; body is the raw PNG bytes (first 8 bytes = PNG magic).

### Check B — missing thumbnail returns 404 (client falls back to default)

```bash
curl -sS -o /dev/null -w '%{http_code}\n' \
  http://127.0.0.1:5136/social/communities/00000000-0000-0000-0000-000000000000/raw-thumbnail.png
```
Expected: `404`. (Valid UUID, no bytes on disk → client uses
`ChatConfig.DefaultCommunityThumbnail`.)

### Check C — path-traversal / invalid id rejected as 404 (not 400, not a file leak)

```bash
curl -sS -o /dev/null -w '%{http_code}\n' \
  'http://127.0.0.1:5136/social/communities/not-a-uuid/raw-thumbnail.png'
curl -sS -o /dev/null -w '%{http_code}\n' \
  --path-as-is 'http://127.0.0.1:5136/social/communities/..%2f..%2fetc%2fpasswd/raw-thumbnail.png'
```
Expected: `404` for both (UUID validation rejects before touching the filesystem; no
`/etc/passwd` content ever returned).

### Check D — ingest is deferred (write path not yet live)

There is no public ingest route yet (deferred behind federation signed-write). Confirm
no unauthenticated PUT/POST writes a thumbnail:

```bash
curl -sS -o /dev/null -w '%{http_code}\n' -X PUT \
  --data-binary @"$CONTENT_DIR/thumbnails/$ID/raw-thumbnail.png" \
  -H 'content-type: image/png' \
  http://127.0.0.1:5136/social/communities/123e4567-e89b-12d3-a456-426614174000/raw-thumbnail.png
```
Expected: `404` or `405` (no write route mounted) — and crucially **never** `200`/`201`.
When the federation write path lands, this should become a signed-write `501`→`200`.

### Check E (optional, when write path lands) — non-PNG upload rejected

Once ingest is implemented, posting a non-PNG body must be rejected by the PNG
magic-byte check so a served `image/png` never lies. Expected: `400`/`415`/`422`
(not `200`), and nothing written under `thumbnails/{id}/`.

---

## 3. Real-client smoke

The thumbnail surface is consumed by the **Unity** client (community voice-chat
titlebar / chat community avatars via `CommunityThumbnail` → `ImageView`). Bevy/Godot
do not implement the communities thumbnail UI, so the meaningful client smoke is
through the upstream Unity refclient.

1. Apply the Unity repoint from §1 (and disable `USE_GATEWAY` or drop
   `CommunityThumbnail` from `SUPPORTED_URLS`).
2. Launch the upstream Unity client headlessly using your client-automation
   tooling (authenticate and sign in as usual).
3. Open a community surface that renders a thumbnail (community voice-chat in-call
   titlebar / communities list). With a seeded thumbnail (Check A id), confirm the
   image renders; with an unseeded id, confirm the default thumbnail
   (`ChatConfig.DefaultCommunityThumbnail`) shows instead of a broken image.
4. Capture a screenshot and tail the client network log to
   verify requests hit `127.0.0.1:5136/social/communities/{id}/raw-thumbnail.png`
   (and were not rewritten to a `gateway.` host).

The Bevy/Godot client is **not applicable** for this lane (no community-thumbnail
consumer in bevy-explorer); note this in the run log rather than attempting a bevy smoke.

---

## 4. Pass criteria

- §2 Checks A–D pass exactly as specified (200+headers, 404, 404/404, no-write).
- §1 repoint resolves `CommunityThumbnail` to `127.0.0.1:5136` with no gateway rewrite.
- §3 Unity smoke shows seeded thumbnail renders and missing → default fallback, with
  traffic confirmed against the local port.
