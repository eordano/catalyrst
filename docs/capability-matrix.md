# Catalyrst user-capability parity matrix

Goal: prove a player can do, against a self-hosted catalyrst stack, everything
they can do against real Decentraland servers.

Method. Ground truth for "what the client needs" is the Unity client's full
network call catalog (`<DATA_DIR>/unity-net-catalog/catalog.db` +
`findings-*.jsonl`), keyed by `DecentralandUrl`. Per-capability status is the
intersection of (a) which catalyrst binary owns each `DecentralandUrl` target,
and (b) whether that binary serves a client-compatible wire shape. Status legend:

- **works** — every endpoint the capability needs is served by a catalyrst
  binary with a client-compatible wire shape (no client-breaking divergence).
- **partial** — capability is reachable but degraded: some sub-flows break, or
  fields the UI renders are missing/empty, while the core path still functions.
- **missing** — a required endpoint is a 501/stub or a wire-incompatible
  protocol, so the capability does not function end-to-end.

`{ENV}` = mainnet/zone/today; ports use the deployment's assigned port range
(shown as `<PORT>` placeholders below). Note that
several catalyrst federation services own DecentralandUrl targets whose real
hostnames differ (e.g. `social-api`, `comms-gatekeeper`, `asset-bundle-registry`);
the stack reaches them through realm/about rewriting, not by spoofing DNS.

---

## Binary → DecentralandUrl ownership map

| catalyrst binary (crate) | Port | Real target(s) it stands in for |
|---|---|---|
| catalyrst-server (`content`) | 5140/5141 | `Content`, `Lambdas`, `Profiles*` (catalyst content + lamb2), `PeerAbout`, `Servers` |
| catalyrst-market | 5133 | `Market` (marketplace-server) |
| catalyrst-places | 5134 | `ApiPlaces`, `ApiWorlds`, `ApiDestinations`, `ContentModerationReport` |
| catalyrst-events | 5135 | `ApiEvents` |
| catalyrst-communities | 5136 | `Communities`, `Members`, `ActiveCommunityVoiceChats` |
| catalyrst-social-rpc | (ws) | `ApiFriends` (rpc-social-service-ea), `SocialServiceMutes` via social-api |
| catalyrst-comms | — | `Gatekeeper`/`GateKeeperSceneAdapter`, `BannedUsers`, `SceneAdmins`, `ChatAdapter`, private/community voice |
| catalyrst-archipelago | 5138/5139 | `ArchipelagoStatus`, `ArchipelagoHotScenes`, `RemotePeers`, `/ws` comms |
| catalyrst-worlds | — | `WorldServer`, `WorldContentServer`, `WorldComms*`, `WorldPermissions`, `RemotePeersWorld` |
| catalyrst-ab-registry | — | `AssetBundleRegistry*`, `EntitiesActive*`, `Profiles*` (registry variant) |
| catalyrst-ab-cdn | — | `AssetBundlesCDN` (ab-cdn) |
| catalyrst-badges | — | `Badges` |
| catalyrst-notifications | — | `Notifications` |
| catalyrst-camera-reel | — | `CameraReelImages/Users/Places` |
| catalyrst-map | — | `Map`, `ApiChunks` (map.png), `POI` portion |
| catalyrst-lists | — | `POI`, `BannedUsers` names, `Blocklist` (dcl-lists) |
| catalyrst-explorer-api | — | `ApiAuth` (auth-server v2), `FeatureFlags`, realm `/about` fan-out, `Genesis`/realm-provider |
| catalyrst-economy | — | `MetaTransactionServer` (transactions-api) |
| catalyrst-credits | — | `MarketplaceCredits` (credits-server) |
| catalyrst-price | — | `ManaUsdRateApiUrl` (coingecko price), `Price` |
| catalyrst-rpc | 5141+ | `ApiRpc`, `rpc.decentraland.org` (eth JSON-RPC relay) |
| catalyrst-media | — | `ChatTranslate` (autotranslate), `MediaConverter` |
| catalyrst-builder | — | `BuilderApi*` |

---

## Capability matrix

| # | User capability | Endpoints needed (DecentralandUrl) | Served by (binary · route) | Status | Blocking reason / notes |
|---|---|---|---|---|---|
| 1 | **Auth / login** | `ApiAuth` POST/GET `/auth/v2/requests/{id}/outcome|validation`, `GET /identities/{token}`; `AuthSignatureWebApp` | catalyrst-explorer-api · `/auth/v2/...` | works | outcome/validation/identities all `match`; DashMap stand-in for Redis is behavior-only. Browser-signature web app is an external link, not an API. |
| 2 | **Realm / about discovery** | `PeerAbout` `/about`, `Genesis` realm-provider `/main`, `Servers` `/lambdas/contracts/servers` | catalyrst-explorer-api · `/main/about`, realm fan-out; catalyrst-server · `/about`,`/contracts/servers` | works | `/about` carries comms+content+lambdas; `acceptingUsers` always healthy and `comms.adapter` always present, both explorer-benign (RealmController coalesces). Extra `/status` keys additive. |
| 3 | **Feature-flag gated features** | `FeatureFlags` `GET /{appName}.json` | catalyrst-explorer-api · `modules/feature_flags.rs` | works | Flags served; consuming clients tolerate missing keys. |
| 4 | **Browse / load the map & tiles** | `Map` `/v1/tiles`,`/v2/tiles`,`/v2/parcels`,`/v2/estates`, `ApiChunks` map.png | catalyrst-map · `/v1\|v2/tiles`,`/v2/parcels/...` | partial | Tiles render; missing `rentalPricePerDay`/`rentalListing`, proximity attrs, and dissolved-estate 200 fallback (404 instead). Navmap usable; rental/estate detail degraded. |
| 5 | **POIs** | `POI` `/pois` | catalyrst-lists · `/pois` | works | `{data:[...]}` single-query port, shape `match`. |
| 6 | **Browse places / destinations** | `ApiPlaces` `/api/places`, `ApiDestinations` `/api/destinations`, places `/status`,`/map/places` | catalyrst-places · `/api/places`,`/api/destinations`,`/api/map` | partial | Envelope+ordering+search ranking match upstream; but `PlaceRow` omits `is_private`,`highlighted_image`,`featured*`,`realms_detail`,`live`,`connected_addresses`,`tags`,`sdk` and several filters (`only_favorites`,`owner`,`names`,`sdk`). JsonUtility tolerates omissions → no crash, but place-info/navmap UI is degraded. |
| 7 | **Browse worlds** | `ApiWorlds` `/api/worlds`, `WorldServer` `/world`, `WorldContentServer` `/contents/` | catalyrst-places · `/api/worlds`; catalyrst-worlds · `/world`,`/contents/` | works | World listing + content fetch functional. about-handler hardcodes content/lambdas `healthy=true` (cosmetic). |
| 8 | **Teleport (jump in) to a parcel/world** | `JumpInGenesisCityLink`/`JumpInWorldLink` (deep links) + realm `/about` + comms join | catalyrst-explorer-api (about) + catalyrst-comms (scene adapter) + catalyrst-archipelago (island) | partial | Genesis-city teleport works (about + archipelago island assignment via `/get-scene-adapter` LiveKit mint). Cross-realm comms join works; live peer discovery is degraded — see #14. |
| 9 | **Load scenes & asset bundles** | `EntitiesActive*`/`Content` entities+contents, `AssetBundleRegistry*` versions, `AssetBundlesCDN` bundle/LOD bytes | catalyrst-server · entities/contents; catalyrst-ab-cdn · bundle bytes; catalyrst-ab-registry · `/entities/active`,`/entities/versions` | partial | Content fetch + CDN byte-serving are full `match`. **Blocker:** ab-registry `POST /entities/active` and `/entities/versions` return flat `versions`/`bundles` instead of Unity's required `versions.assets.{mac,win}.{version,buildDate}` → AB version pin lost (breaks-client). Scenes still load uncompressed from content; AB optimization path is broken. |
| 10 | **View profile** | `Profiles` `GET /profiles/{id}`, lambdas profile routes | catalyrst-server (lamb2) · `/profiles/{id}`,`/profile/{id}`; catalyrst-ab-registry · `/profiles` | works | All lambdas profile routes present and shape-mapped. |
| 11 | **Edit / publish profile** | `Profiles` POST `/profiles[/metadata]`, content `POST /entities` | catalyrst-server · `POST /entities` (gated by READ_ONLY); catalyrst-ab-registry · `POST /profiles` | partial | Deployment route exists but is hidden when `READ_ONLY` is set (staging default). With writes enabled it works; default staging deployment is read-only → editing blocked. |
| 12 | **Wearables / emotes / names / lands** | lambdas `/users/{addr}/wearables|emotes|names|lands|third-party`, `/explorer/wearables|emotes`, `/collections`, `/outfits/{id}` | catalyrst-server (lamb2) · all of the above | works | All lamb2 user/collection/outfit routes present. Catalog pagination omits `lastId` echo — benign (cursor carried in opaque `next`; explorer never calls catalog route). |
| 13 | **Marketplace browse / buy** | `Market` `/v1/nfts`,`/orders`,`/catalog`,`/items`,`/trades`,`/trendings`,`/sales`,`/stats`; `MetaTransactionServer` `/v1/transactions` | catalyrst-market · those routes; catalyrst-economy · `/v1/transactions` | partial | Buy/relayer (`/transactions`) is full `match`. Browse is degraded: `/nfts` always `order/rental/activeOrderId=null`; `/orders` createdAt/updatedAt unit mismatch (ms vs s); `/catalog` picks hardcoded 0; **`/items` and `/trades` are empty stubs** (breaks-client for trade list); `/trendings` always `[]`. Listing/discovery views show incomplete data. |
| 14 | **Friends & presence** | `ApiFriends` WS-RPC `getFriends`,`getMutualFriends`,`getFriendshipStatus`,`upsertFriendship`,`subscribeTo*`; `SocialServiceMutes` | catalyrst-social-rpc · WS-RPC service; catalyrst-server/social-api · `/v1/mutes` | partial | Friend list + requests + subscriptions function. Divergences: `upsertFriendship` misses block enforcement and emits a blank `friend` profile (blank names/avatars in list); `getFriendshipStatus` consults blocks table (precedence drift). Real but non-fatal; presence subscriptions published (in-proc vs Redis transport). |
| 15 | **Communities** | `Communities` `/v1/communities`,`/{id}`,`/{id}/places|members|posts`; `Members` `/v1/members/...` | catalyrst-communities · those routes | partial | Read paths reachable but lossy: list omits `thumbnailUrl`,`ownerName`,`role`,`visibility`,`friends[]`,`voiceChatStatus` (cards render bare); **`/{id}/members` omits the profile fields the Unity converter requires → NullReferenceException (hard crash)**; post `like` always false. Community **writes return 501** (federation write path pending). |
| 16 | **Chat (nearby / scene)** | comms island via `GateKeeperSceneAdapter` `/get-scene-adapter` (LiveKit room) | catalyrst-comms · `/get-scene-adapter`; catalyrst-archipelago · island/`/ws` | partial | Scene/nearby chat rides the LiveKit room the scene-adapter mints — that mint works. But archipelago `/ws` is JSON-text vs upstream binary-protobuf (not wire-compatible) and `/comms/peers`,`/hot-scenes` paths/bodies differ → live peer presence/discovery in-island is degraded. |
| 17 | **Private (direct) messages** | `ChatAdapter` `/private-messages/token` | catalyrst-comms · `/private-messages/token` (deferred) | missing | Returns **501** — "needs MLS + social-rpc" (handlers/deferred.rs:5). No token → no private-message channel. |
| 18 | **Voice chat (scene)** | `GateKeeperSceneAdapter` LiveKit credential | catalyrst-comms · `/get-scene-adapter` | works | Scene-adapter mints a real LiveKit JWT (scene_adapter.rs:64) → in-scene voice works where a LiveKit host is configured. |
| 19 | **Voice chat (private / 1:1)** | comms `/private-voice-chat` POST/DELETE, `/users/{addr}/voice-chat-status`; social-rpc `startPrivateVoiceChat` | catalyrst-comms · deferred; catalyrst-social-rpc · `startPrivateVoiceChat` | partial | social-rpc `startPrivateVoiceChat` exists (missing 2 cross-busy guards) but the comms REST `/private-voice-chat` + `voice-chat-status` it depends on return **501** (needs LiveKit RoomService REST client) → 1:1 voice does not complete. |
| 20 | **Voice chat (community)** | `ApiFriends` RPC `Start/Join/EndCommunityVoiceChat`,`RequestToSpeak`,...; `ActiveCommunityVoiceChats`; comms `/community-voice-chat/*` | catalyrst-social-rpc · RPC (best-effort); catalyrst-comms · `/community-voice-chat/*` (deferred); catalyrst-communities · `/community-voice-chats/active` | missing | comms `/community-voice-chat/*` returns **501** (depends on communities federation gossip); social-rpc gatekeeper calls degrade best-effort. Community voice does not function. |
| 21 | **Events & RSVP** | `ApiEvents` `/api/events`, `/categories`, `/{id}`, RSVP POST | catalyrst-events · those routes | works | Time windows, search (ts_rank_cd), categories i18n match upstream; RSVP write path present. `/schedules/{id}` 404 (explorer doesn't call). |
| 22 | **Badges** | `Badges` `/categories`,`/users/{w}/badges`,`/users/{w}/preview`,`/badges/{id}/tiers` | catalyrst-badges · all four | works | All four endpoints `match`; no upstream to A/B but DTO+consumer shapes verified on disk. |
| 23 | **Camera reel** | `CameraReelImages/Users/Places` GET/POST/DELETE/PATCH (signed) | catalyrst-camera-reel · `/api/images|users|places...` | works | Verbatim port of upstream (itself Rust). Only divergence: image GET serves bytes (200) vs 302 redirect — client-invisible. |
| 24 | **Notifications** | `Notifications` `/notifications`,`/read`,`/subscription`,`/set-email`,`/subscription/opt-outs/...` | catalyrst-notifications · those routes | partial | Read/list/mark-read/subscription all work (timestamps omitted but benign). **`PUT /set-email` upserts but the email-confirmation flow is non-functional** (no confirmation drive) — but the explorer never drives confirmation, so this is a latent gap, not a client break. Treated as partial on the email sub-flow. |
| 25 | **Translate chat / media convert** | `ChatTranslate` `/translate`, `MediaConverter` `/convert` | catalyrst-media · `/translate` | works | `/translate` shape `match`, Postgres-backed cache. |
| 26 | **Price / MANA-USD** | `ManaUsdRateApiUrl` (coingecko) | catalyrst-price · `/api/v3/simple/price` | works | Shape `match`, single indexed SQL. |
| 27 | **Eth JSON-RPC (wallet/contract reads)** | `ApiRpc` ws, `rpc.decentraland.org` POST | catalyrst-rpc · `/{network}` | partial | Relay shape matches; only 5 networks configured (mainnet/ethereum/avalanche/binance/fantom). Networks outside that set return `-32602 Unsupported network`. Covers the chains the client's WS path uses. |
| 28 | **Creator / builder** | `BuilderApi*` `/v1/collections/{id}/items`,`/storage/contents`,`/newsletter` | catalyrst-builder · those routes | works | Per-item `FullItem` fields `match`; ignored filter params are explorer-irrelevant. |
| 29 | **Marketplace credits** | `MarketplaceCredits` `/users/{w}/progress`, `/captcha`, claim | catalyrst-credits · `/users/.../progress`; `/captcha` (deferred) | partial | Progress read works (single join). **`POST /captcha` returns 501**, which aborts the claim flow → users can see credit progress but cannot complete a claim. |
| 30 | **Content moderation / report place** | `ContentModerationReport` `/api/report` | catalyrst-places · `/api/report` (501) | missing | Returns **501** vs upstream 200 `{data:{signed_url}}`. Unity `ReportPlaceAsync` reads `data.signed_url` → 501 aborts the report-upload flow (federation stub). |
| 31 | **Favorite / like places & worlds** | places `/api/places/{id}/favorites|likes` (write) | catalyrst-places · those routes (501) | missing | Favorite/like writes return **501** (federation write path pending). Unity callers don't read the body but the 501 makes the request throw → the action fails. |
| 32 | **Scene admin / moderation** | comms `/scene-admin` GET/POST, `/users/{addr}/bans` | catalyrst-comms · `/scene-admin`,`/users/{addr}/bans` | partial | Routes exist and are reachable, but **shapes break the client**: `/scene-admin` GET returns `{data:[...]}` (object envelope, snake_case, missing `name`/`canBeRemoved`) vs upstream bare array → Unity `List<AdminInfo>` deserialization breaks; `/users/{addr}/bans` returns top-level `{banned,...}` vs upstream `{data:{isBanned,ban}}` → `GetBanStatusResponse` reads `data.isBanned` and breaks. Admin/ban-status read paths broken; mint paths function. |
| 33 | **Mutes / block list** | `SocialServiceMutes` GET/POST/DELETE; `Blocklist` denylist.json | catalyrst-social-rpc / social-api · `/v1/mutes`; catalyrst-lists · `/denylist.json` | works | Mutes CRUD + denylist serving present. |

---

## Summary by status (33 capabilities)

- **works (15):** auth/login (#1), realm-about discovery (#2), feature flags (#3),
  POIs (#5), browse worlds (#7), view profile (#10), wearables/emotes/names/lands
  (#12), scene voice (#18), events & RSVP (#21), badges (#22), camera reel (#23),
  translate/media (#25), price (#26), builder (#28), mutes/blocklist (#33).
- **partial (14):** map/tiles (#4), browse places (#6), teleport (#8), scene/AB
  load (#9), edit profile (#11), marketplace browse (#13), friends & presence
  (#14), communities (#15), nearby chat (#16), private voice (#19), notifications
  email sub-flow (#24), eth-rpc (#27), credits (#29), scene-admin (#32).
- **missing (4):** private messages (#17), community voice (#20), report-place
  (#30), favorite/like places (#31).

(Captcha→credit-claim and profile-edit-under-READ_ONLY are folded into the
partial credits/edit-profile rows rather than counted as standalone missing
capabilities.)

## Top blockers (ranked by user-visible impact)

1. **comms private/community voice + private-messages tokens are 501** — three
   distinct social capabilities (DM, 1:1 voice, community voice) are dead; all
   need the LiveKit RoomService REST client + MLS + communities gossip
   (`catalyrst-comms/src/handlers/deferred.rs`).
2. **ab-registry `POST /entities/active|versions` shape breaks AB version pin** —
   flat `versions`/`bundles` vs Unity's `versions.assets.{mac,win}` → optimized
   asset-bundle loading path is broken (scenes fall back to raw content).
3. **catalyrst-comms `/scene-admin` + `/users/{addr}/bans` wire shapes break the
   client** — envelope/casing/missing-field divergences fail Unity
   deserialization for scene moderation + ban-status reads.
4. **catalyrst-communities `/{id}/members` omits profile fields → Unity
   NullReferenceException** (hard crash), plus all community writes 501.
5. **catalyrst-market `/items` and `/trades` are empty stubs; `/nfts` order/rental
   always null** — marketplace discovery shows incomplete/empty listings.
6. **Federation write stubs (501): places report, favorites/likes, captcha credit
   claim, profile edit under READ_ONLY** — every user-write path that crosses the
   federation boundary is still stubbed.
