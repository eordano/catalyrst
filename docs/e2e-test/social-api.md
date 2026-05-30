# E2E test plan — `social-api` (catalyrst-communities)

Future end-to-end test plan for the catalyrst reimplementation of
`social-api.decentraland.org`.

| | |
|---|---|
| key | `social-api` |
| crate | `catalyrst-communities` |
| workspace | `<WORKSPACE>` |
| port | **`5136`** (`HTTP_SERVER_PORT`, default in `config.rs`) |
| host | `127.0.0.1` (`HTTP_SERVER_HOST`) |
| shared DB | yes — `COMMUNITIES_PG_CONNECTION_STRING` pointing at a PostgreSQL instance (`communities` DB) |
| build | `cargo check -p catalyrst-communities --tests --examples` |

Base URL used below: `http://127.0.0.1:5136`

---

## 1. Unity config — how to repoint this host

The explorer resolves every `social-api.decentraland.org` URL through the
`RawUrl(...)` switch in:

`Explorer/Assets/DCL/NetworkDefinitions/Browser/DecentralandUrlsSource.cs`

Enum values live in:

`Explorer/Assets/DCL/Infrastructure/Utility/DecentralandUrls/DecentralandUrl.cs`
(`Communities = 61`, `CommunityThumbnail = 62`, `Members = 63`, `ActiveCommunityVoiceChats = 67`, `SocialServiceMutes = 87`)

These are **NOT** `/about`-discovered. `social-api` is not part of the realm
`/about` response (only `content`, `lambdas`, `comms`/adapter and archipelago
come from `/about`). All five enums are hard-coded templates in `RawUrl(...)`
keyed off `{ENV}` (replaced with the environment domain, e.g. `org`/`zone`).
**Repointing is done by editing `DecentralandUrlsSource.cs`, not by editing
our `/about` response.**

Exact lines to change in `RawUrl(...)` (line numbers as of this read):

```
221  DecentralandUrl.Communities                => $"https://social-api.decentraland.{ENV}/v1/communities",
222  DecentralandUrl.CommunityThumbnail         => $"https://assets-cdn.decentraland.{ENV}/social/communities/{{0}}/raw-thumbnail.png",
223  DecentralandUrl.Members                    => $"https://social-api.decentraland.{ENV}/v1/members",
227  DecentralandUrl.ActiveCommunityVoiceChats  => $"https://social-api.decentraland.{ENV}/v1/community-voice-chats/active",
255  DecentralandUrl.SocialServiceMutes         => $"https://social-api.decentraland.{ENV}/v1/mutes",
```

Repoint them to the local service (drop `https`/`{ENV}` and point at
`http://127.0.0.1:5136`, keeping the path suffix each handler serves):

```csharp
DecentralandUrl.Communities                => "http://127.0.0.1:5136/v1/communities",
DecentralandUrl.Members                    => "http://127.0.0.1:5136/v1/members",
DecentralandUrl.ActiveCommunityVoiceChats  => "http://127.0.0.1:5136/v1/community-voice-chats/active",
DecentralandUrl.SocialServiceMutes         => "http://127.0.0.1:5136/v1/mutes",
```

`CommunityThumbnail` is special: the explorer points it at the **assets-cdn**
host, not `social-api`. Our thumbnail bytes are served by
`catalyrst-communities` at `GET /social/communities/{id}/raw-thumbnail.png`.
Two ways to repoint:

- Direct (simplest for a local setup): point the enum at the service directly —
  ```csharp
  DecentralandUrl.CommunityThumbnail => "http://127.0.0.1:5136/social/communities/{0}/raw-thumbnail.png",
  ```
- CDN-faithful: keep the assets-cdn shape and front
  `/social/communities/{id}/raw-thumbnail.png` with a reverse proxy serving the
  assets-cdn host, then set `ASSETS_CDN_BASE_URL` on the service so emitted
  `communityImage` values match (see `community_thumbnail_url()` in
  `config.rs`). The enum value stays
  `https://assets-cdn.decentraland.{ENV}/social/communities/{0}/raw-thumbnail.png`
  and only the realm/reverse-proxy routing changes.

> Note: `{ENV}` is `STATIC` for these (cached, not realm-dependent), so the
> change takes effect on next client start. `{0}` is the community UUID,
> filled by the caller. The switch throws `ArgumentOutOfRangeException` for
> unhandled enums, so do not delete the arms — only edit the RHS.

---

## 2. Concrete e2e checks (curl / wscat)

Prereqs: service running on `5136` with the database reachable. Start it via
its environment file (`<ENV_FILE>`):

```bash
cd <WORKSPACE>
set -a; source <ENV_FILE>; set +a
cargo run -p catalyrst-communities
```

### 2.0 Liveness

```bash
curl -fsS -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5136/ping
# expect: 200
```

### 2.1 Active community voice chats (DTO is the load-bearing fix)

```bash
curl -fsS http://127.0.0.1:5136/v1/community-voice-chats/active | jq .
```

Expect: **200**. Body is the Unity `ActiveCommunityVoiceChat.cs` shape — a JSON
array (or `{data:[...]}` envelope, confirm against handler) where each element
has exactly:
`communityId`, `communityName`, `communityImage` (optional / nullable),
`isMember` (bool), `positions` (string[]), `worlds` (string[]),
`participantCount` (int), `moderatorCount` (int).

Steady-state empty table returns `200` with an empty array — this is the
accepted resting state (`community_voice_chats` rows are upserted by
`catalyrst-comms` / archipelago gossip). Assert: HTTP 200, body parses
as JSON array, and when non-empty every element carries all eight fields with
correct types (`communityImage` may be null/absent; `positions`/`worlds` are
arrays not null).

### 2.2 Community thumbnail (public, unauthenticated)

```bash
# Missing thumbnail -> 404
curl -s -o /dev/null -w '%{http_code}\n' \
  http://127.0.0.1:5136/social/communities/00000000-0000-0000-0000-000000000000/raw-thumbnail.png
# expect: 404

# Present thumbnail (substitute a community id whose communities.thumbnail_hash
# is set and whose blob exists in content_store) -> 200 image/png
curl -s -D - -o /tmp/thumb.png \
  http://127.0.0.1:5136/social/communities/<REAL_ID>/raw-thumbnail.png \
  | grep -i '^content-type'
# expect: HTTP 200, Content-Type: image/png, /tmp/thumb.png is a valid PNG
file /tmp/thumb.png   # -> "PNG image data"
```

### 2.3 Communities listing exposes `thumbnails.raw`

```bash
curl -fsS 'http://127.0.0.1:5136/v1/communities?limit=5' | jq '.data[0].thumbnails'
# expect: 200; each community object has thumbnails.raw =
#   "<ASSETS_CDN_BASE_URL>/social/communities/{id}/raw-thumbnail.png"

# voice-chat filter still joins community_voice_chats
curl -s -o /dev/null -w '%{http_code}\n' \
  'http://127.0.0.1:5136/v1/communities?onlyWithActiveVoiceChat=true'
# expect: 200 (empty list acceptable when no active chats)
```

### 2.4 Single community exposes `thumbnails.raw`

```bash
curl -fsS http://127.0.0.1:5136/v1/communities/<REAL_ID> | jq '.thumbnails.raw'
# expect: 200; thumbnails.raw present.
# Unknown id:
curl -s -o /dev/null -w '%{http_code}\n' \
  http://127.0.0.1:5136/v1/communities/00000000-0000-0000-0000-000000000000
# expect: 404
```

### 2.5 Mutes — signed-fetch auth (`x-identity-*` headers)

The mutes routes call `require_signer(headers, method, path)`. Auth is the
catalyrst signed-fetch convention (see `src/auth_chain.rs`):
`x-identity-auth-chain-0..N`, `x-identity-timestamp`, `x-identity-metadata`,
with the signed payload = `"{method}:{path}:{timestamp}:{metadata}"`
lowercased.

**Negative (unauthenticated) — assert rejection:**

```bash
# GET without identity headers
curl -s -o /dev/null -w '%{http_code}\n' 'http://127.0.0.1:5136/v1/mutes?limit=10&offset=0'
# expect: 401 (missing auth chain)

# POST without identity headers
curl -s -o /dev/null -w '%{http_code}\n' -X POST \
  -H 'content-type: application/json' \
  -d '{"wallet":"0x0000000000000000000000000000000000000001"}' \
  http://127.0.0.1:5136/v1/mutes
# expect: 401

# DELETE without identity headers
curl -s -o /dev/null -w '%{http_code}\n' -X DELETE \
  -H 'content-type: application/json' \
  -d '{"wallet":"0x0000000000000000000000000000000000000001"}' \
  http://127.0.0.1:5136/v1/mutes
# expect: 401
```

**Positive (signed) — use the project signer to produce valid headers.**
Generate a signed-fetch request with a test wallet (reuse the
`catalyrst-crypto` signer / `examples/smoke_create.rs` plumbing or
`dcl-walk auth-sign`), then:

```bash
# GET /v1/mutes  (payload signs "get:/v1/mutes:<ts>:<metadata>")
curl -fsS \
  -H "x-identity-auth-chain-0: $LINK0" \
  -H "x-identity-auth-chain-1: $LINK1" \
  -H "x-identity-timestamp: $TS" \
  -H "x-identity-metadata: {}" \
  'http://127.0.0.1:5136/v1/mutes?limit=10&offset=0' | jq .
# expect: 200, paginated list of muted wallets (empty array for a fresh signer)

# POST /v1/mutes  (MuteRequestBody{wallet})
curl -fsS -X POST \
  -H "x-identity-auth-chain-0: $LINK0" -H "x-identity-auth-chain-1: $LINK1" \
  -H "x-identity-timestamp: $TS" -H "x-identity-metadata: {}" \
  -H 'content-type: application/json' \
  -d '{"wallet":"0x000000000000000000000000000000000000dEaD"}' \
  http://127.0.0.1:5136/v1/mutes
# expect: 200/204; afterwards GET returns the muted wallet

# DELETE /v1/mutes (idempotent un-mute)
curl -fsS -X DELETE \
  -H "x-identity-auth-chain-0: $LINK0" -H "x-identity-auth-chain-1: $LINK1" \
  -H "x-identity-timestamp: $TS" -H "x-identity-metadata: {}" \
  -H 'content-type: application/json' \
  -d '{"wallet":"0x000000000000000000000000000000000000dEaD"}' \
  http://127.0.0.1:5136/v1/mutes
# expect: 200/204; afterwards GET no longer lists the wallet
```

> The signed payload path/method must match the route exactly
> (`get`/`post`/`delete` + `/v1/mutes`), or `require_signer` rejects it. A
> stale timestamp or a payload signed for the wrong method/path is the most
> likely cause of a 401 on an otherwise valid chain — assert that flipping the
> method in the signed payload yields 401.

### 2.6 Thumbnail write path (federation, signed)

Out of band of the explorer but part of the lane: PNG bytes upload via
`POST /federation/communities/content`, then a signed `CommunityCreate` /
`CommunityUpdate` carries `thumbnail_content_hash` (persisted to
`communities.thumbnail_hash`). Cover with the crate's own
`tests/content_store.rs` and `tests/federation_smoke.rs`; assert that after a
create carrying a `thumbnail_content_hash`, §2.2 returns the PNG and §2.3/§2.4
surface `thumbnails.raw`, and that GC
(`POST /federation/communities/content/gc`) keeps the referenced
`thumbnail_hash` blob (referenced-set was extended to include it).

### 2.7 No `wscat` needed here

`social-api`'s explorer surface is plain HTTPS REST — none of the five enums
are WebSocket. (`ApiFriends` is the `wss://rpc-social-service-ea` socket and is
a different lane.) No wscat step.

---

## 3. Real-client smoke step

Use **dcl-bevy** or **dcl-walk** after repointing the enums in §1 and
rebuilding/relaunching the client.

1. Repoint the four `social-api` enums (and optionally `CommunityThumbnail`)
   in `DecentralandUrlsSource.cs` to `http://127.0.0.1:5136/...` per §1.
2. Start the service on `5136` (§2). Seed at least one community row (via the
   federation create path, §2.6) with a thumbnail so the UI has something to
   render.
3. Launch a client:
   - `dcl-bevy up` (native) — then open the Communities panel.
   - or `dcl-walk launch` + `dcl-walk auth-sign` (upstream Unity refclient; see
     the `dcl-explore` skill) — the signed identity from `auth-sign` is what
     produces the `x-identity-*` headers the mutes routes need, so this client
     is the right one for exercising §2.5 end to end.
4. Observe / assert:
   - Communities list loads (request hits `GET 127.0.0.1:5136/v1/communities`,
     returns 200, `thumbnails.raw` resolves to a 200 PNG).
   - Community thumbnails render (the `CommunityThumbnail` URL returns the PNG;
     404 communities show the placeholder, not an error).
   - Active community voice-chat indicator: with an empty
     `community_voice_chats` table the panel shows "no active voice chats"
     (200 empty array) rather than erroring — the DTO fix is what keeps the
     client from throwing on this response.
   - Mute/unmute a user from the UI and confirm the signed `POST`/`DELETE`
     `/v1/mutes` round-trips (200) and the mute persists across a list refresh.
5. Capture a screenshot (`dcl-bevy`/`dcl-walk` shot verb) of the Communities
   panel as the smoke artifact.

Pass criteria: no client-side deserialization errors against any of the five
endpoints, thumbnails render, voice-chat indicator handles the empty case, and
a signed mute round-trips.
