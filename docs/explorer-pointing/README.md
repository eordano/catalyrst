# Pointing the Unity explorer at catalyrst

This document is the mechanism to make `unity-explorer` resolve every
Decentraland backend to a catalyrst deployment instead of `*.decentraland.org`.

There are three layers, and all three are needed:

1. **Client URL rewrite** — a `CatalyrstUrlsSource` subclass of
   `DecentralandUrlsSource` (modelled on `GatewayUrlsSource`) that rewrites
   every `https://{service}.decentraland.{env}` host that the client *knows
   statically* (the `RawUrl` switch) to the single catalyrst front host.
   File: [`CatalyrstUrlsSource.cs`](./CatalyrstUrlsSource.cs).

2. **Realm /about discovery** — the URLs the client does **not** know
   statically (`Lambdas`, `Content`, `EntitiesDeployment`, and the
   realm-served `EntitiesActive` / `WorldEntitiesActive` fallbacks). These are
   derived from the realm `/about` document's `content.publicUrl` and
   `lambdas.publicUrl`. Point them at catalyrst (§3).

3. **reverse-proxy path-routing** — one front host (`CATALYRST_BASE`,
   e.g. `https://<CATALYRST_HOST>`) that path-routes a per-upstream prefix to
   the bundle that owns that service (§4).

## Bundle topology

Each bundle process `merge`s its member crates' routers with **no per-member
path prefix** (see `crates/catalyrst-{explore,create,social,data}/src/main.rs`),
so within a bundle every member serves its **real routes at the bundle root**.
nginx is therefore responsible for mapping each *upstream subdomain* to a path
prefix, stripping it, and forwarding the remaining path verbatim to the bundle.

Each bundle listens on its own loopback port (the deployment's assigned port,
shown below as `<PORT_*>` placeholders — substitute the ports your deployment
allocates). The relative offsets are arbitrary; only the per-bundle mapping
matters.

| Bundle / service        | Loopback port | Members (real routes served at root)                                   |
|-------------------------|---------------|------------------------------------------------------------------------|
| catalyrst-live (content)| `<PORT_LIVE>` | `/about`, `/content/*`, `/lambdas/*`, `/status`                         |
| catalyrst-market        | `<PORT_MARKET>` | marketplace `/v1/*`, `/v2/*` (standalone; also a member of `data`)    |
| catalyrst-explore       | `<PORT_EXPLORE>` | places, events, archipelago, worlds, map, lists                     |
| catalyrst-create        | `<PORT_CREATE>` | builder, camera-reel, ab-registry                                    |
| catalyrst-social        | `<PORT_SOCIAL>` | communities, comms, notifications, badges, media                     |
| catalyrst-data          | `<PORT_DATA>` | market, economy, price, credits, rpc                                   |
| catalyrst-ab-cdn        | `<PORT_AB_CDN>` | asset-bundle CDN blob serving                                        |
| catalyrst-social-rpc    | `<PORT_SOCIAL_RPC>` (ws) | social-service friends RPC over WebSocket                    |

## 1. Complete `DecentralandUrl` → catalyrst mapping

`{B}` = `CATALYRST_BASE` (the front host, e.g. `https://<CATALYRST_HOST>`).
`{Bws}` = same host with `wss://` scheme.
The **port** column is the loopback bundle nginx proxies the prefix to.

Routes are reproduced exactly as the member crate serves them, because the
bundle does not prefix members — nginx strips the front-host prefix and the
remaining path hits the member router unchanged.

### Client-rewritten (handled by `CatalyrstUrlsSource`)

| DecentralandUrl | Upstream | Catalyrst URL (front host) | Port |
|---|---|---|---|
| `ApiPlaces` | `places…/api/places` | `{B}/places/api/places` | `<PORT_EXPLORE>` |
| `ApiWorlds` | `places…/api/worlds` | `{B}/places/api/worlds` | `<PORT_EXPLORE>` |
| `ApiDestinations` | `places…/api/destinations` | `{B}/places/api/destinations` | `<PORT_EXPLORE>` |
| `Map` | `places…/api/map` | `{B}/places/api/map` | `<PORT_EXPLORE>` |
| `ContentModerationReport` | `places…/api/report` | `{B}/places/api/report` | `<PORT_EXPLORE>` |
| `ApiEvents` | `events…/api/events` | `{B}/events/api/events` | `<PORT_EXPLORE>` |
| `ArchipelagoStatus` | `archipelago-ea-stats…/status` | `{B}/archipelago/status` | `<PORT_EXPLORE>` |
| `ArchipelagoHotScenes` | `archipelago-ea-stats…/hot-scenes` | `{B}/archipelago/hot-scenes` | `<PORT_EXPLORE>` |
| `RemotePeers` | `archipelago-ea-stats…/comms/peers` | `{B}/archipelago/comms/peers` | `<PORT_EXPLORE>` |
| `Genesis` | `realm-provider-ea…/main` | `{B}/realm-provider/main` | `<PORT_EXPLORE>` |
| `RemotePeersWorld` | `worlds-content-server…/wallet/[USER-ID]/connected-world` | `{B}/worlds-content-server/wallet/[USER-ID]/connected-world` | `<PORT_EXPLORE>` |
| `WorldServer` | `worlds-content-server…/world` | `{B}/worlds-content-server/world` | `<PORT_EXPLORE>` |
| `WorldContentServer` | `worlds-content-server…/contents/` | `{B}/worlds-content-server/contents/` | `<PORT_EXPLORE>` |
| `WorldPermissions` | `worlds-content-server…/world/{0}/permissions` | `{B}/worlds-content-server/world/{0}/permissions` | `<PORT_EXPLORE>` |
| `WorldComms` | `worlds-content-server…/worlds/{0}/comms` | `{B}/worlds-content-server/worlds/{0}/comms` | `<PORT_EXPLORE>` |
| `WorldCommsAdapter` | `worlds-content-server…/worlds/{0}/scenes/{1}/comms` | `{B}/worlds-content-server/worlds/{0}/scenes/{1}/comms` | `<PORT_EXPLORE>` |
| `ApiChunks` | `api…/v1/map.png` | `{B}/map-api/v1/map.png` | `<PORT_EXPLORE>` |
| `POI` | `dcl-lists…/pois` | `{B}/lists/pois` | `<PORT_EXPLORE>` |
| `BuilderApiDtos` | `builder-api…/v1/collections/[COL-ID]/items` | `{B}/builder-api/v1/collections/[COL-ID]/items` | `<PORT_CREATE>` |
| `BuilderApiContent` | `builder-api…/v1/storage/contents/` | `{B}/builder-api/v1/storage/contents/` | `<PORT_CREATE>` |
| `BuilderApiNewsletter` | `builder-api…/v1/newsletter` | `{B}/builder-api/v1/newsletter` | `<PORT_CREATE>` |
| `CameraReelUsers` | `camera-reel-service…/api/users` | `{B}/camera-reel/api/users` | `<PORT_CREATE>` |
| `CameraReelImages` | `camera-reel-service…/api/images` | `{B}/camera-reel/api/images` | `<PORT_CREATE>` |
| `CameraReelPlaces` | `camera-reel-service…/api/places` | `{B}/camera-reel/api/places` | `<PORT_CREATE>` |
| `AssetBundleRegistry` | `asset-bundle-registry…` | `{B}/ab-registry` | `<PORT_CREATE>` |
| `AssetBundleRegistryVersion` | `asset-bundle-registry…/entities/versions` | `{B}/ab-registry/entities/versions` | `<PORT_CREATE>` |
| `Profiles` | `asset-bundle-registry…/profiles` | `{B}/ab-registry/profiles` | `<PORT_CREATE>` |
| `ProfilesMetadata` | `asset-bundle-registry…/profiles/metadata` | `{B}/ab-registry/profiles/metadata` | `<PORT_CREATE>` |
| `EntitiesActiveElements` | `asset-bundle-registry…/entities/active` | `{B}/ab-registry/entities/active` | `<PORT_CREATE>` |
| `Communities` | `social-api…/v1/communities` | `{B}/social-api/v1/communities` | `<PORT_SOCIAL>` |
| `Members` | `social-api…/v1/members` | `{B}/social-api/v1/members` | `<PORT_SOCIAL>` |
| `SocialServiceMutes` | `social-api…/v1/mutes` | `{B}/social-api/v1/mutes` | `<PORT_SOCIAL>` |
| `ActiveCommunityVoiceChats` | `social-api…/v1/community-voice-chats/active` | `{B}/social-api/v1/community-voice-chats/active` | `<PORT_SOCIAL>` |
| `Gatekeeper` | `comms-gatekeeper…` | `{B}/comms-gatekeeper` | `<PORT_SOCIAL>` |
| `GateKeeperSceneAdapter` | `comms-gatekeeper…/get-scene-adapter` | `{B}/comms-gatekeeper/get-scene-adapter` | `<PORT_SOCIAL>` |
| `GatekeeperStatus` | `comms-gatekeeper…/status` | `{B}/comms-gatekeeper/status` | `<PORT_SOCIAL>` |
| `ChatAdapter` | `comms-gatekeeper…/private-messages/token` | `{B}/comms-gatekeeper/private-messages/token` | `<PORT_SOCIAL>` |
| `BannedUsers` | `comms-gatekeeper…/users/{0}/bans` | `{B}/comms-gatekeeper/users/{0}/bans` | `<PORT_SOCIAL>` |
| `SceneAdmins` | `comms-gatekeeper…/scene-admin` | `{B}/comms-gatekeeper/scene-admin` | `<PORT_SOCIAL>` |
| `Notifications` | `notifications…` | `{B}/notifications` | `<PORT_SOCIAL>` |
| `Badges` | `badges…` | `{B}/badges` | `<PORT_SOCIAL>` |
| `CommunityThumbnail` | `assets-cdn…/social/communities/{0}/raw-thumbnail.png` | `{B}/assets-cdn/social/communities/{0}/raw-thumbnail.png` | `<PORT_SOCIAL>` |
| `MediaConverter` | `metamorph-api…/convert?url={0}` | `{B}/media/convert?url={0}` | `<PORT_SOCIAL>` |
| `ChatTranslate` | `autotranslate-server…/translate` | `{B}/media/translate` | `<PORT_SOCIAL>` |
| `Market` | `market…` | `{B}/market` | `<PORT_MARKET>` / `<PORT_DATA>` |
| `MarketplaceCredits` | `credits…` | `{B}/credits` | `<PORT_DATA>` |
| `ApiRpc` | `wss://rpc…` | `{Bws}/rpc` | `<PORT_DATA>` |
| `ApiFriends` | `wss://rpc-social-service-ea…` | `{Bws}/social-rpc` | `<PORT_SOCIAL_RPC>` |
| `AssetBundlesCDN` | `ab-cdn…` | `{B}/ab-cdn` | `<PORT_AB_CDN>` |
| `ApiAuth` | `auth-api…` | `{B}/auth-api` | — (see note) |
| `MetaTransactionServer` | `transactions-api…/v1/transactions` | `{B}/transactions-api/v1/transactions` | — (see note) |
| `FeatureFlags` | `feature-flags…` | `{B}/feature-flags` | — (see note) |
| `Blocklist` | `config…/denylist.json` | `{B}/config/denylist.json` | — (see note) |
| `PeerAbout` | `peer…/about` | `{B}/peer/about` | `<PORT_LIVE>` (see note) |
| `Servers` | `peer…/lambdas/contracts/servers` | `{B}/peer/lambdas/contracts/servers` | `<PORT_LIVE>` (see note) |

> Note — services with no first-class catalyrst crate yet (`auth-api`,
> `transactions-api`, `feature-flags`, `config`): the rewrite still points them
> at `{B}/<prefix>`; nginx may proxy these straight through to the real
> upstream (`proxy_pass https://auth-api.decentraland.org;`) until a native
> crate exists. `peer.*` is the legacy catalyst host; route its `/about`,
> `/lambdas/*`, `/comms/*` to the catalyrst-live content server (`<PORT_LIVE>`).

### Realm-discovered (NOT rewritten by the client — set via /about, §3)

| DecentralandUrl | How it resolves |
|---|---|
| `Lambdas` | realm `/about` `lambdas.publicUrl` → `{B}/lambdas` (`<PORT_LIVE>`) |
| `Content` | realm `/about` `content.publicUrl` → `{B}/content` (`<PORT_LIVE>`) |
| `EntitiesDeployment` | `{content.publicUrl}/entities/` → `{B}/content/entities/` (`<PORT_LIVE>`) |
| `EntitiesActive` | realm fallback `{content.publicUrl}/entities/active`, OR (if `ASSET_BUNDLE_FALLBACK` FF on) `AssetBundleRegistry/entities/active` → already rewritten to `{B}/ab-registry/entities/active` |
| `WorldEntitiesActive` | same as `EntitiesActive` but world-scoped |
| `LocalGateKeeperSceneAdapter` | local-dev only; `comms-gatekeeper-local.decentraland.org` is not rewritten by `MatchDecentralandSubdomain` (different domain) — supply via `gatekeeperBaseOverride` if needed |

### External links — passed through unchanged (NOT rewritten)

These never hit a decentraland service host, or are intentional outbound links;
`CatalyrstUrlsSource` leaves them as-is.

`Host`, `MarketplaceLink`, `MarketplaceClaimName`, `GoShoppingWithMarketplaceCredits`,
`SupportLink`, `Help`, `Account`, `DAO`, `CreatorHub`, `PrivacyPolicy`, `TermsOfUse`,
`ContentPolicy`, `CodeOfEthics`, `AuthSignatureWebApp`, `WhatsOnNewEventLink`,
`WhatsOnEventLink`, `JumpInGenesisCityLink`, `JumpInWorldLink`, `ReportUserForm`,
`CommunityProfileLink` (all `https://decentraland.{ENV}/…` web-app routes);
`MinimumSpecs`, `Support` (`docs.decentraland.{ENV}` — add a `docs` prefix if you
want to self-host); `OpenSea` (`opensea.decentraland.{ENV}` — add an `opensea`
prefix to self-host, otherwise external); and the hard external links
`DiscordDirectLink`, `TwitterLink`, `TwitterNewPostLink`, `NewsletterSubscriptionLink`,
`DecentralandWorlds`, `ManaUsdRateApiUrl` (coingecko), `CameraReelLink` (`reels.…`).

> If you *do* want `decentraland.{ENV}` web-app routes or `docs`/`opensea`/
> `reels` to be self-hosted, the host suffix already matches
> `.decentraland.{env}`; just add the bare subdomain (`""` for the apex,
> `docs`, `opensea`, `reels`) to the `hostPrefix` map. They are deliberately
> omitted so outbound links keep working against the real web app.

## 2. DI registration

Replace the `GatewayUrlsSource` construction in
`Explorer/Assets/DCL/Infrastructure/Global/Dynamic/MainSceneLoader.cs:192`:

```csharp
// was:
// var decentralandUrlsSource = new GatewayUrlsSource(decentralandEnvironment, realmData, launchSettings, gatekeeperBaseOverride);

var catalyrstBase = Environment.GetEnvironmentVariable("CATALYRST_BASE"); // null => default in the subclass
var decentralandUrlsSource = new CatalyrstUrlsSource(
    decentralandEnvironment, realmData, launchSettings, catalyrstBase, gatekeeperBaseOverride);
```

`CatalyrstUrlsSource` is a drop-in `DecentralandUrlsSource`. `MainSceneLoader`
already passes it everywhere downstream as `IDecentralandUrlsSource`
(via `BootstrapContainer`), so no other DI wiring changes. To keep the gateway
as an option, gate the choice on a launch flag / env var and pick one of the
two subclasses at that single call site.

For the standalone playground/test constructors
(`SelfProfilePlayground`, `ScreenRecorderTester`), swap
`new DecentralandUrlsSource(...)` for `new CatalyrstUrlsSource(...)` if you
want those paths pointed at catalyrst too; they are not on the runtime path.

## 3. Realm `/about` content / lambdas / comms values

The realm is discovered from `/about` (served by catalyrst-live on **`<PORT_LIVE>`**,
fronted at `{B}/peer/about` and as the realm root). The client reads three
fields and never rewrites them through `CatalyrstUrlsSource`:

- `content.publicUrl` → becomes `IpfsRealm.ContentBaseUrl` (and, with
  `/entities/`, `EntitiesBaseUrl`; with `/entities/active`,
  `EntitiesActiveEndpoint`). See `IpfsRealm.cs:35-41`.
- `lambdas.publicUrl` → becomes `IpfsRealm.LambdasBaseUrl`.
- `comms.adapter` / `comms.protocol` → the comms transport the client connects
  to.

Set these in the catalyrst-live server env (consumed by
`crates/catalyrst-server/src/bin/live.rs` and `handlers/about.rs`):

```ini
# front host the explorer reaches us at
PUBLIC_URL=https://<CATALYRST_HOST>
# realm-discovered content + lambdas (must be absolute, must end with the
# trailing path the client expects)
CONTENT_URL=https://<CATALYRST_HOST>/content/
LAMBDAS_URL=https://<CATALYRST_HOST>/lambdas/
REALM_NAME=catalyrst

# comms config returned in /about
COMMS_PROTOCOL=v3
# fixed adapter the client dials for the main realm; point at the catalyrst
# comms-gatekeeper-minted LiveKit room or the ws-connector
COMMS_FIXED_ADAPTER=signed-login:https://<CATALYRST_HOST>/comms-gatekeeper/get-scene-adapter
COMMS_WS_CONNECTOR_URL=http://127.0.0.1:5001
COMMS_STATS_URL=http://127.0.0.1:5002
```

Resulting `/about` (the load-bearing fields):

```json
{
  "healthy": true,
  "content":  { "healthy": true, "publicUrl": "https://<CATALYRST_HOST>/content/" },
  "lambdas":  { "healthy": true, "publicUrl": "https://<CATALYRST_HOST>/lambdas/" },
  "comms":    { "healthy": true, "protocol": "v3",
                "fixedAdapter": "signed-login:https://<CATALYRST_HOST>/comms-gatekeeper/get-scene-adapter" },
  "configurations": { "realmName": "catalyrst", "networkId": 1 }
}
```

With those set:
- `DecentralandUrl.Content` → `https://<CATALYRST_HOST>/content/`
- `DecentralandUrl.EntitiesDeployment` → `https://<CATALYRST_HOST>/content/entities/`
- `DecentralandUrl.Lambdas` → `https://<CATALYRST_HOST>/lambdas/`
- `DecentralandUrl.EntitiesActive` (realm-served path) →
  `https://<CATALYRST_HOST>/content/entities/active`

> The client jumps to a realm either by name (`realm=catalyrst`) — in which case
> it fetches `{realmBase}/about` and `CatalystBaseUrl = realmName` — or by the
> genesis flow. To make `catalyrst` the default realm without a CLI flag, point
> `DecentralandUrl.Genesis` (already rewritten to `{B}/realm-provider/main`) at
> a realm-provider response that lists `https://<CATALYRST_HOST>` (or
> `{B}/peer`) as a realm.

## 4. nginx path-routing (front host)

One server block on `CATALYRST_BASE`. Each upstream prefix is stripped and the
remaining path proxied to the bundle's loopback port. `wss://` upstreams need
`Upgrade`/`Connection` headers.

| Front-host prefix | strip → upstream path | proxy_pass |
|---|---|---|
| `/content/` , `/lambdas/` , `/about` , `/status` | (kept) | `http://127.0.0.1:<PORT_LIVE>` |
| `/peer/` | rewrite `/peer/(.*)` → `/$1` | `http://127.0.0.1:<PORT_LIVE>` |
| `/places/` , `/events/` , `/archipelago/` , `/realm-provider/` , `/worlds-content-server/` , `/map-api/` , `/lists/` | strip prefix | `http://127.0.0.1:<PORT_EXPLORE>` |
| `/builder-api/` , `/camera-reel/` , `/ab-registry/` | strip prefix | `http://127.0.0.1:<PORT_CREATE>` |
| `/social-api/` , `/comms-gatekeeper/` , `/notifications/` , `/badges/` , `/media/` , `/assets-cdn/` | strip prefix | `http://127.0.0.1:<PORT_SOCIAL>` |
| `/credits/` , `/rpc/` (ws) | strip prefix | `http://127.0.0.1:<PORT_DATA>` |
| `/market/` | strip prefix | `http://127.0.0.1:<PORT_MARKET>` (or `<PORT_DATA>`) |
| `/ab-cdn/` | strip prefix | `http://127.0.0.1:<PORT_AB_CDN>` |
| `/social-rpc/` (ws) | strip prefix | `http://127.0.0.1:<PORT_SOCIAL_RPC>` |

Example location (strip prefix, forward remainder):

```nginx
location /places/ { proxy_pass http://127.0.0.1:<PORT_EXPLORE>/; }  # trailing / strips /places/
location /comms-gatekeeper/ { proxy_pass http://127.0.0.1:<PORT_SOCIAL>/; }
location /rpc/ {                                                     # wss
    proxy_pass http://127.0.0.1:<PORT_DATA>/;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
}
location /social-rpc/ {                                             # wss
    proxy_pass http://127.0.0.1:<PORT_SOCIAL_RPC>/;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
}
```

> The `proxy_pass http://host:port/;` form (with trailing slash) replaces the
> matched `location` prefix with `/`, i.e. strips `/places/` and forwards the
> rest. This is exactly what the bundle expects, because members are merged at
> the bundle root with their real routes.

## Why a path prefix per *subdomain* (not per *member*)

The member crates were ported 1:1 from the upstream services and serve the
exact upstream paths (`/api/places`, `/get-scene-adapter`, `/v1/communities`,
`/entities/active`, `/v1/catalog`). The bundles `merge` them without a prefix,
so distinct upstream subdomains that share a bundle (e.g. `places` and `events`
both on `<PORT_EXPLORE>`, or `social-api` and `comms-gatekeeper` both on `<PORT_SOCIAL>`) must stay
disambiguated by their front-host prefix; nginx strips the prefix so the member
sees its native path. This keeps the C# rewrite a pure host→`{B}/{prefix}`
substitution with the path untouched, mirroring `GatewayUrlsSource`'s
host-only transform.
