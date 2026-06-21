# E2E Test Plan — `auth-api` (catalyrst-explorer-api)

Future end-to-end test plan for the catalyrst reimplementation of
`auth-api.decentraland.org`.

| | |
|---|---|
| Key | `auth-api` |
| Crate | `catalyrst-explorer-api` (auth lives in `src/modules/auth_api.rs`) |
| Local port | **`5137`** (host-root mount: routes are under `/auth/...`) |
| Shared DB | No — in-process `DashMap` with TTL eviction + identity tombstones |
| Transports | HTTP polling (`/auth/...`) **and** Socket.IO root namespace (`/`) on the same axum router |
| Upstream | `decentraland/auth-server` |

---

## 1. Unity config — how to repoint `ApiAuth` to your host

### Enum
`DecentralandUrl.ApiAuth = 24`
in `Explorer/Assets/DCL/Infrastructure/Utility/DecentralandUrls/DecentralandUrl.cs`
of the `unity-explorer` checkout.

### The line to change
File:
`Explorer/Assets/DCL/NetworkDefinitions/Browser/DecentralandUrlsSource.cs`

Line **168**, inside the `RawUrl(DecentralandUrl)` switch:

```csharp
DecentralandUrl.ApiAuth => $"https://auth-api.decentraland.{ENV}",
```

### Is this `/about`-discovered?
**No.** `ApiAuth` is a plain templated URL (default cache behaviour `STATIC`,
not `REALM_DEPENDENT`). The realm `/about` response does **not** carry an
auth-api endpoint, so the realm cannot repoint it. It is therefore changed by
**editing Unity** at the line above (not by editing your `/about`).

### How to repoint
The base URL has **no path** — Unity's `DappWeb3Authenticator` consumes the bare
host. Two consumers, both reading `Url(DecentralandUrl.ApiAuth)`:
- `DappWeb3Authenticator.Default.cs:27` — builds `authApiUrl`, then opens a
  **Socket.IO connection to the bare base** (`new Uri(authApiUrl)`,
  `DappWeb3Authenticator.cs:499-504`, root `/` namespace, WebSocket transport).
  Events emitted/listened: `request`, `recover`, `outcome`,
  `request-validation-status`.
- `BootstrapContainer.cs:203` — passes the same URL into the auth flow.

So the override target is the **bare origin** (scheme + host + optional port).
Because socketioxide mounts the Socket.IO namespace at the router root and HTTP
routes are under `/auth/...`, pick **one** of:

- **Direct (recommended for local bring-up)** — point straight at the Rust
  process root and rely on it serving both Socket.IO `/` and `/auth/*`:
  ```csharp
  DecentralandUrl.ApiAuth => "http://localhost:5137",
  ```
  (Unity's `SocketIO` will upgrade `ws://localhost:5137/socket.io/`.)

- **Via a reverse proxy with the upstream path layout** — if a reverse proxy
  fronts `auth-api.<host>` and strips `/auth` before forwarding to the service
  while proxying the WS upgrade, set:
  ```csharp
  DecentralandUrl.ApiAuth => $"https://auth-api.<your-host>",
  ```
  Per the impl note, that `/auth`-stripping + WS-upgrade proxy is left to the
  operator to wire up.

> Caveat to confirm during bring-up: upstream auth-server serves Socket.IO at
> the **origin root** while the catalyrst HTTP routes are namespaced under
> `/auth`. If a host serves *both* transports, the HTTP polling fallback expects
> `/auth/...` but Unity's primary path is Socket.IO at `/`. The direct
> `localhost:5137` form satisfies both because the Rust router merges Socket.IO
> at `/` and HTTP under `/auth` on the same listener.

---

## 2. Local service bring-up

```bash
# build & run; honors HTTP_HOST/HTTP_PORT from Config::from_env()
cargo run -p catalyrst-explorer-api

# health gate before running checks
curl -fsS http://localhost:5137/auth/health/ready && echo READY
```

All `curl` checks below assume the service is listening on `localhost:5137`.

---

## 3. E2E checks (HTTP)

Each check lists the command and the expected status + body shape.

### H1 — liveness probe
```bash
curl -i http://localhost:5137/auth/health/live
```
Expect `200`, JSON `{"timestamp": <epoch_ms:int>}`.

### H2 — readiness / startup probes
```bash
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:5137/auth/health/ready
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:5137/auth/health/startup
```
Expect `200` each, empty body.

### H3 — create request (dcl_personal_sign, no auth chain required)
```bash
curl -i -X POST http://localhost:5137/auth/requests \
  -H 'content-type: application/json' \
  -d '{"method":"dcl_personal_sign","params":["hello"]}'
```
Expect `201`, JSON `{"requestId":"<uuidv4>","expiration":"<rfc3339, ~600s out>","code":<int 0..99>}`.
Capture `requestId` as `$RID`.

### H4 — create request missing auth chain for non-personal-sign method
```bash
curl -i -X POST http://localhost:5137/auth/requests \
  -H 'content-type: application/json' \
  -d '{"method":"eth_sendTransaction","params":[]}'
```
Expect `400`, JSON `{"error":"Auth chain is required"}`.

### H5 — input-limit rejection (params > 10)
```bash
curl -i -X POST http://localhost:5137/auth/requests \
  -H 'content-type: application/json' \
  -d '{"method":"dcl_personal_sign","params":[1,2,3,4,5,6,7,8,9,10,11]}'
```
Expect `400`, JSON `{"error":"params exceeds 10 items"}`.

### H6 — v2 recover a pending request
```bash
curl -i http://localhost:5137/auth/v2/requests/$RID
```
Expect `200`, JSON `{"expiration":...,"code":...,"method":"dcl_personal_sign","params":["hello"]}` (no `sender` for personal-sign).

### H7 — GET outcome while still pending (no content)
```bash
curl -i http://localhost:5137/auth/requests/$RID
```
Expect `204` (body, if read, is `{"error":"Request with id \"...\" has not been completed"}`).

### H8 — submit outcome (canonical v2 path)
```bash
curl -i -X POST http://localhost:5137/auth/v2/requests/$RID/outcome \
  -H 'content-type: application/json' \
  -d '{"sender":"0x1111111111111111111111111111111111111111","result":"0xabc"}'
```
Expect `200`, empty body.

### H9 — GET outcome after submission (consumes it)
```bash
curl -i http://localhost:5137/auth/requests/$RID
```
Expect `200`, JSON `{"requestId":"$RID","sender":"0x1111...","result":"0xabc"}` (sender lower-cased).

### H10 — GET outcome again -> already fulfilled
```bash
curl -i http://localhost:5137/auth/requests/$RID
```
Expect `410 Gone`, JSON `{"error":"Request with id \"$RID\" has already been fulfilled"}`.

### H11 — legacy outcome alias (POST /requests/{id}) on a fresh request
```bash
# create a fresh $RID2 via H3 first, then:
curl -i -X POST http://localhost:5137/auth/requests/$RID2 \
  -H 'content-type: application/json' \
  -d '{"sender":"0x2222222222222222222222222222222222222222","result":"0x01"}'
```
Expect `200`. (Verifies the catalyrst-only legacy alias maps to the same handler.)

### H12 — outcome validation errors
```bash
# bad sender
curl -i -X POST http://localhost:5137/auth/v2/requests/$RID3/outcome \
  -H 'content-type: application/json' -d '{"sender":"nope","result":"x"}'
# expect 400 {"error":"sender must be a valid ethereum address"}

# neither result nor error
curl -i -X POST http://localhost:5137/auth/v2/requests/$RID3/outcome \
  -H 'content-type: application/json' \
  -d '{"sender":"0x2222222222222222222222222222222222222222"}'
# expect 400 {"error":"either result or error is required"}
```

### H13 — validation status round-trip
```bash
# create $RID4 via H3, then:
curl -s -o /dev/null -w '%{http_code}\n' \
  -X POST http://localhost:5137/auth/v2/requests/$RID4/validation   # expect 204
curl -i http://localhost:5137/auth/v2/requests/$RID4/validation     # expect 200 {"requiresValidation":true}
```

### H14 — unknown request id
```bash
curl -i http://localhost:5137/auth/v2/requests/00000000-0000-4000-8000-000000000000
```
Expect `404`, JSON `{"error":"Request with id \"...\" not found"}`.

### H15 — identities: signed-fetch gate rejects unsigned request
```bash
curl -i -X POST http://localhost:5137/auth/identities \
  -H 'content-type: application/json' \
  -d '{"identity":{"expiration":"2099-01-01T00:00:00Z","ephemeralIdentity":{"address":"0x..","publicKey":"0x..","privateKey":"0x.."},"authChain":[]}}'
```
Expect `401`, JSON `{"error":"Invalid auth chain","message":"This endpoint requires a signed fetch request. See ADR-44."}`.

### H16 — identities: scene signer rejected
With valid ADR-44 `x-identity-auth-chain-{0,1}` / `x-identity-timestamp` /
`x-identity-metadata` headers where metadata `signer == "decentraland-kernel-scene"`.
Expect `401`, JSON `{"error":"Requests from scenes are not allowed","message":"...ADR-44."}`.
(Requires a signing helper — see fixtures below.)

### H17 — identities: happy path (3 cross-checks)
With a real signed-fetch wallet identity whose:
1. `ephemeralIdentity.address` == auth-chain final authority,
2. signed-fetch signer == identity owner,
3. `ephemeralIdentity.privateKey` derives `ephemeralIdentity.address`.

```bash
# POST /auth/identities  -> expect 201 {"identityId":"<uuidv4>","expiration":...}
# GET  /auth/identities/<identityId>  (same IP) -> expect 200 {"identity":{...same AuthIdentity...}}
# GET  again -> expect 404 {"error":"Identity was already consumed"}
```

### H18 — identities: IP /24 binding + mobile bypass
```bash
# create with isMobile:false then GET from a /24-mismatched IP via spoofed header:
curl -i http://localhost:5137/auth/identities/<id> -H 'true-client-ip: 10.9.9.9'
# expect 403 {"error":"IP address mismatch"} (creator was in a different /24)
# then GET again -> 404 {"error":"Identity was deleted due to IP mismatch"}

# repeat creating with isMobile:true -> GET from mismatched IP succeeds (200, mobile bypass)
```

### H19 — identities: malformed id
```bash
curl -i http://localhost:5137/auth/identities/not-a-uuid
```
Expect `400`, JSON `{"error":"Invalid identity format"}`.

---

## 4. E2E checks (Socket.IO / WebSocket)

Unity uses the Socket.IO `/` namespace, not raw WS frames. Use a Socket.IO
client (`node` + `socket.io-client@4`, matching `EIO=4`). `wscat` alone cannot
speak the Socket.IO handshake, so prefer a tiny node script.

### W1 — connect + `request` ack returns a request id
```js
// node, socket.io-client v4
const { io } = require("socket.io-client");
const s = io("http://localhost:5137", { transports: ["websocket"] });
s.on("connect", () => {
  s.emit("request", { method: "dcl_personal_sign", params: ["hi"] }, (ack) => {
    console.log("request ack", ack); // expect {requestId, expiration, code}
  });
});
```
Expect ack `{requestId, expiration, code}`; no `error` field.

### W2 — `recover` returns the pending request
Emit `recover` `{requestId}` -> ack `{expiration, code, method, params}`.

### W3 — one-active-request-per-socket / outcome relay
On socket A, `request` then have an HTTP client `POST /auth/v2/requests/{id}/outcome`.
Socket A must receive an `outcome` event with `{requestId, sender, result|error}`.
(Confirms HTTP handlers relay back to the originating socket via the stashed
`SocketIo` handle.)

### W4 — validation status relay
On socket A, `request` then HTTP `POST /auth/v2/requests/{id}/validation`.
Socket A must receive `request-validation-status` `{requestId}` exactly once.

### W5 — survives disconnect (HTTP fallback)
Socket A creates a request, then disconnects. An HTTP `POST .../outcome`
must succeed (`200`), and a subsequent HTTP `GET /auth/requests/{id}` must
return the stored outcome (`200`) — i.e. the outcome is stored for polling
when the socket is gone, not lost.

### W6 — split TTL
Create a `dcl_personal_sign` request and a non-personal request; assert the
`expiration` deltas are ~600s vs ~300s respectively.

---

## 5. Real-client smoke step

`auth-api` is exercised by the **Unity** explorer's `DappWeb3Authenticator`
(login / signature flow), not by bevy or godot's normal walk path — login in
those runtimes uses a pre-baked identity. So the meaningful client smoke is on
the Unity refclient:

1. In a `unity-explorer` checkout, repoint line 168 of `DecentralandUrlsSource.cs`
   to `"http://localhost:5137"` (see §1).
2. Start the service (§2) and confirm `GET /auth/health/ready` is `200`.
3. Drive the upstream Unity client headlessly per the `dcl-explore` skill —
   `dcl-walk launch` then `dcl-walk auth-sign`.
4. Observe the Socket.IO traffic on the service port: a `connect`, a `request`
   emit, the browser-side signature, then an `outcome`/`request-validation-status`
   event delivered back. The client should reach a logged-in state.
5. Negative smoke: kill the service mid-flow and confirm the client surfaces a
   timeout/disconnect rather than hanging (validates `OnWebSocketDisconnected`).

For bevy/godot there is no auth-api dependency to smoke; note this explicitly in
the run log rather than attempting `dcl-bevy`.

---

## 6. Fixtures needed (to unblock the signed-path checks)

- An ADR-44 signed-fetch header generator (auth chain links + `x-identity-*`
  headers, timestamp within +/-300s) — reuse `@dcl/crypto` or `catalyrst-crypto`
  to mint `x-identity-auth-chain-0/1`, `x-identity-timestamp`,
  `x-identity-metadata`.
- A wallet identity fixture satisfying the 3 cross-checks for H17 (ephemeral
  key + address + auth chain whose final authority == ephemeral address).
- A node Socket.IO v4 harness for §4.

---

## 7. Out of scope (deferred, do not test here)

- `POST /onboarding/checkpoint` (internal events-notifier endpoint).
- `GET /onboarding/pending-nudges`, `/admin/*` (internal nudge ops).

These are not explorer-facing and are intentionally unimplemented.
