# Gateway mode — serving the stock explorer's gateway contract

The unity-explorer has a built-in "gateway" path (`GatewayUrlsSource`, gated by
the `USE_GATEWAY` feature flag). When it is on, the client rewrites every
*supported* service URL from its own host into a single gateway host, moving the
service subdomain into the first path segment:

```
https://{subdomain}.decentraland.{env}/{path}   ->   https://{gateway}/{subdomain}/{path}
```

| Original | Gateway form |
|---|---|
| `ab-cdn.decentraland.org/v44/<e>/<h>` | `{gateway}/ab-cdn/v44/<e>/<h>` |
| `places.decentraland.org/api/places` | `{gateway}/places/api/places` |
| `asset-bundle-registry.decentraland.org/profiles` | `{gateway}/asset-bundle-registry/profiles` |
| `comms-gatekeeper.decentraland.org/get-scene-adapter` | `{gateway}/comms-gatekeeper/get-scene-adapter` |

**Gateway mode lets catalyrst answer this contract with no explorer code
change.** Because the transform is a pure host → `{gateway}/{subdomain}`
substitution that leaves the path untouched, and the catalyrst members are 1:1
ports that serve the real upstream paths at the bundle root, the edge only has to
strip the leading subdomain segment and forward the rest to the bundle that owns
that subdomain. This is the same prefix-stripping nginx already does for the
[curated-prefix model](../explorer-pointing/README.md) — only the prefix names
differ (here they are the *exact* upstream subdomains).

Config: [`nginx-catalyrst-gateway.conf`](./nginx-catalyrst-gateway.conf).

## Subdomain → bundle map

Only the subdomains the client actually rewrites are routed. The set is
`GatewayUrlsSource.SUPPORTED_URLS` (plus the non-client-origin `profile-images`
host); every entry below corresponds to a `DecentralandUrl` in that set.

| Gateway subdomain | Bundle | `DecentralandUrl`s routed there |
|---|---|---|
| `places` | explore | `ApiPlaces`, `ApiWorlds`, `ApiDestinations`, `Map`, `ContentModerationReport` |
| `api` | explore | `ApiChunks` (`/v1/map.png`, `/v2/*` tiles) |
| `archipelago-ea-stats` | explore | `ArchipelagoStatus`, `ArchipelagoHotScenes`, `RemotePeers` |
| `worlds-content-server` | explore | `WorldContentServer`, `RemotePeersWorld` |
| `realm-provider-ea` | explorer-api | `Genesis` (`/main`) |
| `auth-api` | explorer-api | `ApiAuth` |
| `asset-bundle-registry` | create | `AssetBundleRegistry`, `AssetBundleRegistryVersion`, `Profiles`, `EntitiesActiveElements` |
| `camera-reel-service` | create | `CameraReelImages`, `CameraReelPlaces`, `CameraReelUsers` |
| `social-api` | social | `Communities`, `Members`, `ActiveCommunityVoiceChats` |
| `comms-gatekeeper` | social | `GateKeeperSceneAdapter`, `ChatAdapter`, `GatekeeperStatus`, `BannedUsers` |
| `notifications` | social | `Notifications` |
| `badges` | social | `Badges` |
| `metamorph-api` | social | `MediaConverter` (`/convert`) |
| `assets-cdn` | social | `CommunityThumbnail` |
| `credits` | data | `MarketplaceCredits` |
| `ab-cdn` | ab-cdn | `AssetBundlesCDN` |
| `profile-images` | profile-images | non-client-origin profile thumbnails |

`Profiles` resolves to `{AssetBundleRegistry}/profiles`, so it inherits the
`asset-bundle-registry` prefix — there is no separate `/profiles` gateway prefix.

## What gateway mode does NOT cover

The client only rewrites the URLs in `SUPPORTED_URLS`. Everything else keeps its
own host and is **not** reached through the gateway:

- **Realm content/lambdas/about** — discovered from the realm `/about`
  (`content.publicUrl` / `lambdas.publicUrl`), so they must be served as natives
  on the realm host. `nginx-catalyrst-gateway.conf` includes those locations so a
  single host can be both realm and gateway; drop them for a pure gateway host.
- **Not gateway-routed services** — `events`, `dcl-lists` (POI), `market`,
  `builder-api`, the `rpc` HTTP/WS relay and the `social-rpc` friends WebSocket
  are absent from `SUPPORTED_URLS`. To self-host those too, intercept their own
  hosts as well (the [curated-prefix model](../explorer-pointing/README.md)
  rewrites *every* host and is the option when you want full coverage).

## Pointing the stock explorer here

No explorer change is required — only that gateway traffic reaches this host:

- pin DNS / `/etc/hosts` so `gateway.decentraland.org` (and `.zone`) resolve to
  this host, and set `server_name` in the config to match; or
- set `server_name` to your own host and feed it via the explorer's gateway host
  override.

> Caveat: the stock `GatewayUrlsSource` transform only fires for hosts under
> `.decentraland.{org,zone}` — it cannot retarget to a bare `.dcl.one` host on
> its own. Use the `gateway.decentraland.*` hostname interception above (or an
> explorer-side gateway-host override) to drive traffic to a custom host.
