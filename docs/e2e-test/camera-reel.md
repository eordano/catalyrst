# E2E test plan — catalyrst-camera-reel (key=camera-reel)

Reimplementation of `camera-reel-service.decentraland.org`.

- Crate: `catalyrst-camera-reel`
- Local port: `5143` (referred to below as `:5143`; examples use `5143`)
- Workspace: `<WORKSPACE>`
- Shared DB: `places_events` (new table `camera_reel_images`)
- Upstream: https://github.com/decentraland/camera-reel-service

---

## 1. Unity config to repoint

These URLs are **NOT** `/about`-discovered. The camera-reel host is a static
template baked directly into the `RawUrl(...)` switch in unity-explorer; the only
substitution is the `{ENV}` token, which is replaced by
`environment.ToString().ToLower()` (`org` / `zone` / `today`) in the
`DecentralandUrlsSource` constructor — **not** by any realm `/about` response.
So repointing is a Unity source edit, not an `/about` edit.

File: `Explorer/Assets/DCL/NetworkDefinitions/Browser/DecentralandUrlsSource.cs`
(in the unity-explorer clone). Exact lines (as of this writing):

```
203:  DecentralandUrl.CameraReelUsers  => $"https://camera-reel-service.decentraland.{ENV}/api/users",
204:  DecentralandUrl.CameraReelImages => $"https://camera-reel-service.decentraland.{ENV}/api/images",
205:  DecentralandUrl.CameraReelPlaces => $"https://camera-reel-service.decentraland.{ENV}/api/places",
```

(`CameraReelLink => https://reels.decentraland.{ENV}` on line 206 is the web
gallery deep-link, not a service endpoint — leave it.)

The three enum cases come from
`Explorer/Assets/DCL/NetworkDefinitions/Browser/DecentralandUrls/DecentralandUrl.cs`
(`CameraReelUsers`, `CameraReelImages`, `CameraReelPlaces`) — only the
`RawUrl` switch needs changing, the enum stays.

**Repoint:** replace the three RHS string templates so they resolve to our host
(default `http://127.0.0.1:5143`). Edit lines 203–205 to:

```csharp
DecentralandUrl.CameraReelUsers  => "http://127.0.0.1:5143/api/users",
DecentralandUrl.CameraReelImages => "http://127.0.0.1:5143/api/images",
DecentralandUrl.CameraReelPlaces => "http://127.0.0.1:5143/api/places",
```

Note our service's base is the single host; the client appends `/{image_id}`,
`/{user_address}`, `/{place_id}/images`, `/images` etc. to these three bases, so
the three roots above are sufficient. Keep the scheme `http` (not `https`) for the
loopback host. If you need TLS, front the service with a local reverse proxy and
point at that instead.

---

## 2. Local service e2e checks (curl / wscat)

Assumes the service is up on `:5143` with `CONTENT_STORAGE_DIR` writable, a
read/write role on `places_events`, the migration applied, and catalyrst-places
running on its configured port (for world `.eth` resolution). No WebSocket surface exists on
this service (camera-reel is pure REST + SNS no-op), so all checks are curl.

Auth: the 3 mutating routes require a signed-fetch auth chain over
`METHOD:uri.path()` via the `x-identity-auth-chain-0/1/2` headers
(`verify_auth_chain` in `src/auth_chain.rs`). The GET `/users/*` routes accept an
optional chain (private counts only when `signer == user_address`). For a real
signed chain in checks, generate it with an explorer identity helper or reuse a
captured chain; checks below mark where a valid chain is required vs. where its
absence must degrade gracefully.

### 2.0 Health
```bash
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5143/health/live
# expect: 200
```

### 2.1 GET /api/users/{user_address} — public counts, no auth
```bash
curl -s -w '\n%{http_code}\n' \
  http://127.0.0.1:5143/api/users/0x0000000000000000000000000000000000000001
# expect: 200; JSON UserDataResponse {currentImages, maxImages:500, ...}
#   (camelCase). No private fields exposed for an unauthenticated caller.
```

### 2.2 GET /api/users/{user_address}/images — list, paginated
```bash
curl -s -w '\n%{http_code}\n' \
  'http://127.0.0.1:5143/api/users/0x0000000000000000000000000000000000000001/images?offset=0&limit=20'
# expect: 200; GetImagesResponse { images:[...], currentImages, maxImages } camelCase.

curl -s -w '\n%{http_code}\n' \
  'http://127.0.0.1:5143/api/users/0x0000000000000000000000000000000000000001/images?compact=true'
# expect: 200; GetGalleryImagesResponse (compact shape) — confirm the response key
#   differs from compact=false.
```

### 2.3 POST /api/images — upload (requires valid signed chain)
```bash
# Replace the three header values with a chain signed over "POST:/api/images".
curl -s -w '\n%{http_code}\n' -X POST http://127.0.0.1:5143/api/images \
  -H "x-identity-auth-chain-0: $AC0" \
  -H "x-identity-auth-chain-1: $AC1" \
  -H "x-identity-auth-chain-2: $AC2" \
  -F 'image=@/tmp/shot.png;type=image/png' \
  -F 'metadata={"userName":"tester","dateTime":"1","scene":{"name":"x","location":{"x":0,"y":0}},"realm":"main"};type=application/json' \
  -F 'is_public=true'
# expect: 200; UploadResponse { image:{ id (UUID), url, thumbnailUrl, ... } }.
#   url/thumbnailUrl must be http://127.0.0.1:5143/api/images/<hash> (NOT a 302/S3 url).
# Capture image.id (UUID) and the hash from url for the checks below.
```

Auth-failure variant (no chain headers):
```bash
curl -s -o /dev/null -w '%{http_code}\n' -X POST http://127.0.0.1:5143/api/images \
  -F 'image=@/tmp/shot.png;type=image/png'
# expect: 401  (body {"message":"Unauthorized"})
```

### 2.4 GET /api/images/{hash} — streams bytes
```bash
curl -s -o /tmp/roundtrip.png -w '%{http_code} %{content_type}\n' \
  http://127.0.0.1:5143/api/images/<HASH-from-2.3>
# expect: 200, image/png (or jpeg); body bytes-identical to the upload (NOT a 302).
cmp /tmp/shot.png /tmp/roundtrip.png   # for png upload, should match
```

### 2.5 GET /api/images/{image_id}/metadata — by UUID
```bash
curl -s -w '\n%{http_code}\n' http://127.0.0.1:5143/api/images/<UUID-from-2.3>/metadata
# expect: 200; full Image DTO (camelCase).
curl -s -o /dev/null -w '%{http_code}\n' \
  http://127.0.0.1:5143/api/images/00000000-0000-0000-0000-000000000000/metadata
# expect: 404
```

### 2.6 PATCH /api/images/{image_id}/visibility — owner only, snake_case body
```bash
curl -s -w '\n%{http_code}\n' -X PATCH \
  http://127.0.0.1:5143/api/images/<UUID-from-2.3>/visibility \
  -H "x-identity-auth-chain-0: $AC0" -H "x-identity-auth-chain-1: $AC1" -H "x-identity-auth-chain-2: $AC2" \
  -H 'content-type: application/json' \
  -d '{"is_public": false}'
# expect: 200. Body key is snake_case is_public (NOT isPublic). Re-PATCH same value -> 200 no-op.
# Non-owner chain -> 401/403. Missing UUID -> 404.
```

### 2.7 GET /api/places/{place_id}/images
```bash
# Genesis-style place UUID:
curl -s -w '\n%{http_code}\n' \
  http://127.0.0.1:5143/api/places/<PLACE-UUID>/images
# expect: 200; GetPlaceImagesResponse (public images only).

# World name (.eth) resolves through catalyrst-places:
curl -s -w '\n%{http_code}\n' \
  http://127.0.0.1:5143/api/places/some-name.dcl.eth/images
# expect: 200 if places resolves it; 502 if the places lookup fails.
```

### 2.8 POST /api/places/images — multi, camelCase body
```bash
curl -s -w '\n%{http_code}\n' -X POST http://127.0.0.1:5143/api/places/images \
  -H 'content-type: application/json' \
  -d '{"placesIds":["<PLACE-UUID>"]}'
# expect: 200; GetMultiplePlacesImagesResponse. Body key is camelCase placesIds.
curl -s -o /dev/null -w '%{http_code}\n' -X POST http://127.0.0.1:5143/api/places/images \
  -H 'content-type: application/json' -d '{"placesIds":[]}'
# expect: 400  (empty list rejected)
```

### 2.9 DELETE /api/images/{image_id} — owner only
```bash
curl -s -w '\n%{http_code}\n' -X DELETE \
  http://127.0.0.1:5143/api/images/<UUID-from-2.3> \
  -H "x-identity-auth-chain-0: $AC0" -H "x-identity-auth-chain-1: $AC1" -H "x-identity-auth-chain-2: $AC2"
# expect: 200; UserDataResponse with currentImages decremented.
# Then GET /api/images/<HASH>/metadata -> 404 and the content files are gone from disk.
# Non-owner chain -> 401/403.
```

### 2.10 Quota edge (optional, slow)
Upload `MAX_IMAGES_PER_USER` (default 500) images for one signer, then one more:
```
# expect: 403 with ForbiddenError {"reason":"maxLimitReached","message":...}
```
For a fast check, run the service with `MAX_IMAGES_PER_USER=1` and upload twice.

---

## 3. Real-client smoke (upstream Unity)

The camera-reel UI lives in the Unity client (the in-world camera / reel gallery,
plus the place-info "Photos" tab). Bevy/Godot do not implement camera-reel, so the
client smoke runs against the upstream Unity client after applying the Unity
repoint from section 1.

1. Apply the section-1 edit (lines 203–205) in the unity-explorer clone the
   client builds from, then build/launch the client and establish an identity
   (the same identity helper used to mint the `x-identity-auth-chain-*` headers
   reused in section 2).
2. Confirm the client talks to your host, not prod: tail the camera-reel service
   log and watch for `GET /api/users/<wallet>` on connect. If the service runs as
   a systemd unit (`catalyrst-camera-reel.service`), follow it with
   `journalctl --user -u catalyrst-camera-reel -f`; otherwise run the binary
   directly and watch stdout.
3. Take a photo in-world (the CameraReel shortcut), then open the reel gallery.
   Expect: the upload POST hits `:5143/api/images` (201/200 in the log), the new
   thumbnail renders in the gallery (proves GET `/api/images/{hash}` streaming
   works against the Unity image loader, since we stream bytes rather than 302),
   and the user counter reflects the new image.
4. Toggle a photo public/private in the gallery -> PATCH `/visibility` 200 in log.
   Delete a photo -> DELETE 200 and it disappears from the gallery.
5. Open a place's info panel "Photos" tab (Navmap `PlaceInfoPanelController`) ->
   GET `/api/places/{place_id}/images` in the log, public shots render.

Pass criteria: every camera-reel network call in the Unity log targets
`127.0.0.1:5143` (never `camera-reel-service.decentraland.*`), and the gallery /
place-photos UI renders thumbnails sourced from our streamed `/api/images/{hash}`.

---

## 4. Pre-reqs / not-yet-wired (maintainer)

Per the impl summary, before any runtime check:
- Provision a **read/write** role on `places_events` (the events crate uses a
  read-only role; uploads/deletes need INSERT/UPDATE/DELETE) and create the
  service's environment file (`<ENV_FILE>`) from
  `crates/catalyrst-camera-reel/deploy/catalyrst-camera-reel.env.example`.
- Deploy the service the same way as catalyrst-places (e.g. behind a reverse
  proxy as a long-running process; a `catalyrst-camera-reel.service` systemd unit
  is provided under `deploy/`).
- Ensure `CONTENT_STORAGE_DIR` (default `<DATA_DIR>/camera-reel`) exists and is
  writable.
- Migration `migrations/20260609000000_camera_reel_images.sql` runs at startup
  via `sqlx::migrate!`; verify the `camera_reel_images` table + 4 indexes exist.
