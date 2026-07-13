# Pointing the Unity explorer at catalyrst (full-coverage model)

> Full dossier (per-`DecentralandUrl` table + a ready `CatalyrstUrlsSource.cs`):
> `ff400cab^:catalyrst/docs/explorer-pointing/`. Zero-client-change alternative covering a subset
> of services: [gateway.md](./gateway.md).

Making `unity-explorer` resolve every Decentraland backend to catalyrst takes three layers, all required:

1. **Client URL rewrite** - a `CatalyrstUrlsSource` subclass of `DecentralandUrlsSource` (modelled
   on `GatewayUrlsSource`) rewrites every statically-known `https://{service}.decentraland.{env}`
   host to `{CATALYRST_BASE}/{prefix}`; swapped in at the single construction site in
   `MainSceneLoader.cs`, consumed downstream as `IDecentralandUrlsSource`.
2. **Realm `/about` discovery** - `Lambdas`, `Content`, `EntitiesDeployment` (and the realm-served
   `EntitiesActive` fallbacks) are not statically known; the client derives them from `/about`'s
   `content.publicUrl` / `lambdas.publicUrl`. Set them in the content core's env.
3. **Edge path-routing** - one front host; nginx strips a per-upstream prefix and forwards the
   remainder to the owning bundle.

Prefixes are per upstream *subdomain*, not per member crate: member crates are 1:1 ports serving
the exact upstream paths (`/api/places`, `/get-scene-adapter`, `/v1/communities`, ...) merged at
the bundle root, so two upstreams sharing a bundle (places + events on explore; social-api +
comms-gatekeeper on social) are disambiguated only by the front-host prefix, which nginx strips.
The C# rewrite stays a pure host->prefix substitution, mirroring `GatewayUrlsSource`.

**Prefix -> bundle routing** (the shape; ports are deployment-assigned):

| Front-host prefix | Upstream it stands in for | Target |
|---|---|---|
| `/content/`, `/lambdas/`, `/about`, `/status` (kept, not stripped) | realm content core | live |
| `/peer/` (rewrite `/peer/(.*) -> /$1`) | `peer.decentraland.org` legacy catalyst | live |
| `/places/`, `/events/`, `/archipelago/`, `/realm-provider/`, `/worlds-content-server/`, `/map-api/`, `/lists/` | respective upstreams | explore |
| `/builder-api/`, `/camera-reel/`, `/ab-registry/` | builder-api, camera-reel-service, asset-bundle-registry | create |
| `/social-api/`, `/comms-gatekeeper/`, `/notifications/`, `/badges/`, `/media/`, `/assets-cdn/` | social stack | social |
| `/credits/`, `/rpc/` (ws) | credits, rpc relay | data |
| `/market/` | marketplace-server | market (or data) |
| `/ab-cdn/` | ab-cdn | abgen |
| `/social-rpc/` (ws) | rpc-social-service-ea | social-rpc |

`location /places/ { proxy_pass http://127.0.0.1:<PORT>/; }` - the trailing slash on `proxy_pass`
strips the prefix. WebSocket prefixes (`/rpc/`, `/social-rpc/`) need `proxy_http_version 1.1` +
`Upgrade`/`Connection` headers.

**Realm `/about` values** (layer 2), set on the content core (consumed by `bin/live.rs` +
`handlers/about.rs`):

```ini
PUBLIC_URL=https://<CATALYRST_HOST>
CONTENT_URL=https://<CATALYRST_HOST>/content/     # trailing path matters
LAMBDAS_URL=https://<CATALYRST_HOST>/lambdas/
REALM_NAME=catalyrst
COMMS_PROTOCOL=v3
COMMS_FIXED_ADAPTER=signed-login:https://<CATALYRST_HOST>/comms-gatekeeper/get-scene-adapter
```

The client reads `content.publicUrl`, `lambdas.publicUrl`, and `comms.adapter`/`comms.protocol`
from `/about`, never through the URL source. To make catalyrst the default realm without a CLI
flag, point `Genesis` (`/realm-provider/main`) at a realm-provider response listing this host.

**Deliberately NOT rewritten:** outbound web links (`decentraland.org` web-app routes,
Discord/Twitter, OpenSea, docs, reels, the CoinGecko rate URL) pass through unchanged so links
keep working. Their host suffixes already match the rewrite pattern - self-hosting any of them is
one entry in the prefix map. Services without a first-class crate can be proxied straight through
to the real upstream at their prefix until a native crate exists.
