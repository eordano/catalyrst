# Verification: catalyrst-camera-reel (service "camera-reel")

Adversarial re-check of the prior finding for the camera-reel lane, committed tree
`feat/service-plane-crates`.

- Our crate: `crates/catalyrst-camera-reel`
- Upstream: `decentraland/camera-reel-service` (Rust/actix, **not** TS)
- Unity consumer: `decentraland/unity-explorer/Explorer/Assets/DCL/InWorldCamera/CameraReelStorageService/`
- Bundle: create (5144), `crates/catalyrst-create/src/main.rs` mounts `camera-reel` via `api_router()`.

## Verdict on the flagged finding

The single flagged item — **`GET /api/docs/ui/*` and `/api/docs/openapi.json` MISSING** —
is **CONFIRMED REAL but cosmetic / non-issue**. Verified, not rejected:

- (a) Divergence is real on the committed tree. `lib.rs:68 api_router()` registers 9
  functional routes nested under `/api`; there is **no** SwaggerUi route. Upstream
  `src/api/docs.rs:53-54` builds `SwaggerUi::new("/api/docs/ui/{_:.*}").url("/api/docs/openapi.json", ...)`
  and `src/api.rs:35-37` mounts it via `config.service(docs)`. So upstream serves docs; we don't.
- (b) Never client-called. The Unity net-catalog (`the Unity net-catalog`,
  `endpoints` table) has **zero** rows touching any `/api/docs` or `openapi` path for the
  camera-reel domain. Swagger UI is a human-facing dev surface; the Unity client never fetches it.
- (c) Failure mode is benign. `main.rs` (standalone) and `catalyrst-create/src/main.rs` (bundle)
  register **no `fallback`** route, so a request to `/api/docs/*` hits axum's default 404
  (empty body) instead of upstream's 200 Swagger HTML. No panic, no 500.

Severity: **none**. Prior finding's `shape_verdict=divergent`, `client_reaction=ok` are accurate.

## Per-endpoint table

| Endpoint | Shape vs upstream | Client reaction | Severity | Failure-modes-ok | Notes |
|---|---|---|---|---|---|
| GET /api/docs/ui/*, /api/docs/openapi.json | divergent (missing in ours) | ok (never called) | none | yes | axum 404 empty body vs upstream 200 swagger. Not in net-catalog. Cosmetic. |
| POST /api/images (multipart upload) | match | ok | none | yes | 403 MaxLimitReached `{reason:"maxLimitReached",message}` == upstream `ForbiddenError`. 200 `UploadResponse{image,currentImages,maxImages}` flattened. |
| GET /api/images/{id} | match (intended storage swap) | ok | none | yes | BUCKET_URL set -> 307 redirect; unset -> 200 bytes from local store, 404 if absent. Client follows redirects / consumes bytes equally. |
| DELETE /api/images/{id} | match | ok | none | yes | 200 `UserDataResponse{currentImages,maxImages}`; 403 forbidden on non-owner; 404 not found. |
| PATCH /api/images/{id}/visibility | match | ok | none | yes | Body `{is_public:bool}` (snake_case, no rename) == Unity body. 200 empty; client uses `WithNoOpAsync`. |
| GET /api/images/{id}/metadata | match | ok | none | yes | 200 `Image{id,url,thumbnailUrl,isPublic,metadata}`. ColumnDecode -> 500 "couldn't decode image" (== upstream); else 404. |
| GET /api/users/{addr} | match | ok | none | yes | 200 `UserDataResponse{currentImages,maxImages}`; DB error -> 404 "user not found" (== upstream). |
| GET /api/users/{addr}/images | match | ok | none | yes | compact -> `GetGalleryImagesResponse`, else `GetImagesResponse`. Both 200 (see 210 note). |
| GET /api/places/{placeId}/images | match | ok | none | yes | 200 `GetPlaceImagesResponse{images,maxImages}` (no currentImages, == upstream). |
| POST /api/places/images | match | ok | none | yes | Body `{placesIds:[...]}` (camelCase) == Unity. 200 `GetMultiplePlacesImagesResponse`. |

## Confirmed issues

None beyond the cosmetic docs gap above. The functional surface is a faithful port.

Re-verified against the Unity consumer (`CameraReelImagesMetadataRemoteDatabase.cs`) and DTOs
(`Schemas/CameraReelResponses.cs`):

- Response casing matches: `dto.rs` uses `#[serde(rename_all="camelCase")]` everywhere the C#
  DTO expects camelCase (`thumbnailUrl`, `isPublic`, `currentImages`, `maxImages`, `dateTime`),
  and snake_case exactly where the C# request body uses it (`is_public`; Unity sends `placesIds`
  which maps to our camelCase `places_ids`).
- Error model matches upstream exactly: `http.rs` `ResponseError{message}` for all variants
  except `MaxLimitReached` -> 403 `ForbiddenError{reason:"maxLimitReached",message}`. Upstream
  `api.rs:158-196` is identical in shape. Unity's `CameraReelErrorResponse{message,reason}` maps
  onto both. Status map (400/401/403/403/404/502/500/500) == upstream.

## Client-crash risks

None. The Unity client parses every response via `CreateFromJson<T>(WRJsonParser.Unity)`
(JsonUtility). JsonUtility silently leaves missing fields at default (e.g. place responses omit
`currentImages`, so the client's field stays 0 — same as against real upstream, which also omits
it). There are **no non-null assertions** on the deserialized DTOs in the converter; values flow
into plain serializable classes. The only client failure axis is the HTTP status: `CreateFromJson`
throws on non-2xx, and we return the same status codes as upstream for every modeled error, so no
spurious throw is introduced.

## Failure-mode gaps

None confirmed. Every error path re-read:

- Startup panic-free on optional deps. `Config::from_env` (config.rs:28) makes
  `CAMERA_REEL_PG_CONNECTION_STRING` required -> missing yields `anyhow::Err` -> clean process
  exit, no panic. `build_state` (lib.rs:31-48) connects Postgres and runs `sqlx::migrate!` at boot;
  DB unreachable -> `anyhow::Err` -> clean exit (parity: upstream also requires the DB). `BUCKET_URL`
  is `.ok().filter(non-empty)` (config.rs:33) so blank == unset. No S3/SNS/LiveKit deps (upstream
  needs S3+SNS; we substitute local disk + drop SNS by design).
- `PlacesClient::new` (ports/places.rs:45-48) has one `expect("failed to build reqwest client")`;
  `reqwest::Client::builder().build()` only errors on TLS backend init, not configured here, so it
  does not fail in practice. PlacesClient is only invoked on `.eth` world resolution.
- **210-vs-200 trap (checked, parity holds):** upstream's OpenAPI annotation documents status
  **210** for the compact gallery response (`get.rs:154`), but the runtime handler returns
  `HttpResponse::Ok()` = **200** (`get.rs:211`). Our crate returns 200 for compact
  (`handlers/users.rs:84`). This matches real upstream runtime behavior, **not** a divergence — the
  210 is doc-only and would in fact have broken the client's `CreateFromJson` (non-2xx throw) had
  upstream actually emitted it.

## Summary

10 endpoints checked. The lone flagged item (missing `/api/docs` Swagger UI) is real but cosmetic:
not in the Unity net-catalog, benign 404 fallback, severity none — verdict upheld. All 9 functional
endpoints are faithful ports with matching field casing, error shapes (`{message}` /
`{reason:"maxLimitReached",message}`), status codes, and the compact 200-not-210 behavior. No
client-crash risk (JsonUtility no-throws on missing fields; status codes preserved). No failure-mode
gaps; startup is panic-free with only Postgres + a writable content dir required.
