# catalyrst-profile-images

Rust port of [`decentraland/profile-images`](https://github.com/decentraland/profile-images) (`profile-images.decentraland.org`) - serves 2D avatar face and body thumbnails on the HTTP contract the explorer clients consume.

The Unity explorer's `CatalyrstUrlsSource` does not rewrite `profile-images.decentraland.{ENV}` (only `GatewayUrlsSource` does), so avatar thumbnail requests leak to prod against a local catalyrst realm; this crate fills that gap. Since [ADR-290](https://adr.decentraland.org/adr/ADR-290), profile entities carry no `face256.png`/`body.png` content files; the lambdas `/profiles` endpoint points `snapshots.face256`/`snapshots.body` at this service, which renders on demand.

## The upstream contract

Upstream is a producer/consumer rig: a producer polls the content-server and enqueues render jobs on AWS SQS; a consumer runs a headless Godot avatar renderer (`decentraland.godot.client --avatar-renderer`) that rasterizes the equipped wearables to two PNGs (body `256x512`, face `256x256`) and uploads them to S3, fronted by CloudFront as `profile-images.decentraland.org`. The client-facing surface is the S3/CDN key shape (upstream's own HTTP server only exposes `/status` + an auth'd `/schedule-processing`). Clients GET:

```
GET /entities/{entityId}/face.png   -> 200 image/png   (256x256)
GET /entities/{entityId}/body.png   -> 200 image/png   (256x512)
                                       404 when not yet rendered
```

`{entityId}` is the profile entity id (an IPFS CID, `Qm...` v0 or `ba...` v1).

## What this crate implements

A local avatar-render pipeline with a content-addressed disk cache. The primary backend (`PROFILE_IMAGES_BACKEND=render`):

1. `GET /entities/{id}/{face,body}.png` serves from the disk cache if present;
2. on a miss, resolve the profile entity from the local content core - `GET <PROFILE_IMAGES_CONTENT_URL>/contents/{id}` -> `metadata.avatars[0].avatar`, as upstream's `scripts/local_entity_snapshot.sh` does;
3. render via the headless godot-explorer avatar renderer, the same invocation as upstream's `src/adapters/godot.ts`:

   ```text
   decentraland.godot.client.x86_64 \
     --rendering-method gl_compatibility --rendering-driver opengl3 \
     --avatar-renderer --avatars <avatars.json> [--dclenv <env>]
   ```

   with an `avatars.json` payload (`baseUrl` = the content core, one entry with `destPath`/`faceDestPath` + the `256x512`/`256x256` dims + the avatar);
4. cache both PNGs under the content-addressed layout and serve the one requested.

Entity ids are validated as canonical CIDs (no path traversal); responses carry `Content-Type: image/png`, `Cache-Control: public, max-age=86400`, and `X-Cache: HIT|RENDER|FALLBACK|MISS`; `/health` + `/health/live` return `"alive"`. Cache layout (sharded like the content-server's content store):

```
<PROFILE_IMAGES_CACHE_DIR>/<sha256(entityId)[0:2]>/<entityId>/{face,body}.png
```

### Single-flight (`src/queue.rs`)

`RenderQueue::render_once` keeps a per-entity-id map of in-flight renders: the first caller leads and renders; other callers for the same id park on a `broadcast` channel for the outcome. One render emits both face and body, so concurrent face+body requests collapse to one Godot invocation. A `Semaphore` caps concurrent Godot processes (`PROFILE_IMAGES_RENDER_MAX_CONCURRENT`, default 1); the leader re-checks the cache after acquiring its slot. Outcomes: `Rendered` / `NotFound` (no avatar -> 404) / `Failed(msg)` (-> 502 unless the proxy fallback is enabled).

### Backends

- **`render`** (primary) - requires `PROFILE_IMAGES_CONTENT_URL` and `PROFILE_IMAGES_GODOT_BIN`.
- **`proxy`** - origin-pull from `PROFILE_IMAGES_ORIGIN_URL` and cache; kept for hosts without a Godot build.
- **`disabled`** - cache-only; 404 on every miss (offline / tests).

Under `render`, the proxy is consulted only when `PROFILE_IMAGES_RENDER_FALLBACK_PROXY=true`; otherwise a render failure returns `502`.

### Prerequisite: the Godot client must be built

`nix run .#install-units` only builds this Rust crate. Build the exported godot-explorer client once from its checkout:

```bash
cd /path/to/godot-explorer
cargo run -- build -r              # build the Rust GDExtension (libdclgodot.so)
cargo run -- export --target linux # export -> exports/decentraland.godot.client.x86_64
```

(or use `scripts/local_profile_snapshot.sh <addr>` to build + smoke-test the renderer in one shot). Point `PROFILE_IMAGES_GODOT_BIN` at the resulting `exports/decentraland.godot.client.x86_64`. The renderer needs a GL context: a usable DRM render node (e.g. `/dev/dri/renderD128`) so the Compatibility renderer (`gl_compatibility`/`opengl3`) can draw. With `--headless` you must provide an Xvfb / EGL-surfaceless display (`PROFILE_IMAGES_GODOT_DISPLAY`); headless is off by default for that reason. For testnet (Amoy) wearables set `PROFILE_IMAGES_DCLENV=zone` - the content base alone does not redirect the renderer's wearable lookups (godot-explorer `docs/PROFILE_IMAGE.md`, "Catalyst environment").

## Config

See `deploy/catalyrst-profile-images.env.example`. Key knobs:

| Var | Default | Meaning |
|---|---|---|
| `HTTP_SERVER_PORT` | `5152` (code default; **do not use** - see note) | listen port |
| `PROFILE_IMAGES_BACKEND` | `render` if `CONTENT_URL` set, else `proxy`/`disabled` | `render` \| `proxy` \| `disabled` |
| `PROFILE_IMAGES_CONTENT_URL` | - | local content base, e.g. `http://127.0.0.1:5141/content` (required for `render`) |
| `PROFILE_IMAGES_GODOT_BIN` | - | path to `decentraland.godot.client.x86_64` (required for `render`) |
| `PROFILE_IMAGES_GODOT_PROJECT` | `<bin>/../..` | godot project root to spawn from |
| `PROFILE_IMAGES_RENDERING_METHOD` / `_DRIVER` | `gl_compatibility` / `opengl3` | godot render flags |
| `PROFILE_IMAGES_DCLENV` | - | `org`\|`zone`\|`today` wearable env (testnet needs `zone`) |
| `PROFILE_IMAGES_GODOT_HEADLESS` | `false` | pass `--headless` (needs Xvfb/EGL) |
| `PROFILE_IMAGES_RENDER_TIMEOUT_SECONDS` | `120` | per-render wall-clock timeout |
| `PROFILE_IMAGES_RENDER_MAX_CONCURRENT` | `1` | max concurrent godot processes |
| `PROFILE_IMAGES_RENDER_FALLBACK_PROXY` | `false` | on render failure, fall back to proxy (else 502) |
| `PROFILE_IMAGES_ORIGIN_URL` | - | prod base (required for `proxy` and for the fallback) |
| `PROFILE_IMAGES_CACHE_DIR` | `<DATA_DIR>/profile-images` | disk cache root |
| `PROFILE_IMAGES_CACHE_TTL_SECONDS` | `86400` | re-render/re-pull after this age; `0` = never expire |

> **Port collision note:** no `umbrella/env/catalyrst-profile-images.env` exists; the crate is not currently deployed. The code default `5152` is live-bound by `catalyrst-presence` (`umbrella/env/catalyrst-presence.env`); the repo's own `deploy/catalyrst-profile-images.env.example` overrides to `HTTP_SERVER_PORT=8080` for that reason. Pick an unused port from the deployment's port map when wiring this service into `umbrella/`.

## Client hand-off (unity-explorer `CatalyrstUrlsSource`)

Add to the `CatalyrstUrlsSource` SERVICES map (an nginx/gateway rewrite of the prod host to the local bundle), using the actually deployed port (not `5152`, owned by `catalyrst-presence`):

```csharp
{ "profile-images", ("/profile-images", <deployed-port>) },
```

so `https://profile-images.decentraland.org/entities/{id}/face.png` resolves to `http://<catalyrst-host>/profile-images/entities/{id}/face.png`, which proxies to this service. See the integration note in the deploy runbook.
