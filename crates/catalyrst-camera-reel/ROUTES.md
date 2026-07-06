# catalyrst-camera-reel routes (port `5163`)

Rust port of decentraland/camera-reel-service. All under `/api`; CORS allow-any-origin, methods
GET/POST/PATCH/DELETE. Plus `GET /health/live`. Port `5163` (see `umbrella/env/catalyrst-camera-reel.env`,
which notes "5149 is catalyrst-sync" - `5143` belongs to `catalyrst-world-storage`, not this crate).

| Method | Path | Auth | Notes |
|---|---|---|---|
| POST | `/api/images` | signed-fetch (required) | multipart image+metadata(+is_public). 640x360 thumbnail, png/jpeg, 5MiB. UUID id, content-hash store. 403 `{reason:maxLimitReached}` at MAX_IMAGES_PER_USER. `UploadResponse`. |
| POST | `/api/images-json` | signed-fetch (required) | JSON-body variant of upload: base64-encoded image (`req.image`, <=15 MiB decoded) + metadata, same content-hash store as `/api/images` (`handlers::images::upload_image_json`). |
| DELETE | `/api/images/{image_id}` | signed-fetch (required) | owner-only by reel UUID. deletes row + content files. `UserDataResponse`. |
| PATCH | `/api/images/{image_id}/visibility` | signed-fetch (required) | body `{is_public}` snake_case. owner-only, 404 if missing, no-op 200 if unchanged. |
| GET | `/api/images/{image_id}` | none | `image_id` = content hash. Streams bytes from ContentStorage (upstream 302s to a bucket). |
| GET | `/api/images/{image_id}/metadata` | none | full `Image` by UUID. 404 missing, 500 decode error. |
| GET | `/api/users/{user_address}` | optional | `UserDataResponse`. private counts only when signer==user. |
| GET | `/api/users/{user_address}/images` | optional | `compact=false` -> `GetImagesResponse`; `compact=true` -> `GetGalleryImagesResponse`. offset/limit. |
| GET | `/api/places/{place_id}/images` | none | public images. `.eth` resolves via PlacesClient (502 on failure). else UUID. `GetPlaceImagesResponse`. |
| POST | `/api/places/images` | none | body `{placesIds}` camelCase. 400 if empty, 502 on world resolve. `GetMultiplePlacesImagesResponse`. |
| GET | `/docs/openapi.json` | none | generated OpenAPI schema (`docs::openapi_json`). |
| GET | `/docs/ui`, `/docs/ui/`, `/docs/ui/{*rest}` | none | Swagger UI (`docs::swagger_ui`), not part of upstream `camera-reel-service`. |

## Deviations from upstream

- File store: rust-s3 `Bucket` -> catalyrst-storage `ContentStorage` (sharded, content-addressed). Image/thumbnail keyed by dcl IPFS CIDv1 (catalyrst-hashing). `image.url`/`thumbnailUrl` = `{API_URL}/api/images/{hash}`; GET streams instead of redirecting.
- Auth: upstream `dcl-crypto-middleware-rs` -> catalyrst-crypto `verify_auth_chain` over `x-identity-auth-chain-*` headers (auth_chain.rs copied from catalyrst-communities).
- PlacesClient base_url -> the local catalyrst-places service (`http://127.0.0.1:5134`). moka TTL 300s / max 1000 preserved.
- SNS (camera/photoTaken, camera/photoPrivacyChanged): no-op. The explorer never observes it.
- DB: shared `places_events` DB, table `camera_reel_images` (distinct from any bare `images`).
