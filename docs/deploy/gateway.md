# Gateway mode - serving the stock explorer's gateway contract

Behind the `USE_GATEWAY` feature flag (`GatewayUrlsSource`), unity-explorer rewrites every supported service URL onto one gateway host, moving the subdomain into the first path segment:

```
https://{subdomain}.decentraland.{env}/{path}  ->  https://{gateway}/{subdomain}/{path}
```

The path is untouched and catalyrst members serve the real upstream paths at their bundle root, so the edge only strips the leading subdomain segment and forwards the rest - no explorer code change required. Config: [`nginx-catalyrst-gateway.conf`](./nginx-catalyrst-gateway.conf).

## Subdomain -> bundle map

Only subdomains in `GatewayUrlsSource.SUPPORTED_URLS` are routed (plus the non-client-origin `profile-images` host). `Profiles` resolves to `{AssetBundleRegistry}/profiles`, so it inherits the `asset-bundle-registry` prefix - there is no separate `/profiles` gateway prefix.

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

## Not covered

- Realm content/lambdas/about - discovered from realm `/about` (`content.publicUrl` / `lambdas.publicUrl`), so they must be served natively on the realm host. The gateway conf includes those locations so one host can be both realm and gateway; drop them for a pure gateway host.
- Never gateway-routed: `events`, `dcl-lists` (POI), `market`, `builder-api`, the `rpc` relay, and the `social-rpc` friends WebSocket are absent from `SUPPORTED_URLS`. For full self-hosting, intercept their hosts too - the [explorer-pointing](./explorer-pointing.md) model, which rewrites every host.

Pointing the stock explorer here: pin DNS/`/etc/hosts` so `gateway.decentraland.org` (and `.zone`) resolve here and set `server_name` accordingly, or use an explorer-side gateway-host override. Caveat: the stock transform only fires for hosts under `.decentraland.{org,zone}` - it cannot retarget to an arbitrary apex on its own; use hostname interception or the explorer-side override.
