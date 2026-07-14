# Deploy - bundle runbook, explorer pointing, gateway mode

Nginx configs: [docs/deploy/](./deploy/).

## Bundle-stack bring-up runbook

One TLS host, six bundle binaries + content core, all loopback, path-routed by a reverse proxy ([deploy/nginx-catalyrst-bundles.conf](./deploy/nginx-catalyrst-bundles.conf)).

| Process | Default port | Members |
|---|---|---|
| catalyrst-live | 5141 (`HTTP_SERVER_PORT`) | content, lambdas, about |
| catalyrst-explore | 5143 | places, events, archipelago, worlds, map, lists |
| catalyrst-create | 5144 | builder, camera-reel |
| catalyrst-social | 5145 | communities, comms, notifications, badges, media |
| catalyrst-data | 5146 | market, economy, price, credits, rpc |
| abgen | 5147 | asset-bundle CDN + AB-registry + in-process converter (upstream decentraland/abgen) |
| catalyrst-social-rpc | 5148 | dcl-rpc WebSocket (friends/voice) |
| catalyrst-market | 5133 | standalone marketplace (optional; data already serves `/v1`) |

Bundles bind `BUNDLE_HTTP_PORT` on loopback; any port range works. Prereqs: PostgreSQL 18, squid marketplace DB populated, content DB synced, LiveKit SFU reachable.

**1. Build** - pin binaries at stable paths (symlink from `target/release/`):

```bash
for b in explore create social data abgen social-rpc; do
  cargo build --release --bin catalyrst-$b
done
cargo build --release --bin catalyrst-live
# NixOS: use an FHS/nix shell - see ./build-and-test.md
```

**2. Least-privilege DB roles** - `_ro` read-only, `_rw` owner (runs sqlx migrations at `build_state()`):

```bash
PSQL="psql -h <DB_HOST> -p <DB_PORT> -U <DB_ADMIN>"
$PSQL -c "CREATE ROLE cat_explore_ro LOGIN PASSWORD '...';"   # + cat_data_ro, cat_content_ro
$PSQL -c "CREATE ROLE cat_create_rw  LOGIN PASSWORD '...';"   # + cat_social_rw, cat_data_rw

# explore reads places_events + marketplace + content
for db in places_events marketplace content; do
  $PSQL -d $db -c "GRANT CONNECT ON DATABASE $db TO cat_explore_ro;"
  $PSQL -d $db -c "GRANT USAGE ON SCHEMA public TO cat_explore_ro;"
  $PSQL -d $db -c "GRANT SELECT ON ALL TABLES IN SCHEMA public TO cat_explore_ro;"
  $PSQL -d $db -c "ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT TO cat_explore_ro;"
done
# data reads marketplace schemas, owns credits
$PSQL -d marketplace -c "GRANT USAGE ON SCHEMA marketplace, favorites, squid_marketplace TO cat_data_ro;"
$PSQL -d marketplace -c "GRANT SELECT ON ALL TABLES IN SCHEMA marketplace, favorites, squid_marketplace TO cat_data_ro;"
$PSQL -c "CREATE DATABASE credits OWNER cat_data_rw;"
# abgen owns ab_registry (registry surface), reads content; social owns its four DBs
$PSQL -c "CREATE DATABASE ab_registry OWNER cat_create_rw;"
for db in communities comms_gatekeeper notifications badges; do
  $PSQL -c "CREATE DATABASE $db OWNER cat_social_rw;"
done
```

**3. Env files** - a bundle's env file concatenates its members'. Duplicate `HTTP_SERVER_PORT`/`HTTP_SERVER_HOST` keys are harmless (only `BUNDLE_HTTP_PORT` binds); check overlapping keys resolve as intended. `chmod 600` everything.

- `catalyrst-live` auto-loads `/etc/catalyrst/content.env` at boot.
- LiveKit: comms/worlds fail fast without `LIVEKIT_API_KEY`/`_SECRET` (`LIVEKIT_ALLOW_DEV_CREDS=1` for dev defaults); set key/secret/`LIVEKIT_HOST`/`LIVEKIT_WEBHOOK_KEY` identically across explore+social - one SFU, see [operations.md](./operations.md).
- Money endpoints (credits/market/economy) need an admin token to bind beyond loopback.
- Template units: `nixos/systemd/catalyrst-*.service` (fill `<WORKSPACE>`/`<DATA_DIR>`, `EnvironmentFile=`).

**4. Realm discovery (`/about`)** - clients discover the realm via `GET /about`; point content core's public URLs at the TLS host:

```ini
PUBLIC_URL=https://realm.example.com
CONTENT_URL=https://realm.example.com/content/     # trailing path matters
LAMBDAS_URL=https://realm.example.com/lambdas/
CONTENT_SERVER_ADDRESS=https://realm.example.com/content
REALM_NAME=my-realm
COMMS_PROTOCOL=v3
COMMS_FIXED_ADAPTER=signed-login:https://realm.example.com/comms-gatekeeper/get-scene-adapter
```

The client reads `content.publicUrl`, `lambdas.publicUrl`, `comms.adapter`/`comms.protocol` from `/about` - never a static URL.

**5. Start order + health** - reload proxy with the bundle server block:

```bash
systemctl --user start catalyrst.service           # content core first
systemctl --user start catalyrst-explore.service   # depends on content
systemctl --user start catalyrst-{create,social,data,abgen}.service
systemctl --user start catalyrst-social-rpc.service  # after social (gatekeeper)

for p in 5143 5144 5145 5146; do curl -s localhost:$p/health | jq .; done
# {"status":"ok","members":{...:"up"}}; "degraded" names the down member - bundles fail soft
```

**6. Smoke tests** - loopback:

```bash
curl -s 'localhost:5143/api/places?limit=1' | jq '.data|length'
curl -s 'localhost:5143/api/events' | jq '.total'
curl -s 'localhost:5143/pois' | jq 'length'
curl -s 'localhost:5144/profiles/metadata' -o /dev/null -w '%{http_code}\n'
curl -s 'localhost:5145/v1/communities?limit=1' | jq '.data|length'
curl -s 'localhost:5146/v1/catalog?first=1' | jq '.data|length'
curl -s 'localhost:5146/api/v3/simple/price?ids=decentraland&vs_currencies=usd' | jq .
curl -s -o /dev/null -w '%{http_code}\n' 'localhost:5147/manifest/doesnotexist_windows.json'  # 404
curl -s -o /dev/null -w '%{http_code}\n' -H 'Connection: Upgrade' -H 'Upgrade: websocket' \
  -H 'Sec-WebSocket-Key: x' -H 'Sec-WebSocket-Version: 13' localhost:5148/   # 101/426
```

Through the edge:

```bash
H=https://realm.example.com
curl -s "$H/about" | jq '.healthy, .configurations.realmName'
curl -s "$H/content/status" | jq '.version'
# spot parity vs upstream
diff <(curl -s "$H/api/places?limit=1" | jq -S .) \
     <(curl -s 'https://places.decentraland.org/api/places?limit=1' | jq -S .) | head
```

Green = every `/health` ok, edge `/about` `healthy:true`, smoke curls return data.

**7. Teardown / notes**

```bash
systemctl --user stop catalyrst-{explore,create,social,data,abgen,social-rpc}.service
# repoint the proxy; the content core is independent and can stay up
```

- Standalone `catalyrst-market` isolates marketplace from economy/price/credits; nginx `X-Accel-Redirect` (`STORAGE_X_ACCEL_BASE`) gives zero-copy content-serving throughput.

## Pointing the Unity explorer at catalyrst (full-coverage model)

Zero-client-change subset alternative: Gateway mode below.

Three layers resolve every backend to catalyrst:

1. **Client URL rewrite** - `CatalyrstUrlsSource` (`DecentralandUrlsSource` subclass) rewrites every statically-known `https://{service}.decentraland.{env}` host to `{CATALYRST_BASE}/{prefix}`; one construction site, `MainSceneLoader.cs`.
2. **Realm `/about` discovery** - `Lambdas`/`Content`/`EntitiesDeployment` (+ `EntitiesActive` fallback) derive from `/about`. Default without a CLI flag: point `Genesis` (`/realm-provider/main`) at a response listing this host.
3. **Edge path-routing** - one front host; nginx strips a per-upstream prefix, forwards to the owning bundle.

Prefixes map upstream *subdomains*, not crates: upstreams sharing a bundle are told apart by the prefix nginx strips.

**Prefix -> bundle routing** (ports deployment-assigned):

| Front-host prefix | Upstream it stands in for | Target |
|---|---|---|
| `/content/`, `/lambdas/`, `/about`, `/status` (kept, not stripped) | realm content core | live |
| `/peer/` (rewrite `/peer/(.*) -> /$1`) | `peer.decentraland.org` catalyst | live |
| `/places/`, `/events/`, `/archipelago/`, `/realm-provider/`, `/worlds-content-server/`, `/map-api/`, `/lists/` | respective upstreams | explore |
| `/builder-api/`, `/camera-reel/`, `/ab-registry/` | builder-api, camera-reel-service, asset-bundle-registry | create |
| `/social-api/`, `/comms-gatekeeper/`, `/notifications/`, `/badges/`, `/media/`, `/assets-cdn/` | social stack | social |
| `/credits/`, `/rpc/` (ws) | credits, rpc relay | data |
| `/market/` | marketplace-server | market (or data) |
| `/ab-cdn/` | ab-cdn | abgen |
| `/social-rpc/` (ws) | rpc-social-service-ea | social-rpc |

`location /places/ { proxy_pass http://127.0.0.1:<PORT>/; }` - trailing slash strips prefix. WebSocket prefixes (`/rpc/`, `/social-rpc/`) need `proxy_http_version 1.1` + `Upgrade`/`Connection`.

**Not rewritten:** web links (`decentraland.org` web-app, Discord/Twitter/OpenSea/docs/reels, the CoinGecko rate URL) pass through unchanged - suffixes match the pattern, self-hosting any is one prefix-map entry. Crate-less services proxy straight to their upstream.

## Gateway mode - the stock explorer's gateway contract

Behind `USE_GATEWAY` (`GatewayUrlsSource`), unity-explorer rewrites every supported service URL onto one gateway host:

```
https://{subdomain}.decentraland.{env}/{path}  ->  https://{gateway}/{subdomain}/{path}
```

Path untouched; edge strips the leading subdomain - no explorer code change. Config: [deploy/nginx-catalyrst-gateway.conf](./deploy/nginx-catalyrst-gateway.conf).

`GatewayUrlsSource.SUPPORTED_URLS` subdomains route (+ `profile-images`). `Profiles` rides the `asset-bundle-registry` prefix (`{AssetBundleRegistry}/profiles`) - no separate `/profiles` prefix.

| Gateway subdomain | Bundle | Routed `DecentralandUrl`s |
|---|---|---|
| `places` | explore | ApiPlaces, ApiWorlds, ApiDestinations, Map, ContentModerationReport |
| `api` | explore | ApiChunks (`/v1/map.png`, `/v2/*` tiles) |
| `archipelago-ea-stats` | explore | ArchipelagoStatus, ArchipelagoHotScenes, RemotePeers |
| `worlds-content-server` | explore | WorldContentServer, RemotePeersWorld |
| `realm-provider-ea` | explorer-api | Genesis (`/main`) |
| `auth-api` | explorer-api | ApiAuth |
| `asset-bundle-registry` | create | AssetBundleRegistry(+Version), Profiles, EntitiesActiveElements |
| `camera-reel-service` | create | CameraReelImages/Places/Users |
| `social-api` | social | Communities, Members, ActiveCommunityVoiceChats |
| `comms-gatekeeper` | social | GateKeeperSceneAdapter, ChatAdapter, GatekeeperStatus, BannedUsers |
| `notifications` | social | Notifications |
| `badges` | social | Badges |
| `metamorph-api` | social | MediaConverter (`/convert`) |
| `assets-cdn` | social | CommunityThumbnail |
| `credits` | data | MarketplaceCredits |
| `ab-cdn` | ab-cdn | AssetBundlesCDN |
| `profile-images` | profile-images | profile thumbnails |

Not covered:

- Realm content/lambdas/about: from `/about`, served natively on the realm host; gateway conf includes those locations - one host can be both, drop for a pure gateway host.
- Never gateway-routed: `events`, `dcl-lists` (POI), `market`, `builder-api`, `rpc` relay, `social-rpc` WebSocket. Full self-hosting needs the full-coverage model.

Stock explorer: pin DNS/`/etc/hosts` so `gateway.decentraland.org`+`.zone` resolve here, set `server_name`, or use an explorer-side override - the transform only fires for `.decentraland.{org,zone}` hosts.
