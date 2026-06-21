# catalyrst-comms - v1 scope + deferred work

## Implemented in v1

System
- `GET /ping`, `GET /status`, `POST /livekit-webhook` (signature verified when `LIVEKIT_WEBHOOK_KEY` set; otherwise accept-and-log)

Scene communication (hot path)
- `POST /get-scene-adapter` - auth-chain optional in v1 (also accepts `identity` in body), runs scene-ban + user-ban check, mints HS256 LiveKit JWT, returns `{ adapter: "livekit:wss://...?access_token=..." }`
- `POST /get-server-scene-adapter` - same flow with `server:` identity prefix
- `GET /scene-participants` - LiveKit RoomService `ListParticipants` (twirp POST with an HS256 roomAdmin/roomList admin JWT for the target room). Response `{ok:true,data:{addresses:[...]}}` matches upstream `getSceneParticipantsHandler`; world+pointer resolves per-pointer via the worlds content server `POST /world/{name}/scenes` (upstream `worlds.fetchWorldSceneByPointer`), unresolvable pointers 404 with upstream's messages, LiveKit failures degrade to an empty roster (upstream `listRoomParticipants` catch). Local room names stay the crate's own (`scene:{sceneId}` without realm, `world-{name}-{sceneId}`) so the roster reads the same rooms `/get-scene-adapter` mints tokens for.

Scene admin (DB-backed)
- `GET /scene-admin?place_id=...`, `POST /scene-admin`, `DELETE /scene-admin?place_id=...&admin=...`

Scene bans (DB-backed)
- `GET /scene-bans?place_id=...`, `GET /scene-bans/addresses?place_id=...`, `POST /scene-bans`, `DELETE /scene-bans?place_id=...&banned_address=...`

User moderation (DB-backed, moderator-gated)
- `GET /users/{address}/bans` (public, no auth) - `{data:{isBanned,ban?}}`
- `POST /users/{address}/bans` - 201 `{data:UserBan}`; 409 when already banned
- `DELETE /users/{address}/bans` - 204; 404 when no active ban
- `GET /users/{address}/warnings` - 200 `{data:[UserWarning]}` (moderator read); `POST` - 201 `{data:UserWarning}`
- `GET /bans` - 200 `{data:[UserBan]}` (active bans)
- Moderator authz (`moderator.rs`): Bearer `MODERATOR_TOKEN` (with sanitized `?moderator=` name for writes) OR signed-fetch signer in `PLATFORM_USER_MODERATORS` allowlist. Mirrors upstream `moderatorAuthMiddleware`.
- Upstream's blockchain side-effects (LiveKit `removeParticipantFromAllRooms`, moderation-event publish to the message bus) are intentionally omitted - the ban row is the source of truth the scene-adapter hot path reads.

Private messages (LiveKit transport)
- `GET /private-messages/token` - mints a `private-messages` room JWT keyed to the signed identity (canSubscribe + canPublishData, no audio publish), returns `{ adapter: "livekit:..." }`; privacy metadata from the `private_messages_privacy` table is embedded in the token.
- `PATCH /users/{address}/private-messages-privacy`

## MLS messaging delivery service (RFC 9420) - `src/mls.rs`, `src/handlers/messaging.rs`, `migrations/0004_mls_messaging.sql`

Server delivery-service is implemented (see `docs/federation/messaging.md`); the remaining work is the client (explorer) MLS stack. This catalyst is the MLS delivery service, NOT a group member: it distributes KeyPackages, routes opaque Welcome/Commit/application ciphertext, persists encrypted history, and serialises epoch advances; it holds no group secret and cannot decrypt. Built on `openmls` 0.6 (ADR-pinned `0.6.0 <= openmls < 0.7.0`); the server parses only the public MLS framing (ciphersuite, group_id, epoch) and cryptographically validates published KeyPackages (`KeyPackageIn::validate`) - it never constructs a group. Ciphersuite pinned to `MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519` (RFC 9420 id `0x0001`) - `mls::PINNED_CIPHERSUITE`; protocol version MLS 1.0.

Endpoints (all signed-fetch authed via the `x-identity-auth-chain-*` headers, `catalyrst-crypto`):
- `POST /mls/key-packages` - publish one-time KeyPackages `{ key_packages: [<b64 MLSMessage>] }`; credential identity must match the authed wallet.
- `GET  /mls/key-packages/{owner}` - claim ONE unconsumed KP (single-use, atomic).
- `GET  /mls/key-packages/{owner}/count` - remaining KP count (replenish signal).
- `POST /mls/groups` - register a group `{ group_id, group_kind, initial_members, initial_commit?, welcome? }`; creator becomes epoch-author.
- `POST /mls/groups/{id}/commits` - submit an epoch+1 commit (must hit the epoch-author catalyst; serialised under a row lock) `{ epoch, commit, welcome?, added_members[], removed_members[] }`.
- `GET  /mls/groups/{id}/commits?from=N` - catch-up handshake fetch (members only).
- `POST /mls/groups/{id}/messages` - submit application ciphertext `{ ciphertext: <b64 MLSMessage> }`; group_id/epoch framing-checked, content-addressed + deduped (members only).
- `GET  /mls/groups/{id}/messages?before&limit` - encrypted history, newest-first (members only).
- `GET  /mls/blobs/{hash}` - single ciphertext blob by content hash (must be a member of a referencing group).

Authz: history/commit/blob fetch require current membership in `mls_group_members`; KP publish is signer-bound; KP fetch is any authed wallet.

### Client contract for hand-off (unity-v3 explorer MLS stack)

The explorer's chat surface (`Explorer/Assets/DCL/Chat/*`) sends cleartext over the LiveKit data channel and only AES-encrypts local history (`ChatHistoryEncryptor.cs`, ECB). It is NOT MLS-ready. To interoperate, the client MUST:

1. Use an MLS library at ciphersuite `MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519` (`0x0001`), MLS 1.0. Credential = `BasicCredential`, identity bytes = lowercase wallet address (UTF-8). The server rejects any other ciphersuite and binds the published credential to the authed wallet.
2. Wire all payloads as TLS-serialised `MLSMessage` (the openmls `MlsMessageOut` wire form): KeyPackage, Welcome, Commit (PrivateMessage/PublicMessage handshake), application PrivateMessage. Base64 (standard, padded) for the JSON bodies above; raw `MLSMessage` bytes on the LiveKit data channel for live delivery.
3. Publish a batch of one-time KeyPackages on login; replenish when `/mls/key-packages/{me}/count` runs low.
4. DM = MLS group of size 2; community/world channel = size N. To start a DM/add a member: claim the peer's KeyPackage (`GET /mls/key-packages/{peer}`), build the group locally, `POST /mls/groups` (DM) or `POST .../commits` (add), and deliver the Welcome to the new member (stored in `mls_commits.welcome_bytes` and returned by the commits fetch).
5. Send a message by encrypting locally to the current epoch and POSTing the ciphertext; for live chat also publish the same bytes on the LiveKit data channel. On (re)join, pull `/mls/groups/{id}/commits?from=<local_epoch>` to catch the tree up, then `/mls/groups/{id}/messages` for history, decrypting each blob locally.
6. Epoch authority: send all add/remove/update commits to the group's epoch-author catalyst (the creator's home; `epoch_author` in the create response). The server serialises them and rejects out-of-order epochs (409).

Server-side federation gossip (NATS `MessageRef`/`GroupCommit` fan-out across catalysts) is the next step - marked with a `NOTE(federation)` in `send_message`; single-node is correct as-is (LiveKit handles live delivery, the DB rows are durable history).

Private voice chat (1:1)
- `POST /private-voice-chat` - mints publish/subscribe tokens for the listed addresses in `voice-chat-private-<room_id>`, records presence in `voice_chat_users`.
- `GET /users/{address}/voice-chat-status`
- `DELETE /private-voice-chat/{id}` - clears presence + deletes the LiveKit room.

Community voice chat
- `POST /community-voice-chat` - create-or-join; `action=create` + moderator/owner role grants publish (speaker), everyone else joins muted-listener; metadata carries role/isSpeaker/muted + profile fields.
- `GET/POST /community-voice-chat/{id}/status` and `/community-voice-chat/status`; `GET /community-voice-chat/active`; `DELETE /community-voice-chat/{id}`
- Per-user sub-routes (POST/DELETE speak-request, POST/DELETE speaker, PATCH mute, DELETE kick) - mutate LiveKit participant state via RoomService `UpdateParticipant` (metadata read-modify-write) / `RemoveParticipant`, matching upstream `voice.ts`.
- Room ownership / moderator-role enforcement is delegated to catalyrst-social-rpc (gates these calls against community membership before the room-access authorization layer). Federation gossip for room<->community ownership is still TODO.

## Deferred to v2 (handlers return 501 with structured `deferred: true` body)

Cast 2.0 (RTMP / ingress)
- `/cast/*` family
- `PUT/DELETE /scene-stream-access`
- Needs: LiveKit IngressClient (RTMP ingress create/delete), background expiration checker, presenter management against room metadata. Large surface - split out into its own crate or `cast` module once the LiveKit REST client lands.

## Known limitations of the v1 hot path

- `place_id` for the hot-path ban check resolves world-name -> scene content-hash via the worlds content server (`fetch_world_scene_id`, mirroring upstream `worlds.fetchWorldSceneId`) in both `POST /get-scene-adapter` (1696077) and `GET /worlds/{world}/parcels/{baseParcel}/users/{address}/ban-status` (e39c507), so bans/rooms key on the resolved scene identity. Still keyed on the world's *first* deployed scene (`scenesUrn[0]`) rather than the parcel-specific scene - full per-parcel scene selection needs the Places API resolution upstream does via `getWorldScenePlace(realmName, parcel)`. The scene-ban *listing* path (`GET /scene-bans` + `GET /scene-bans/addresses`) now keys identically: `resolve_listing_place_id` mirrors the hot path (metadata `sceneId` hash wins; `.eth` candidates - from metadata realm or an explicit `?place_id=` - resolve through `fetch_world_scene_id`, 400 when unresolvable). No SQL data migration ships for legacy realm-keyed `scene_bans` rows: the name->hash mapping needs the live worlds content server and drifts on every world redeploy, and such rows were never read by the enforcement path. Audit with `SELECT place_id, count(*) FROM scene_bans WHERE place_id ILIKE '%.eth' GROUP BY 1;` and re-issue those bans through the API if any turn up. `GET /scene-admin` still lists via `place_from_metadata` (realm-name keying for worlds); the ban write paths (`POST`/`DELETE /scene-bans`) still trust the caller's `place_id` verbatim - migrating either means re-keying the `scene_admin` authz rows the ban writes check, so land them together.
- No platform-wide denylist check yet (upstream pulls a JSON denylist). The `user_bans` table covers the same moderation surface for now; the JSON denylist fold-in is a separate task.
- Room-metadata sync ships for bans + admins (`room_metadata_sync::{add,remove}_{ban,admin}` wired from the scene-ban/scene-admin write paths); presenter add is the remaining piece - land alongside Cast 2.0.
