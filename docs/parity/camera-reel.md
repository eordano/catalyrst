# Parity report — camera-reel (`catalyrst-camera-reel` vs `decentraland/camera-reel-service`)

Upstream is itself a Rust (actix-web) service; our port is axum over a disk-backed
`catalyrst-storage::ContentStorage` (content-addressed, IPFS-style hashing) instead of an
external S3/CDN bucket. All other surfaces are verbatim ports.

Verification method: read both the upstream handler/struct and our handler/DTO for the
flagged endpoint, then cross-checked the Unity net-catalog
(`the Unity net-catalog`) for what the client actually reads.

## Per-endpoint summary

| Endpoint | Shape | Efficiency | Severity | Notes |
|---|---|---|---|---|
| `POST /api/images` | match | same | none | `UploadResponse` verbatim; same 1 COUNT + 1 INSERT + image/thumbnail write. URL tail differs (hash vs uuid-filename) but opaque to client. |
| `GET /api/images/{image_id}` | divergent | worse | minor | Upstream 302→bucket (no I/O); we 200 with raw bytes from disk. Client-transparent (see below). |
| `DELETE /api/images/{image_id}` | match | same | none | `UserDataResponse` identical. Behavioral-only divergence: upstream 500s on a failed bucket delete; we treat blob deletes as best-effort. Happy path identical. |
| `PATCH /api/images/{image_id}/visibility` | match | same | none | snake_case `{is_public}` on both; bare 200; early-return-on-unchanged on both. |
| `GET /api/images/{image_id}/metadata` | match | same | none | `Image` verbatim; identical error discrimination (ColumnDecode→500, else→404). 1 SELECT. |
| `GET /api/users/{user_address}` | match | same | none | `UserDataResponse`; identical only-public auth. 1 COUNT. |
| `GET /api/users/{user_address}/images` | match | same | none | compact/full responses verbatim; offset/limit window, ORDER BY created_at DESC. 2 SQL. |
| `GET /api/places/{place_id}/images` | match | same | none | `.eth` branch uses moka-cached PlacesClient; same 2 SQL. |
| `POST /api/places/images` | match | same | none | `{placesIds:[]}` camelCase; sequential moka-cached fan-out + 2 SQL, same as upstream. |

## Confirmed shape issues

### `GET /api/images/{image_id}` — transport divergence (minor)

- **Upstream** (`src/api/get.rs:26-28`): `get_image` is literally
  `Redirect::to(format!("{}/{}", settings.bucket_url, image_id))` — an HTTP 302 to the public
  bucket URL, no body, no storage access. `image_id` is the S3 object name.
- **Ours** (`src/handlers/images.rs:247-271`): `state.store.retrieve(&image_id)` (1 disk read via
  `ContentStorage`, `src/ports/storage.rs:36-38`), sniffs format
  (`guess_format` → `image/png` / `image/jpeg` / `application/octet-stream`), returns 200 with the
  raw bytes inline. `image_id` is the content hash.
- **Neither side has a JSON body.** The only difference is the HTTP transport (302 vs 200+bytes)
  and the `image_id` semantics (S3 object name vs content hash) — but both are opaque path tails the
  client received from the upload/metadata response.

This is a true `divergent` shape verdict, but the client-observable impact is **nil**: the
net-catalog records this call as a plain `GET` on an opaque "reel image URL" / "reel url passed in
— `/api/images/<filename>`", with no body shape — the client just fetches the URL it was handed and
treats the result as image bytes (texture). Standard HTTP clients (Unity `UnityWebRequest`,
browsers) auto-follow 302, so the client receives image bytes either way. Confirmed real, kept at
**minor** severity because of zero functional impact on the explorer.

## Confirmed efficiency findings

### `GET /api/images/{image_id}` — structurally `worse` (minor)

Verified by reading both implementations:

- Upstream does **zero storage I/O** per request — it formats a redirect string; the client then
  GETs the public bucket (CDN) directly. Image-serving bandwidth lives on the bucket/CDN.
- We do **1 disk read** from `ContentStorage` and stream the bytes through the service process,
  putting image-serving bandwidth on the app server and routing all image traffic through our node.

This is a genuine structural difference (extra work + bandwidth routing), not a language artifact,
and is the deliberate consequence of having no external bucket in this deployment. It is the only
non-`same` efficiency verdict in the lane.

No `better` efficiency claims were made in this lane (correctly — there is no place where we do
structurally less work than upstream).

## Rejected during verification

Nothing rejected. The single flagged finding (`GET /api/images/{image_id}`, divergent/worse/minor)
survived adversarial verification against both source files and the net-catalog. The remaining
endpoints' `match`/`same` verdicts were spot-checked against `src/api/get.rs` and were consistent.
