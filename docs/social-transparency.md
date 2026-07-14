# Social-surface transparency vs upstream

> Status: verified live 2026-07-31 with `social-probe` against this node
> (social-rpc WS + communities REST) and the production upstream
> (`wss://rpc-social-service-ea.decentraland.org`,
> `https://social-api.decentraland.org`), using two funded-nothing test
> identities. Re-run the commands at the bottom to refresh any claim here.
> The reusable core of steps 1-4 below now exists as the crate's `upstream`
> module -- compiling and unit-tested, not yet wired into the live request
> path; see "Implementation status & wiring plan" at the end.

The question under test: can this node sit between one trusted user's client
and the upstream social service as a transparent middlebox -- showing the
client the same data the upstream would (down), and showing the upstream the
same writes a direct client would produce (up)?

## Verdict

| Direction | Surface | Verdict |
|---|---|---|
| client -> this node | social-rpc WS (friends/requests/blocks/settings) | wire-compatible -- stock auth chain + full bootstrap sequence served identically to upstream; defects below |
| client -> this node | communities REST | shape-compatible -- same envelope; `voiceChatStatus` defect; data is a stale seed, not a mirror |
| this node -> upstream | everything | absent by design -- no write-back, no read-through; local and upstream state diverge silently |

The same probe binary -- built only from this workspace's generated protos and
auth-chain code -- completes the full client bootstrap against both this node
and the production upstream, proving protocol-level client compatibility in
both A/B directions.

## Down-direction defects (client sees different data here)

1. `voiceChatStatus` is `null` here, an object upstream. Upstream returns
   `{isActive, participantCount, moderatorCount}` on `/v1/communities` even
   for idle communities; this node returns `null`. A client dereferencing
   `.isActive` breaks only on this node.
2. Seeded data drifts. The mirrored community rows are one-time copies:
   during verification the same community showed 1271 members upstream vs 2
   here, an outdated description, and 180 communities upstream vs 2 here.
   Nothing refreshes them; there is no read-through on cache miss.
3. Default social settings diverge. First-touch `GetSocialSettings`
   returns `private_messages_privacy: All` upstream but `OnlyFriends` here --
   same procedure, different default policy for an identical fresh account.
4. Unauthenticated REST over-shares. Upstream computes `role`, `friends`,
   `visibility` only for authenticated callers and omits them otherwise; this
   node returns them (empty/`none`) to anonymous callers too. Additive, but
   an observable fingerprint. This node also always emits `createdAt`,
   `isLive`, `unlisted`, `thumbnailUrl`, which upstream omits -- clients
   ignore unknown fields, so these are cosmetic.
5. UpsertFriendship id instability. The `id` returned by a local
   `UpsertFriendship(request)` differs from the `id` the sent-requests
   listing reports for the same request; upstream keeps them consistent.

## The WS auth path contract (middlebox-relevant)

Both this node and the upstream verify the signed-fetch first frame against
the exact request path -- a frame signed for `/social-rpc` is rejected by
both; `/` is accepted by both (verified live). Stock clients sign the path
of the URL they dial. Consequence: fronting the WS at a rewritten prefix
(`/social-rpc` -> `/`) breaks every stock client's signature. A conforming
edge must serve the social WS at `/` on its own hostname (as upstream does).
The HTTP routes tolerate `x-original-path` (suffix-checked); the WS
handshake does not -- if prefix-fronting is ever required, that tolerance is
the fix.

## Up-direction: structurally absent

[federation.md](./federation.md) is explicit: federation is
catalyst-peer-to-peer and never writes back to Decentraland servers.
Verified consequences, live:

- wallet-1 <-> wallet-2 hold an `ACCEPTED` friendship on this node; the same
  pair upstream reads `status: NONE`, zero friends, zero pending or sent
  requests. The local write is invisible upstream, permanently.
- The gateway vhost serves only what this node implements --
  unmatched routes return 404 rather than proxying upstream.
- There is no outbound social client anywhere in the workspace (no dcl-rpc
  client session, no signed-fetch replay), and signed-fetch is
  request-bound, so verbatim relay of a client's write is impossible by
  design; acting upstream requires holding the user's ephemeral identity.
- Presence aggregation polls archipelago/comms peers and carries no per-user
  or friend-graph state; friends' connectivity derives purely from sockets
  attached to this process, so an upstream friend's online state can never
  reach a client terminated here.

## What single-user transparency would take

The trusted-single-user premise makes the hard part easy: the node may hold
the user's ephemeral auth chain. The shortest path, in dependency order:

1. Upstream session client -- an outbound dcl-rpc WS session to the
   upstream, opened with the user's ephemeral chain (the probe already
   proves the handshake and calls work from this codebase).
2. Read-through + subscribe -- bootstrap friends/requests/settings from
   upstream into the local DB at session start; hold the upstream
   subscription streams open and re-emit updates to locally attached
   clients.
3. Write-through tee -- `UpsertFriendship`, block/unblock, settings
   writes apply locally and replay upstream over the session client;
   upstream failure surfaces to the client (no silent divergence).
4. Communities read-through -- REST cache-miss/staleness fallback to
   `social-api` with the user's signed fetch, replacing the one-time seed.
5. Parity fixes -- emit idle `voiceChatStatus` objects, align default
   settings with upstream, make upsert/listing ids consistent, gate
   `role`/`friends` on auth.

## Repro

```
cargo build -p catalyrst-social-service --features probe --bin social-probe

social-probe ws   --url ws://127.0.0.1:5149/ --key <wallet.key> [--peer 0x..] \
                  [--action request|accept|reject|cancel|delete] [--sign-path /p] [--handshake-only]
social-probe ws   --url wss://rpc-social-service-ea.decentraland.org --key <wallet.key> --peer 0x..
social-probe rest --base http://127.0.0.1:5136 --key <wallet.key> [--raw]
social-probe rest --base https://social-api.decentraland.org --key <wallet.key> [--raw]
```

The WS snapshot runs the stock client bootstrap (GetFriends, pending/sent
requests, GetBlockedUsers, GetSocialSettings, GetFriendshipStatus); `rest`
signs each GET with the same 3-link ephemeral chain and reports status plus
body shape (`--raw` for full bodies).

## Implementation status & wiring plan

The up-direction foundation is implemented as `src/rpc/upstream/` in
`catalyrst-social-service`, behind the new cargo feature `upstream`
(`upstream = ["rpc", "dcl-rpc/client", "dcl-rpc/tungstenite-rustls"]`;
`probe` now builds on top of it). It compiles and its unit tests pass
offline; nothing in the live request path calls it yet.

What the module provides now:

- `UpstreamIdentity` (`identity.rs`) -- the single-user identity. Built
  from the configured root private key; derives a deterministic long-lived
  ephemeral wallet (SHA-256 of the key material under a fixed domain string)
  and signs the 3-link delegation once at construction. Yields signed-fetch
  header sets for any `(method, path)` and the WS auth frame for any signed
  path -- the exact shapes `social-probe` proved live against production.
  Unit tests assert determinism, byte-stability at a fixed timestamp, and
  round-trip acceptance by this crate's own `verify_handshake`, including
  rejection of a path mismatch.
- `UpstreamConfig` (`config.rs`) -- reads `UPSTREAM_SOCIAL_RPC_URL`,
  `UPSTREAM_SOCIAL_API_URL`, `UPSTREAM_SOCIAL_KEY_FILE`. All default unset;
  `from_env()` returns `None` (bridge disabled) unless the key file and at
  least one URL are present. Construction never panics and never touches the
  network or the filesystem; `identity()` reads the key file on demand.
- `UpstreamSession` (`session.rs`) -- outbound dcl-rpc WS session:
  connect, send the auth frame, RPC handshake, `create_port("social")`,
  `load_module("SocialService")`. Exposes typed bootstrap reads
  (`get_friends`, `get_pending_friendship_requests`,
  `get_sent_friendship_requests`, `get_blocked_users`,
  `get_social_settings`, `get_friendship_status`) returning the crate's
  generated proto v2 types, plus the write-tee core:
  `write_through(FriendshipMutation)` / `write_through_payload(UpsertFriendshipPayload)`
  replay a friendship mutation upstream and return the upstream
  `UpsertFriendshipResponse`.
- `UpstreamApi` (`api.rs`) -- reqwest client for `social-api`: signed
  GETs (`get_json`, `get_communities`, `get_community`) using the same
  identity, signing the path without its query string as the probe verified
  upstream accepts.

Wiring plan (the deliberately-not-yet-done part; each hook is a later,
separately reviewable change):

1. Read-through at session bootstrap -- hold one shared
   `UpstreamSession` (and `Arc<UpstreamIdentity>`) in `AppState`
   (`src/rpc/state.rs`), constructed at startup iff
   `UpstreamConfig::from_env()` is `Some`. When the trusted user's client
   attaches (`ws_upgrade` in `src/rpc/ws.rs`, after the gatekeeper accepts
   the signer), refresh friends / pending / sent / blocked / settings from
   the session's bootstrap reads into the local DB (`src/rpc/db.rs`) before
   the local procedures serve them. Step 2's subscription streams
   (`call_server_streams_procedure`) re-emit upstream updates to attached
   clients from the same session.
2. Write-tee -- in `upsert_friendship`
   (`src/rpc/service/server/friends.rs`), after the local apply succeeds,
   call `write_through_payload` with the same decoded payload.
   Apply-local-then-replay: if the upstream replay fails, return the failure
   to the client and mark the local row for reconciliation -- the client must
   never observe a success the upstream did not accept, and the two stores
   must never silently diverge. The same pattern later covers
   block/unblock and settings writes.
3. Communities read-through -- in `get_communities` / the single-community
   read (`src/rest/handlers/communities.rs`), on cache miss or staleness,
   fall back to `UpstreamApi::get_communities` / `get_community`, refresh
   the stored rows, then serve -- replacing the one-time seed.

Identity decision: this is a trusted single-user server, so the server is
configured with the user's own root key and mints its own long-lived
ephemeral delegation -- the live client never forwards its private key, and
signed-fetch is request-bound, so a configured server-held identity is the
only way to act upstream on the user's behalf.

Validation status: identity generation, config gating, and the
write-through payload encodings are unit-tested offline (`cargo test
--features upstream --lib upstream`). Live read verification against
production is gated behind `UPSTREAM_SOCIAL_LIVE_TEST` (unset in CI; reads
only). Live upstream **write** validation is deferred to the user: the
agent harness blocks autonomous writes to the production upstream, so the
first real `write_through` replay must be run by a human with a disposable
identity.
