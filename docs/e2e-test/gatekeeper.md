# E2E test plan — catalyrst-comms (comms-gatekeeper port)

- **Host key:** `gatekeeper`
- **Upstream:** `https://comms-gatekeeper.decentraland.org`
- **Crate:** `catalyrst-comms` (existing workspace member, DB `comms_gatekeeper` on a shared PostgreSQL instance)
- **Prod port:** `5138` (systemd: `catalyrst-comms.service`)
- **Workspace under test:** `<WORKSPACE>`
- **This lane's change:** Increment 1 — `GET /private-messages/token` made live (signed-fetch AuthChain, deny-list, LiveKit listen-only token). Everything else unchanged (scene-adapter family already shipped; voice/community/cast remain `501`).

---

## 1. Unity config — how to repoint this host

**File:** `<UNITY_EXPLORER>/Explorer/Assets/DCL/NetworkDefinitions/Browser/DecentralandUrlsSource.cs`
**Enum:** `<UNITY_EXPLORER>/Explorer/Assets/DCL/Infrastructure/Utility/DecentralandUrls/DecentralandUrl.cs`

All seven enums for this host derive from one base. The `RawUrl(...)` switch only has a **literal** for the base `Gatekeeper`; the rest are built by string-concat off `Url(DecentralandUrl.Gatekeeper)`:

```
178: DecentralandUrl.Gatekeeper                  => $"https://comms-gatekeeper.decentraland.{ENV}",   <-- the ONLY literal to change
179: DecentralandUrl.GateKeeperSceneAdapter       => $"{Url(DecentralandUrl.Gatekeeper)}/get-scene-adapter",
180: DecentralandUrl.LocalGateKeeperSceneAdapter  => "https://comms-gatekeeper-local.decentraland.org/get-scene-adapter",  (separate literal)
181: DecentralandUrl.ChatAdapter                  => $"{Url(DecentralandUrl.Gatekeeper)}/private-messages/token",   <-- the route this lane lights up
200: DecentralandUrl.GatekeeperStatus             => $"{Url(DecentralandUrl.Gatekeeper)}/status",
234: DecentralandUrl.BannedUsers                  => $"{Url(DecentralandUrl.Gatekeeper)}/users/{{0}}/bans",
235: DecentralandUrl.SceneAdmins                  => $"{Url(DecentralandUrl.Gatekeeper)}/scene-admin",
```

**This host is NOT `/about`-discovered.** The gatekeeper base is a STATIC, hardcoded URL — it does not come from the realm `/about` response, so editing your `/about` does nothing here. Repoint in Unity (one of two ways):

- **Recommended (no recompile): `gatekeeperBaseOverride` ctor arg.** `DecentralandUrlsSource(...)` accepts a `gatekeeperBaseOverride`; when set it does `cache[DecentralandUrl.Gatekeeper] = new UrlData(STATIC, gatekeeperBaseOverride)` (lines 73-77), which transitively repoints `GateKeeperSceneAdapter`, `ChatAdapter`, `GatekeeperStatus`, `BannedUsers`, `SceneAdmins`. Pass `http://127.0.0.1:5138` (no trailing slash). NOTE: `LocalGateKeeperSceneAdapter` (line 180) is a separate literal and is NOT covered by the override — irrelevant unless local-scene-adapter mode is exercised.
- **Source edit:** change the line 178 literal to `$"http://127.0.0.1:5138"` (drop `https`/`{ENV}` for the local plaintext service). One edit covers all six derived enums; line 180 must be edited separately if needed.

`Today`-environment forces `Url(DecentralandUrl.Gatekeeper)` early (line 67) then flips the domain to `org` — the override still wins because it is written into `cache` after that block.

---

## 2. Local service e2e checks

Launch the binary under test on a non-prod port (here `5147`) so it doesn't collide with the production daemon:

```bash
cd /path/to/catalyrst
cargo build -p catalyrst-comms
set -a; . <ENV_FILE>; set +a
export HTTP_SERVER_HOST=127.0.0.1 HTTP_SERVER_PORT=5147
./target/debug/catalyrst-comms &
BASE=http://127.0.0.1:5147
```

### System / liveness
```bash
# expect: 200, non-empty body
curl -fsS -o /dev/null -w '%{http_code}\n' $BASE/ping

# expect: 200, JSON {"healthy":true,"version":"<semver>","livekit_configured":<bool>,"livekit_host":"<wss host>"}
curl -fsS $BASE/status | jq .
```

### Scene-adapter family (already shipped — regression guard)
```bash
# BannedUsers enum target, public, fresh DB -> 200 {"banned":false,...}
curl -fsS $BASE/users/0x000000000000000000000000000000000000dead/bans | jq .

# SceneAdmins enum target, no auth chain -> 400
curl -s -o /dev/null -w '%{http_code}\n' "$BASE/scene-admin?place_id=abc"

# GateKeeperSceneAdapter enum target -> 200, adapter string "livekit:wss://..."
curl -fsS -X POST -H 'Content-Type: application/json' \
  -d '{"sceneId":"bafkrei0000000000000000000000000000000000000000000000000000","identity":"0x000000000000000000000000000000000000beef","parcel":"0,0","realmName":"main"}' \
  $BASE/get-scene-adapter | jq -e '.adapter | startswith("livekit:wss://")'
```

### ChatAdapter — `GET /private-messages/token` (THIS LANE — Increment 1)
This route now requires a signed-fetch AuthChain with signed `metadata.signer == "dcl:explorer"`. Behaviour by input:

```bash
# (a) No AuthChain headers -> 401 "Access denied, invalid identity"
curl -s -o /dev/null -w '%{http_code}\n' $BASE/private-messages/token
# expect: 401

# (b) Malformed / wrong-signer AuthChain -> 401
curl -s -o /dev/null -w '%{http_code}\n' \
  -H 'x-identity-auth-chain-0: {"type":"SIGNER","payload":"0xbeef","signature":""}' \
  $BASE/private-messages/token
# expect: 401  (no valid signer / metadata.signer != dcl:explorer)

# (c) Valid AuthChain for a deny-listed wallet -> 403 "Access denied, deny-listed wallet"
#     (seed a ban first, then sign — see "Signed-fetch helper" below)
# expect: 403

# (d) Valid AuthChain, clean wallet -> 200 {"adapter":"livekit:<host>?access_token=<jwt>"}
#     jwt is a listen-only grant (canPublish=false) for PRIVATE_MESSAGES_ROOM_ID,
#     metadata embeds {"private_messages_privacy":"<ALL|...>"}
curl -fsS <signed-fetch headers> $BASE/private-messages/token \
  | jq -e '.adapter | startswith("livekit:")'
# expect: 200; decode the JWT (cut -d. -f2 | base64 -d) and assert:
#   video.room == $PRIVATE_MESSAGES_ROOM_ID, video.canPublish == false,
#   video.canUpdateOwnMetadata == false, metadata contains private_messages_privacy
```

> **Smoke-script drift to fix:** any smoke check that still asserts `GET /private-messages/token -> 501` is stale. After this lane it returns **401** (no auth) — update that assertion to `401`, and add an authed 200 case using the signed-fetch helper.

#### Signed-fetch helper (for cases c/d)
The AuthChain + `metadata.signer:"dcl:explorer"` payload must be EIP-712 signed by a test identity. Reuse the catalyrst signing path:
- crypto verify side: `crates/catalyrst-comms/src/auth_chain.rs::require_signer_with_metadata`.
- To mint headers locally, use the same ephemeral-identity signer the other catalyrst e2e lanes use (the `catalyrst-fed` / `catalyrst-crypto` test signer), producing `x-identity-auth-chain-0..N`, `x-identity-timestamp`, `x-identity-metadata` for method `get` path `/private-messages/token` with body metadata `{"signer":"dcl:explorer"}`. If no CLI signer exists yet, drive case (d) through the real client (section 3) which signs natively.

### Deferred surfaces (still 501 — guard they did NOT regress to 404/500)
```bash
for p in \
  "POST /private-voice-chat" \
  "GET  /users/0xdead/voice-chat-status" \
  "POST /community-voice-chat" \
  "GET  /community-voice-chat/active" \
  "GET  /cast/generate-stream-link"; do
  m=${p%% *}; u=${p##* }
  echo -n "$p -> "; curl -s -o /dev/null -w '%{http_code}\n' -X $m $BASE$u -H 'Content-Type: application/json' -d '{}'
done
# expect: 501 for each
# PATCH /users/{address}/private-messages-privacy -> 501 (Increment 2)
curl -s -o /dev/null -w '%{http_code}\n' -X PATCH $BASE/users/0xdead/private-messages-privacy -d '{}' -H 'Content-Type: application/json'
```

### LiveKit webhook (no signature key by default -> accepted)
```bash
curl -s -o /dev/null -w '%{http_code}\n' -X POST -H 'Content-Type: application/json' \
  -d '{"event":"room_started","room":{"name":"r1"}}' $BASE/livekit-webhook
# expect: 200
```

---

## 3. Real-client smoke (private-messaging plane)

The only enum this lane unblocks end-to-end is **`ChatAdapter`** (private messages). The Unity refclient (`dcl-walk`) is the canonical driver for chat, but the LiveKit adapter URL handshake can be validated with either client once the host is repointed.

1. **Repoint:** build/run the explorer with `gatekeeperBaseOverride = http://127.0.0.1:5138` (Unity ctor arg per section 1) OR edit line 178. Point the rest of the realm at your local catalyst (e.g. `http://127.0.0.1:5140`).
2. **Auth a real identity:** sign in with a real wallet so the gatekeeper accepts its signed-fetch.
3. **Trigger the token fetch:** open the chat / private-messages surface. The client calls `GET {Gatekeeper}/private-messages/token` (ChatAdapter). Confirm in the service log a `200` with `wallet=<addr>` and no `rejected ... deny-listed` warning.
4. **Assert client side:** the explorer should connect to the returned `livekit:<host>?access_token=` adapter as a listen-only participant (no publish). Capture client logs / a screenshot showing the private-messages channel connected.
5. **Deny-list negative path:** insert a `user_bans` row for the test wallet, repeat step 3, expect the client to fail to obtain a chat token (service logs `403 deny-listed`).
6. **Alternate client fallback:** a second explorer client run against the repointed realm exercises the scene-adapter (voice/comms) path through the local host as a broader regression even though its private-messaging UI differs.

---

## 4. Exit criteria
- `/ping` 200, `/status` 200 JSON with `healthy:true`.
- Scene-adapter family unchanged (200 adapter mint; 400/401 on missing auth).
- `GET /private-messages/token`: 401 unauth, 403 deny-listed, 200 listen-only LiveKit adapter for a clean authed wallet (JWT grants verified).
- All Increment 2/3/Cast routes still return structured `501`.
- Smoke script `private-messages-501` assertion updated to `401` (+ authed-200 case added).
- Real client (`dcl-walk`) connects to the private-messages LiveKit room against the local host.
