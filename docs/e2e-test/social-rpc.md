# E2E Test Plan — catalyrst-social-rpc (`social-rpc`)

Reimplementation of `rpc-social-service-ea.decentraland.org` (upstream `social-service-ea`,
SocialService v2 over a single multiplexed **dcl-rpc WebSocket**).

- Crate: `catalyrst-social-rpc`
- Workspace: `<WORKSPACE>`
- Listen: `127.0.0.1:5143` (HTTP + WS upgrade on `/`)
- Unit: `catalyrst-social-rpc.service` (env `<ENV_FILE>`)
- DB: shared `communities` Postgres instance, tables from `migrations/0001_social.sql`
- Gatekeeper: `COMMS_GATEKEEPER_URL=http://127.0.0.1:5138` (catalyrst-comms; voice handlers degrade gracefully)

The service runtime model: every authenticated WS connection is bridged into one shared
`RpcServer` holding the `SocialService` (33 procedures). The WS handshake is **not** dcl-rpc —
it is a signed-fetch auth gate. The very first frame the client sends must be a JSON object of
signed-fetch headers; on success the server replies `{"welcome":"<lowercased address>"}` and then
all subsequent frames are dcl-rpc protocol bytes for the SocialService.

---

## 1. Unity client repoint (host: `ApiFriends`)

**This URL is NOT `/about`-discovered.** It is hard-coded in the Unity URL registry and resolved
directly by `SocialServicesContainer.GetApiUrl()` — editing our realm `/about` response does
**nothing** here. Repoint Unity itself (or pass the CLI flag).

### File + exact line to change

`<UNITY_EXPLORER>/Explorer/Assets/DCL/NetworkDefinitions/Browser/DecentralandUrlsSource.cs`

**Line 208** (inside the `RawUrl(DecentralandUrl url)` switch):

```csharp
DecentralandUrl.ApiFriends => $"wss://rpc-social-service-ea.decentraland.{ENV}",
```

Repoint to your local node (plain `ws://`, no TLS for localhost):

```csharp
DecentralandUrl.ApiFriends => "ws://127.0.0.1:5143",
```

Enum member: `DecentralandUrl.ApiFriends = 47` in
`<UNITY_EXPLORER>/Explorer/Assets/DCL/Infrastructure/Utility/DecentralandUrls/DecentralandUrl.cs:76`.

### Preferred override (no source edit): CLI flag

`SocialServicesContainer.GetApiUrl()` (`.../PluginSystem/Global/SocialServicesContainer.cs:85-93`)
checks `AppArgsFlags.FRIENDS_API_URL` first and uses it verbatim if present. The flag string is
`friends-api-url` (`.../Infrastructure/Global/AppArgs/AppArgsFlags.cs:41`). Launch the client with:

```
--friends-api-url ws://127.0.0.1:5143
```

This is the cleanest repoint — it overrides line 208 without touching the registry.

---

## 2. Prereqs / bring-up

```bash
# Build
cargo build -p catalyrst-social-rpc --release

# Bring the unit up (env file already written)
systemctl --user daemon-reload
systemctl --user start catalyrst-social-rpc.service
systemctl --user status catalyrst-social-rpc.service

# OR run straight from the workspace for iteration:
env $(grep -v '^#' <ENV_FILE> | xargs) \
  cargo run -p catalyrst-social-rpc

# Confirm it's listening
ss -ltnp | grep 5143
```

Tooling for WS checks: `wscat` (`npm i -g wscat`) or `websocat`. JSON shaping via `jq`.

---

## 3. HTTP smoke checks (curl)

These hit the plain-HTTP routes registered in `main.rs` (`/info`, `/health`, `/health/live`).

```bash
# C1 — liveness probe: 200, body exactly "alive"
curl -fsS -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5143/health/live   # expect 200
curl -fsS http://127.0.0.1:5143/health/live                                    # expect: alive

# C2 — health: 200 + {"healthy":true}
curl -fsS http://127.0.0.1:5143/health | jq -e '.healthy == true'              # expect: true

# C3 — service info: 200 + identifies crate + advertises ws "/"
curl -fsS http://127.0.0.1:5143/info \
  | jq -e '.service == "catalyrst-social-rpc" and .ws == "/"'                    # expect: true

# C4 — GET / without Upgrade header is rejected by the WS upgrade extractor
#      (axum WebSocketUpgrade returns 426/400 for a non-upgrade GET — NOT 200)
curl -fsS -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5143/              # expect 426 (or 400)

# C5 — process bound to loopback only (security posture; should NOT answer on the LAN IP)
curl -fsS -o /dev/null -w '%{http_code}\n' --max-time 3 http://0.0.0.0:5143/health || echo "refused-as-expected"
```

## 4. WebSocket handshake checks (wscat / websocat)

The WS auth gate (see `src/ws.rs` + `src/auth_chain.rs`):
- First frame must be a JSON object of signed-fetch headers:
  `x-identity-auth-chain-0`, `x-identity-auth-chain-1`, ... (each a JSON-stringified auth link),
  `x-identity-timestamp`, `x-identity-metadata`.
- Canonical signed payload = `"get:/:<timestamp>:<metadata>"` **lowercased**.
- Success → server sends `{"welcome":"0x<lowercased signer>"}` and keeps the socket open.
- Failure/timeout → socket closed with **code 3003** ("Unauthorized").

```bash
# C6 — connect with NO auth frame, then wait: server must close with 3003 after AUTH_WINDOW_SECS.
#      (Use a tiny window via env override AUTH_WINDOW_SECS=3 when iterating to keep the test fast.)
websocat -E ws://127.0.0.1:5143/ </dev/null
# expect: connection closes; close code 3003 / reason "Unauthorized"

# C7 — send a garbage first frame (not valid JSON auth headers): immediate close 3003.
printf 'not-an-auth-chain' | websocat -1 ws://127.0.0.1:5143/
# expect: close 3003 Unauthorized

# C8 — send a structurally-valid-but-expired/forged auth object: close 3003
#      (timestamp outside the 300s window OR signature that doesn't recover the SIGNER).
cat > /tmp/forged_auth.json <<'JSON'
{"x-identity-auth-chain-0":"{\"type\":\"SIGNER\",\"payload\":\"0x0000000000000000000000000000000000000000\",\"signature\":\"\"}","x-identity-auth-chain-1":"{\"type\":\"ECDSA_EPHEMERAL\",\"payload\":\"...\",\"signature\":\"0xdead\"}","x-identity-timestamp":"1","x-identity-metadata":"{}"}
JSON
websocat -1 ws://127.0.0.1:5143/ < /tmp/forged_auth.json
# expect: close 3003 Unauthorized (InsufficientLinks/expired/bad-recover)

# C9 — HAPPY PATH: a real signed-fetch identity. Easiest source of a valid auth chain is a
#      dcl identity created by the client/SDK. Generate the first-frame JSON with the helper
#      from catalyrst-fed / catalyrst-crypto test fixtures (same EIP-712 + ephemeral key path
#      the explorer uses), addressed to method=get path=/ metadata={}.
#      Then:
node <WORKSPACE>/scripts/sign-fetch-frame.mjs get / '{}' > /tmp/auth_frame.json  # (write this helper if absent)
websocat -n ws://127.0.0.1:5143/ < /tmp/auth_frame.json
# expect: first server frame is {"welcome":"0x<your lowercased address>"} and socket stays open.

# C10 — DB-backed round trip over dcl-rpc (after C9 welcome): drive a GetFriends + UpsertFriendship.
#       dcl-rpc frames are length-prefixed protobuf, not hand-typable — use the typed client.
#       See section 5 (the bevy/Unity smoke is the realistic integration driver for this).
```

### dcl-rpc procedure-level assertions (run via a typed client harness)

Once the welcome frame is received, exercise the SocialService procedures. Recommended: a small
Rust integration test that reuses the generated `proto_gen` client against `ws://127.0.0.1:5143/`,
or the real explorer (section 5). Assertions to cover:

- `GetFriends` for a fresh identity → empty page (`users: []`, sane pagination total `0`).
- `UpsertFriendship{REQUEST -> A->B}` → returns accepted/pending status; a row appears in
  `friendships` + an append in `friendship_actions`; `SubscribeToFriendshipUpdates` on B's
  connection receives a `FriendshipUpdate` (request).
- `UpsertFriendship{ACCEPT}` from B → status becomes friends; both `GetFriends` now include each other.
- Illegal transition (e.g. ACCEPT when no request) → rejected by the state-machine validator
  (ported `isUserActionValid`), no DB mutation.
- `BlockUser(B)` from A → `blocks` row inserted, a block `friendship_actions` append, B receives a
  `BlockUpdate` (and a `FriendshipUpdate.block`); `GetBlockedUsers(A)` lists B;
  `GetFriendshipStatus(A,B)` reflects blocked.
- `UnblockUser(B)` → `BlockUpdate{is_blocked:false}`; status clears.
- `GetSocialSettings` defaults, then `UpsertSocialSettings` round-trips; `GetPrivateMessagesSettings`
  reflects it.
- `StartPrivateVoiceChat(A->B)` → one-active-per-user enforced (second Start while one is active is
  rejected); B's `SubscribeToPrivateVoiceChatUpdates` gets a `REQUESTED` push;
  `GetIncomingPrivateVoiceChatRequest(B)` returns it; `AcceptPrivateVoiceChat` → `ACCEPTED` push +
  gatekeeper creds (deterministic fallback connection_url is acceptable while comms-gatekeeper 501s).
- Community voice authz: `StartCommunityVoiceChat` succeeds only for a `community_members.role` of
  owner/moderator; `JoinCommunityVoiceChat` requires membership; a non-member is rejected. Seed a
  `community_members` row first (it lives in the same shared DB).

### Expected-deferred (document, assert "does not crash", do NOT fail the suite)

Per the impl summary, these are stubbed/idle and must degrade gracefully, not error the connection:
- Cross-node federation fan-out for all 6 subscription streams (single-node in-process broadcast).
- `SubscribeToFriendConnectivityUpdates` / `SubscribeToCommunityMemberConnectivityUpdates` — streams
  open and stay idle (no presence feed yet). Assert: subscribe returns a live stream, no error close.
- `FriendProfile` hydration — addresses only when `PROFILE_API_URL` unset. Assert: name/picture empty,
  address present and lowercased.
- LiveKit credential minting + server-side mute/kick/promote/demote — delegated to catalyrst-comms
  gatekeeper which 501s; `gatekeeper.rs` treats 404/501 as Ok and returns a deterministic fallback
  `connection_url`. Assert: voice procedures return Ok with a connection_url, no 5xx surfaced to client.

## 5. Real-client smoke (dcl-bevy / dcl-walk)

`ApiFriends` is consumed only by the **Unity** explorer (`SocialServicesContainer`). bevy-explorer
does not currently wire this SocialService, so the authoritative real-client smoke is **dcl-walk**
(upstream Unity client). Use the `dcl-explore` skill for headless driving.

```bash
# Launch the upstream Unity client pointed at your local social-rpc via the CLI override (no source edit):
dcl-walk launch --friends-api-url ws://127.0.0.1:5143
dcl-walk auth-sign          # complete signed login (produces the real auth chain used in the WS handshake)

# Then, with the service logs tailing:
journalctl --user -u catalyrst-social-rpc.service -f
# Expect on connect:
#   "social-rpc client authenticated" with the same address dcl-walk authed as
#   a {"welcome":...} ack, NO close 3003
# In-client: open the Friends / social panel; expect the friends list to load (empty for a fresh
#   account) without a connection-error toast. Send a friend request to a seeded second account and
#   confirm it lands (FriendshipUpdate) — cross-check the friendships table:
psql "postgresql:///communities?host=<SOCKET_DIR>&port=5433" \
  -c "select * from friendships order by created_at desc limit 5;"
```

If editing the registry instead of using the flag, rebuild the Unity client after changing line 208.

## 6. DB / migration verification

```bash
PSQL='psql postgresql:///communities?host=<SOCKET_DIR>&port=5433&user=<DB_USER>'
# Tables created by 0001_social.sql exist in the shared communities DB:
$PSQL -c "\dt" | grep -E 'friendships|friendship_actions|blocks|social_settings|user_mutes|private_voice_chats'
# CHECK/index parity with upstream node-pg-migrate (spot-check a couple):
$PSQL -c "\d friendships"
$PSQL -c "\d friendship_actions"
```

## 7. Pass/fail summary

PASS when: C1-C5 HTTP shapes match; C6-C8 yield close 3003; C9 yields a `{"welcome":...}` ack with the
correct lowercased address; the dcl-rpc procedure assertions in §4 hold (friend state machine,
blocks, settings, private-voice one-active rule, community-voice authz) with corresponding DB rows;
deferred features degrade gracefully (no 5xx, no unexpected close); and the dcl-walk smoke connects,
authenticates, and loads the friends panel without a connection error.
