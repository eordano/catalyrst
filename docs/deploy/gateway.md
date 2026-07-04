# Gateway mode — serving the stock explorer's gateway contract

> Status: re-verified 2026-07-03 (docs-stale-audit); paths cross-checked
> against `GatewayUrlsSource` in unity-explorer.

The unity-explorer ships a "gateway" path (`GatewayUrlsSource`, behind the
`USE_GATEWAY` feature flag): the client rewrites every *supported* service URL
from its own host onto one gateway host, moving the subdomain into the first
path segment:

```
https://{subdomain}.decentraland.{env}/{path}  →  https://{gateway}/{subdomain}/{path}
```

Because that transform is a pure host → `{gateway}/{subdomain}` substitution
with the path untouched, and catalyrst members serve the real upstream paths
at their bundle root, the edge only strips the leading subdomain segment and
forwards the rest — **no explorer code change required**. Config:
[`nginx-catalyrst-gateway.conf`](./nginx-catalyrst-gateway.conf).

## Subdomain → bundle map

Only subdomains the client actually rewrites are routed — the set is
`GatewayUrlsSource.SUPPORTED_URLS` (plus the non-client-origin
`profile-images` host):

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

`Profiles` resolves to `{AssetBundleRegistry}/profiles`, so it inherits the
`asset-bundle-registry` prefix — there is no separate `/profiles` gateway
prefix.

## What gateway mode does NOT cover

The client only rewrites `SUPPORTED_URLS`; everything else keeps its own host:

- **Realm content/lambdas/about** — discovered from realm `/about`
  (`content.publicUrl` / `lambdas.publicUrl`), so they must be served natively
  on the realm host. The gateway conf includes those locations so one host can
  be both realm and gateway; drop them for a pure gateway host.
- **Never gateway-routed** — `events`, `dcl-lists` (POI), `market`,
  `builder-api`, the `rpc` relay, and the `social-rpc` friends WebSocket are
  absent from `SUPPORTED_URLS`. For full self-hosting, intercept their hosts
  too — that's the [explorer-pointing](./explorer-pointing.md) model, which
  rewrites *every* host.

## Pointing the stock explorer here

Gateway traffic just has to reach this host: pin DNS/`/etc/hosts` so
`gateway.decentraland.org` (and `.zone`) resolve here and set `server_name`
accordingly, or use an explorer-side gateway-host override.

> Caveat: the stock transform only fires for hosts under
> `.decentraland.{org,zone}` — it cannot retarget to an arbitrary apex on its
> own; use hostname interception or the explorer-side override.
