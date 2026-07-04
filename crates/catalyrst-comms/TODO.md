# catalyrst-comms — v1 scope + deferred work

## Implemented in v1

System
- `GET /ping`
- `GET /status`
- `POST /livekit-webhook` (signature verified when `LIVEKIT_WEBHOOK_KEY` set; otherwise accept-and-log)

Scene communication (hot path)
- `POST /get-scene-adapter` — auth-chain optional in v1 (also accepts `identity` in body), runs scene-ban + user-ban check, mints HS256 LiveKit JWT, returns `{ adapter: "livekit:wss://...?access_token=..." }`
- `POST /get-server-scene-adapter` — same flow with `server:` identity prefix
- `GET /scene-participants` — stub `[]` (TODO: LiveKit RoomService REST)

Scene admin (DB-backed)
- `GET /scene-admin?place_id=...`
- `POST /scene-admin`
- `DELETE /scene-admin?place_id=...&admin=...`

Scene bans (DB-backed)
- `GET /scene-bans?place_id=...`
- `GET /scene-bans/addresses?place_id=...`
- `POST /scene-bans`
- `DELETE /scene-bans?place_id=...&banned_address=...`

User moderation (DB-backed, moderator-gated)
- `GET /users/{address}/bans` (public, no auth) — `{data:{isBanned,ban?}}`
- `POST /users/{address}/bans` — 201 `{data:UserBan}`; 409 when already banned
- `DELETE /users/{address}/bans` — 204; 404 when no active ban
- `GET /users/{address}/warnings` — 200 `{data:[UserWarning]}` (moderator read)
- `POST /users/{address}/warnings` — 201 `{data:UserWarning}`
- `GET /bans` — 200 `{data:[UserBan]}` (active bans)
- Moderator authorization (`moderator.rs`): Bearer `MODERATOR_TOKEN` (with sanitized
  `?moderator=` name for writes) OR signed-fetch signer present in
  `PLATFORM_USER_MODERATORS` allowlist. Mirrors upstream `moderatorAuthMiddleware`.
- The blockchain side-effects upstream layers on (LiveKit `removeParticipantFromAllRooms`,
  moderation-event publish to the message bus) are intentionally omitted — the ban row is
  the source of truth the scene-adapter hot path already reads, so the user ACTION succeeds.

## Implemented in v1 (continued) — LiveKit transport layer

Private messages
- `GET /private-messages/token` — mints a `private-messages` room JWT keyed to the
  signed identity (canSubscribe + canPublishData, no audio publish), returns
  `{ adapter: "livekit:..." }`. Privacy metadata is read from the
  `private_messages_privacy` table and embedded in the token.
- `PATCH /users/{address}/private-messages-privacy`
- **MLS end-to-end encryption — server delivery-service IS implemented** (see below
  and `docs/federation/messaging.md`). The LiveKit data channel carries opaque MLS
  ciphertext between online peers; this service is the key-package directory + history
  + ordering authority. The remaining work is the **client (explorer) MLS stack** — see
  the client contract below.

## MLS messaging delivery service (RFC 9420) — `src/mls.rs`, `src/handlers/messaging.rs`, `migrations/0004_mls_messaging.sql`

This catalyst is the MLS **delivery service**, NOT a group member. It distributes
KeyPackages, routes opaque Welcome/Commit/application ciphertext, persists encrypted
history, and serialises epoch advances. It holds no group secret and **cannot decrypt**.
Built on `openmls` 0.6 (ADR-pinned `0.6.0 <= openmls < 0.7.0`); the server parses only
the public MLS framing (ciphersuite, group_id, epoch) and cryptographically validates
published KeyPackages (`KeyPackageIn::validate`) — it never constructs a group.

Ciphersuite pinned to **`MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519` (RFC 9420 id
`0x0001`)** — `mls::PINNED_CIPHERSUITE`. Protocol version MLS 1.0.

Endpoints (all signed-fetch authed via the `x-identity-auth-chain-*` headers,
`catalyrst-crypto`):
- `POST /mls/key-packages` — publish one-time KeyPackages `{ key_packages: [<b64 MLSMessage>] }`;
  credential identity must match the authed wallet.
- `GET  /mls/key-packages/{owner}` — claim ONE unconsumed KP (single-use, atomic).
- `GET  /mls/key-packages/{owner}/count` — remaining KP count (replenish signal).
- `POST /mls/groups` — register a group `{ group_id, group_kind, initial_members, initial_commit?, welcome? }`;
  creator becomes epoch-author.
- `POST /mls/groups/{id}/commits` — submit an epoch+1 commit (must hit the epoch-author
  catalyst; serialised under a row lock) `{ epoch, commit, welcome?, added_members[], removed_members[] }`.
- `GET  /mls/groups/{id}/commits?from=N` — catch-up handshake fetch (members only).
- `POST /mls/groups/{id}/messages` — submit application ciphertext `{ ciphertext: <b64 MLSMessage> }`;
  group_id/epoch framing-checked, content-addressed + deduped (members only).
- `GET  /mls/groups/{id}/messages?before&limit` — encrypted history, newest-first (members only).
- `GET  /mls/blobs/{hash}` — single ciphertext blob by content hash (must be a member of a
  referencing group).

Authz: history/commit/blob fetch require current membership in `mls_group_members`;
KP publish is signer-bound; KP fetch is any authed wallet.

### Client contract for hand-off (unity-v3 explorer MLS stack)

The explorer's chat surface (`Explorer/Assets/DCL/Chat/*`) today sends cleartext over the
LiveKit data channel and only AES-encrypts local history (`ChatHistoryEncryptor.cs`, ECB).
It is NOT MLS-ready. To interoperate, the client MUST:

1. Use an MLS library configured to ciphersuite `MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519`
   (`0x0001`) and MLS 1.0. The credential is a `BasicCredential` whose identity bytes are the
   lowercase wallet address (UTF-8). The server rejects any other ciphersuite and binds the
   published credential to the authed wallet.
2. Wire all payloads as TLS-serialised `MLSMessage` (the openmls `MlsMessageOut` wire form):
   KeyPackage, Welcome, Commit (PrivateMessage/PublicMessage handshake), and application
   PrivateMessage. Base64 (standard, padded) for the JSON bodies above; raw `MLSMessage`
   bytes on the LiveKit data channel for live delivery.
3. Publish a batch of one-time KeyPackages on login and replenish when
   `/mls/key-packages/{me}/count` runs low.
4. DM = MLS group of size 2; community/world channel = size N. To start a DM/add a member,
   claim the peer's KeyPackage (`GET /mls/key-packages/{peer}`), build the group locally,
   `POST /mls/groups` (DM) or `POST .../commits` (add), and deliver the Welcome to the new
   member (it is stored in `mls_commits.welcome_bytes` and returned by the commits fetch).
5. Send a message by encrypting locally to the current epoch and POSTing the ciphertext;
   for live chat also publish the same bytes on the LiveKit data channel. On (re)join, pull
   `/mls/groups/{id}/commits?from=<local_epoch>` to catch the tree up, then
   `/mls/groups/{id}/messages` for history, decrypting each blob locally.
6. Epoch authority: send all add/remove/update commits to the group's epoch-author catalyst
   (the creator's home; `epoch_author` in the create response). The server serialises them
   and rejects out-of-order epochs (409).

Server-side federation gossip (NATS `MessageRef`/`GroupCommit` fan-out across catalysts) is
the next step — marked with a `NOTE(federation)` in `send_message`; single-node is correct
as-is (LiveKit handles live delivery, the DB rows are durable history).

Private voice chat (1:1)
- `POST /private-voice-chat` — mints publish/subscribe tokens for the listed addresses
  in `voice-chat-private-<room_id>`, records presence in `voice_chat_users`.
- `GET /users/{address}/voice-chat-status`
- `DELETE /private-voice-chat/{id}` — clears presence + deletes the LiveKit room.

Community voice chat
- `POST /community-voice-chat` — create-or-join; `action=create` + moderator/owner role
  grants publish (speaker), everyone else joins muted-listener; metadata carries
  role/isSpeaker/muted + profile fields.
- `GET/POST /community-voice-chat/{id}/status` and `/community-voice-chat/status`
- `GET /community-voice-chat/active`
- `DELETE /community-voice-chat/{id}`
- Per-user sub-routes (POST/DELETE speak-request, POST/DELETE speaker, PATCH mute,
  DELETE kick) — mutate LiveKit participant state via RoomService `UpdateParticipant`
  (metadata read-modify-write) / `RemoveParticipant`, matching upstream `voice.ts`.
- Room ownership / moderator-role enforcement is delegated upstream to
  catalyrst-social-rpc (which gates these calls against community membership before
  hitting the room-access authorization layer). Federation gossip for room↔community
  ownership is still TODO.

## Deferred to v2 (handlers return 501 with structured `deferred: true` body)

Cast 2.0 (RTMP / ingress)
- `/cast/*` family
- `PUT/DELETE /scene-stream-access`
- Needs: LiveKit IngressClient (RTMP ingress create/delete), background expiration checker, presenter management against room metadata. Large surface — split out into its own crate or `cast` module once the LiveKit REST client lands.

## Known limitations of the v1 hot path

- `place_id` for the hot-path ban check now resolves world-name → scene content-hash via the worlds content server (`fetch_world_scene_id`, mirroring upstream `worlds.fetchWorldSceneId`) in both `POST /get-scene-adapter` (1696077) and `GET /worlds/{world}/parcels/{baseParcel}/users/{address}/ban-status` (e39c507), so bans/rooms key on the resolved scene identity. Still keyed on the world's *first* deployed scene (`scenesUrn[0]`) rather than the parcel-specific scene — full per-parcel scene selection needs the Places API resolution upstream does via `getWorldScenePlace(realmName, parcel)`. The scene-ban *listing* path (`GET /scene-bans` via `place_from_metadata`) still keys worlds on `realm_name`; migrate it for full consistency.
- No platform-wide denylist check yet (upstream pulls a JSON denylist). The `user_bans` table covers the same moderation surface for now; the JSON denylist fold-in is a separate task.
- No room-metadata sync (presenter add, banned-address propagation). Land alongside Cast 2.0.
- `GET /scene-participants` returns `[]` — LiveKit RoomService REST client needed.
