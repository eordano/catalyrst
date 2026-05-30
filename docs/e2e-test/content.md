# E2E test plan — catalyrst content (peer.decentraland.org port)

Key: `content` · Crate: `catalyrst-server` · Live binary: `catalyrst-live`
Workspace: `<WORKSPACE>` (e.g. `/path/to/catalyrst`)
Live port (shared-content-DB binary): **5141** (`CATALYRST_PORT`, default in `src/bin/live.rs:1688`)
Sync/source binary: `catalyrst-server` (`HTTP_SERVER_PORT`, default 5140, `src/main.rs:172`)

This service is a faithful Rust port of the catalyst content + lambdas + `/about` HTTP layer
(the public `peer.decentraland.org` surface). It reads a shared `content` Postgres DB
(`deployments`, `active_pointers`, `content_files`, `failed_deployments`). No new
tables, no migrations.

Route shapes (from `src/routes.rs`): content routes are mounted **both** at the top level
(`/contents/...`, `/entities/...`, `/status`, etc.) **and** nested under `/content/...`.
Lambdas routes carry the literal `/lambdas/...` prefix in their own paths. `/about` is top-level.
The Catalyst convention the Unity client uses is `/content/...` and `/lambdas/...`, so the e2e
checks below exercise the prefixed paths the real client hits.

---

## 1. Unity config — how to repoint each enum

Registry file: `Explorer/Assets/DCL/NetworkDefinitions/Browser/DecentralandUrlsSource.cs` (in the unity-explorer checkout)
Enum file:     `Explorer/Assets/DCL/Multiplayer/Connections/DecentralandUrls/DecentralandUrl.cs`

The relevant enums split into two groups: **hardcoded templates** (changed in Unity) and
**realm-discovered** (changed by editing OUR `/about` response, NOT Unity).

### Group A — hardcoded in `RawUrl(...)`, repoint in Unity

| Enum | Current `RawUrl` line | Repoint to |
|---|---|---|
| `PeerAbout` | `DecentralandUrlsSource.cs:188` → `$"https://peer.decentraland.{ENV}/about"` | `"http://127.0.0.1:5141/about"` |
| `Servers` | `DecentralandUrlsSource.cs:216` → `$"https://peer.decentraland.{ENV}/lambdas/contracts/servers"` | `"http://127.0.0.1:5141/lambdas/contracts/servers"` |

How to repoint: edit the right-hand string literal of each `switch` arm in
`RawUrl(DecentralandUrl decentralandUrl)`. Replace the `https://peer.decentraland.{ENV}` base
with `http://127.0.0.1:5141`. (Drop the `{ENV}` token entirely on these two lines; for a real
catalyst on the LAN use that host:port instead of loopback.) Both are `CacheBehaviour.STATIC`,
so the change takes effect on next client launch — no realm-change invalidation needed.

`PeerAbout` is the bootstrap hook: it is the realm `/about` the client fetches to discover the
rest of the content/lambdas surface. Repointing `PeerAbout` is the single most important edit;
everything in Group B follows from it.

### Group B — realm-discovered from `/about` (do NOT edit Unity; edit our `/about` response)

These five enums resolve to `UrlData.RealmDependent(realmData.Ipfs.*)` and are **not** literal
URLs — they are computed from the `/about` JSON we serve. Confirmed in
`Explorer/Assets/DCL/NetworkDefinitions/IpfsRealm.cs:35-41`: `ContentBaseUrl`,
`LambdasBaseUrl`, `EntitiesBaseUrl`, `EntitiesActiveEndpoint` are all built from
`serverAbout.content.publicUrl` and `serverAbout.lambdas.publicUrl`.

| Enum | `RawUrl` line | Source of truth |
|---|---|---|
| `Content` | `DecentralandUrlsSource.cs:253` `realmData.Ipfs.ContentBaseUrl` | our `/about` → `content.publicUrl` |
| `Lambdas` | `DecentralandUrlsSource.cs:252` `realmData.Ipfs.LambdasBaseUrl` | our `/about` → `lambdas.publicUrl` |
| `EntitiesDeployment` | `DecentralandUrlsSource.cs:251` `realmData.Ipfs.EntitiesBaseUrl` | our `/about` → `content.publicUrl` + `entities/` |
| `EntitiesActive` | `DecentralandUrlsSource.cs:242-243` `realmData.Ipfs.EntitiesActiveEndpoint` | our `/about` → `content.publicUrl` + `entities/active` (unless `ASSET_BUNDLE_FALLBACK` FF is on, which reroutes to AB-Registry) |
| `WorldEntitiesActive` | `DecentralandUrlsSource.cs:248-249` same endpoint (worlds) | our `/about` → `content.publicUrl` (unless AB-Registry fallback FF) |

To repoint Group B: do **nothing in Unity**. Make our `/about` (served at
`handlers::about::get_about`, `src/handlers/about.rs`) return `content.publicUrl` and
`lambdas.publicUrl` pointing at our host (e.g. `http://127.0.0.1:5141/content` and
`http://127.0.0.1:5141/lambdas`). Those come from `state.content_public_url` /
`state.lambdas_public_url` in `AppState` — set them via the server's public-URL env/config
when launching the live binary. The client will follow whatever we advertise there.

Caveat: `EntitiesActive` / `WorldEntitiesActive` are overridden to the asset-bundle registry
when the `ASSET_BUNDLE_FALLBACK` feature flag is enabled and not in local-scene-dev mode
(`DecentralandUrlsSource.cs:242,248`). For a clean content e2e, disable that FF or accept that
active-entity fetches for wearables/emotes go to AB-Registry, not our host.

### Out-of-scope enums (point at AB-Registry, not peer — do not touch for this lane)

`Profiles` (`:238`) and `ProfilesMetadata` (`:239`) resolve to
`{AssetBundleRegistry}/profiles[/metadata]`, and `EntitiesActiveElements` (`:246`) to
`{AssetBundleRegistry}/entities/active`. These belong to the asset-bundle-registry host, not
`peer.decentraland.org`. Our service does implement `/lambdas/profiles` and
`POST /content/entities/active`, but the Unity client reaches them through the AB-Registry
host for these specific enums. Leave them unless you are also repointing AB-Registry.

---

## 2. E2E curl/wscat checks (against `http://127.0.0.1:5141`)

Assumes the live binary is up against the shared content DB:
`cd <WORKSPACE>/crates/catalyrst-server && cargo run --bin catalyrst-live`
(set `POSTGRES_*` to point at your content DB; `CATALYRST_PORT=5141`).

Pick a known-good `$HASH` and `$ENTITY_ID` from the DB first (adjust connection flags for your
PostgreSQL instance):
`psql -h <SOCKET_DIR> -p 5433 -U <DB_USER> content -tAc "SELECT entity_id FROM deployments WHERE deleter_deployment IS NULL LIMIT 1"`

Each check below lists the command + expected status/shape.

1. `curl -fsS -w '\n%{http_code}\n' http://127.0.0.1:5141/about` — 200, JSON with `healthy`, `content.publicUrl`, `lambdas.publicUrl`, `configurations.networkId`, `configurations.scenesUrn`/`globalScenesUrn` arrays, `comms`, `bff`. (503 if content not "Syncing" or comms probe down — that is the documented unhealthy path.)
2. `curl -fsS -w '\n%{http_code}\n' http://127.0.0.1:5141/content/status` — 200, JSON content-server status (sync state, version, commitHash).
3. `curl -fsS -w '\n%{http_code}\n' http://127.0.0.1:5141/lambdas/status` — 200, JSON lambdas status.
4. `curl -fsS -w '\n%{http_code}\n' http://127.0.0.1:5141/lambdas/contracts/servers` — 200, JSON array of catalyst servers (the `Servers` enum payload).
5. `curl -fsS -w '\n%{http_code}\n' http://127.0.0.1:5141/lambdas/contracts/pois` — 200, JSON array of POI pointers.
6. `curl -fsS -w '\n%{http_code}\n' http://127.0.0.1:5141/lambdas/contracts/denylisted-names` — 200, JSON array.
7. `curl -fsS -w '\n%{http_code}\n' -X POST -H 'content-type: application/json' -d '{"pointers":["0,0"]}' http://127.0.0.1:5141/content/entities/active` — 200, JSON array of active entities for pointer 0,0 (genesis plaza scene); empty array if none, never 5xx.
8. `curl -fsS -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5141/content/contents/$HASH` — 200 with body bytes for a real hash; `curl ... /content/contents/bafyDOESNOTEXIST` — 404.
9. `curl -fsS -I -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5141/content/contents/$HASH` — 200 (HEAD), with `content-length` header and no body.
10. `curl -fsS -X POST -H 'content-type: application/json' -d '{"cids":["'$HASH'","bafyMISSING"]}' http://127.0.0.1:5141/content/available-content` — 200, JSON array of `{cid, available}` with the real hash `available:true`, the bogus one `false`. (Confirm exact param name against `handlers/get_available_content.rs` if it 400s.)
11. `curl -fsS -w '\n%{http_code}\n' "http://127.0.0.1:5141/content/audit/scene/$ENTITY_ID"` — 200, audit JSON for a real scene entity; 404 for a bogus id.
12. `curl -fsS -w '\n%{http_code}\n' "http://127.0.0.1:5141/content/deployments?limit=1"` — 200, paginated `{deployments:[...], pagination:{...}}`.
13. `curl -fsS -w '\n%{http_code}\n' "http://127.0.0.1:5141/content/pointer-changes?entityType=scene&limit=1"` — 200, `{deltas:[...], pagination:{...}}`.
14. `curl -fsS -w '\n%{http_code}\n' http://127.0.0.1:5141/content/snapshots` — 200, JSON (snapshot manifest / array).
15. `curl -fsS -w '\n%{http_code}\n' http://127.0.0.1:5141/content/failed-deployments` — 200, JSON array (likely empty locally).
16. `curl -fsS -w '\n%{http_code}\n' http://127.0.0.1:5141/content/challenge` — 200, `{challengeText:"..."}`.
17. `curl -fsS -w '\n%{http_code}\n' "http://127.0.0.1:5141/content/entities/scene?pointer=0,0"` — 200, JSON array of scene entities at 0,0.
18. `curl -fsS -w '\n%{http_code}\n' "http://127.0.0.1:5141/content/entities/active/collections/urn:decentraland:off-chain:base-avatars"` — 200, JSON list of active entities in that collection urn.
19. `curl -fsS -o /dev/null -w '%{http_code}\n' "http://127.0.0.1:5141/content/contents/$HASH/active-entities"` — 200, JSON array of entity ids referencing that content hash.
20. `curl -fsS -w '\n%{http_code}\n' "http://127.0.0.1:5141/lambdas/profiles/0x0000000000000000000000000000000000000001"` — 200 or empty profile object for an arbitrary address (never 5xx).
21. `curl -fsS -w '\n%{http_code}\n' -X POST -H 'content-type: application/json' -d '{"ids":["0x0000000000000000000000000000000000000001"]}' http://127.0.0.1:5141/lambdas/profiles` — 200, JSON array of profiles.
22. `curl -fsS -o /dev/null -w '%{http_code}\n' "http://127.0.0.1:5141/content/queries/items/urn:decentraland:off-chain:base-avatars:eyebrows_00/thumbnail"` — 200 image bytes (or 404 if that urn has no thumbnail); never 5xx.
23. `curl -fsS -o /dev/null -w '%{http_code}\n' "http://127.0.0.1:5141/content/queries/items/urn:decentraland:off-chain:base-avatars:eyebrows_00/image"` — 200 image bytes or 404.
24. `curl -fsS -o /dev/null -w '%{http_code}\n' "http://127.0.0.1:5141/content/queries/erc721/1/0x32b7495895264ac9d0b12d32afd435453458b1c6/0/0"` — 200 ERC-721 metadata JSON (wearable), or 404 for an unknown contract.
25. CORS preflight: `curl -fsS -i -X OPTIONS -H 'Origin: https://x' -H 'Access-Control-Request-Method: POST' http://127.0.0.1:5141/content/entities/active` — 2xx with `access-control-allow-origin` header (cors middleware in `src/cors.rs`).
26. Metrics: `curl -fsS -w '\n%{http_code}\n' http://127.0.0.1:5141/metrics` — 200, Prometheus text exposition.
27. Negative/fallback: `curl -fsS -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5141/nonsense` — 404 "Not found" (fallback handler).

Notes on tooling: every route here is HTTP; there is **no websocket endpoint** on this content
service, so `wscat` is not applicable (comms/archipelago lanes own the WS surface). The only
network egress this service makes is the `/about` comms probe to
`COMMS_WS_CONNECTOR_URL` and `COMMS_STATS_URL`; if those are down, `/about`
reports `comms.healthy:false` and returns 503 — that is expected, not a content-layer failure.

---

## 3. Real-client smoke step

The cleanest renderer for a headless content smoke is **bevy-explorer** (no GE-Proton, native,
fast to launch). Use `dcl-bevy`.

Steps:
1. Bring up the live content service on 5141 with the shared DB (above). Confirm check #1 returns 200 and that `/about`'s `content.publicUrl` / `lambdas.publicUrl` advertise `http://127.0.0.1:5141/content` and `.../lambdas` (Group B repointing).
2. If driving Unity instead, apply the Group A edit (`PeerAbout` → `http://127.0.0.1:5141/about`, `Servers` → `http://127.0.0.1:5141/lambdas/contracts/servers`) in `DecentralandUrlsSource.cs`, rebuild, and launch the Unity client pointed at the local realm.
3. Point the client at our realm. For bevy: `dcl-bevy up` then load the realm whose `/about` is `http://127.0.0.1:5141/about` (set the explorer's realm/server arg to `http://127.0.0.1:5141`). The client fetches `/about`, derives content/lambdas/entities-active from `publicUrl`, then `POST /content/entities/active` for the spawn parcel and `GET /content/contents/{hash}` for each scene asset.
4. Observe: avatar spawns at genesis plaza (0,0) with scene geometry loaded — proves `/about` discovery + `POST /content/entities/active` + `GET /content/contents/{hash}` all served from our host. Grab a screenshot (`dcl-bevy ... shot` / `dcl-rig shot`) and confirm the minimap renders (proves the `configurations.map` block in `/about`).
5. Cross-check the service logs / `/metrics` (check #26) to confirm the client's content+lambdas traffic landed on 5141 and not on production peer.

Pass criteria: scene loads with no missing-content errors, `/metrics` shows non-zero hits on
`/content/contents/*`, `/content/entities/active`, and `/about`, and no requests leaked to
`peer.decentraland.org`.
