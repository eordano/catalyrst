# E2E test plan — `rpc-wss` (catalyrst-rpc)

Reimplementation of `rpc.decentraland.org`: a thin, stateless per-network Ethereum
JSON-RPC reverse proxy.

- **Crate:** `catalyrst-rpc`
- **Port:** `5141` (`HTTP_SERVER_HOST=127.0.0.1`, `HTTP_SERVER_PORT=5141` in
  the service's environment file, `<ENV_FILE>`)
- **Workspace:** `<WORKSPACE>`
- **Upstream target this replaces:** `wss://rpc.decentraland.{ENV}` + `/{network}`
  (and the HTTP `POST https://rpc.decentraland.{ENV}/{network}` ThirdWeb path)
- **No DB / Redis / S3 / SQS / AuthChain.** Pure passthrough; JSON-RPC envelope is
  opaque (`id` preserved, any method forwarded).

## Routes under test

| Method | Path | Behavior |
|---|---|---|
| `GET` (WS upgrade) | `/{network}` | Per text/binary frame: parse `EthApiRequest`, forward to upstream, reply with one text frame = upstream JSON; `id` preserved; socket stays open for pipelined read-only calls; ping→pong. |
| `POST` | `/{network}` | ThirdWeb path: forward JSON body to upstream, reply JSON-RPC envelope, **always HTTP 200** (errors live in the envelope). |
| `GET` | `/health` | Liveness probe. |

Recognized networks = any with `RPC_UPSTREAM_<NAME>` set. Env currently provisions:
`mainnet`, `ethereum`, `sepolia`, `polygon`, `amoy`. Unknown network ⇒ JSON-RPC
error `-32601` with echoed `id` (never a non-200 / never a silent dead socket).

---

## 1. Unity config — where to repoint

`ApiRpc` is a **hardcoded Unity URL template**, NOT `/about`-discovered. It does
**not** appear in the realm `/about` response, so editing our `/about` will have no
effect — you must edit Unity.

**File:** `Explorer/Assets/DCL/NetworkDefinitions/Browser/DecentralandUrlsSource.cs`
(in the Unity explorer source tree)

**Line 169** (`RawUrl(...)` switch arm):

```csharp
DecentralandUrl.ApiRpc => $"wss://rpc.decentraland.{ENV}",
```

`{ENV}` (constant `ENV = "{ENV}"`, line 33) is later replaced by the active
`decentralandDomain` (`org` / `zone`) in `Url(...)` / `Probe(...)`. The result is a
`URLDomain`; consumers append `/{network}` themselves
(`DappWeb3Authenticator.ConnectToRpcAsync` →
`urlBuilder.AppendDomain(rpcServerUrl); urlBuilder.AppendPath(new URLPath(network));`,
lines 313-314). So the effective wire URL is `wss://rpc.decentraland.{ENV}/{network}`,
and our service's `GET /{network}` / `POST /{network}` already match that shape.

**To repoint to our local host, change line 169 to a literal (drop the `$"..{ENV}.."`
interpolation so the env-domain substitution can't rewrite it):**

```csharp
DecentralandUrl.ApiRpc => "ws://127.0.0.1:5141",
```

(Use `ws://` not `wss://` — the local service is plain HTTP/WS, no TLS. The POST/
ThirdWeb consumer reuses the same domain over `http://` automatically via the URL
builder.)

**Enum definition (for reference, no change needed):**
`Explorer/Assets/DCL/Infrastructure/Utility/DecentralandUrls/DecentralandUrl.cs` — `ApiRpc = 26`.

**Consumers (both pick up the change automatically; no edits needed):**
- `Explorer/Assets/DCL/Web3/Authenticators/Implementations/Dapp/DappWeb3Authenticator.Default.cs:29`
  (`rpcServerUrl = URLDomain.FromString(...Url(DecentralandUrl.ApiRpc))`)
- `Explorer/Assets/DCL/Infrastructure/Global/Dynamic/BootstrapContainer.cs:205`

---

## 2. Bring the service up

If the service is not yet listening on `5141` (`ss -tln` shows nothing), start it
first:

```bash
# Option A — via systemd if a unit is provisioned
systemctl --user start catalyrst-rpc.service
systemctl --user status catalyrst-rpc.service

# Option B — run from the workspace (loads the service's environment file)
cd <WORKSPACE>
set -a; source <ENV_FILE>; set +a
cargo run -p catalyrst-rpc

# confirm it is listening
ss -tln | grep ':5141'
```

---

## 3. E2E checks (curl / wscat)

Each check lists the command and the expected status/shape. All HTTP checks use
`-i` so the status line is visible.

### C1 — Health liveness
```bash
curl -i http://127.0.0.1:5141/health
```
Expect: `HTTP/1.1 200 OK`, body `{"status":"ok"}`.

### C2 — POST mainnet, real block number (ThirdWeb path)
```bash
curl -s -X POST http://127.0.0.1:5141/mainnet \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":42,"method":"eth_blockNumber","params":[]}'
```
Expect: HTTP 200; JSON `{"jsonrpc":"2.0","id":42,"result":"0x..."}` — `id` echoed as
`42`, `result` a hex block number (during impl this returned `0x181ba75`).

### C3 — POST eth_chainId on polygon (multi-network routing)
```bash
curl -s -X POST http://127.0.0.1:5141/polygon \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":"p1","method":"eth_chainId","params":[]}'
```
Expect: HTTP 200; `{"jsonrpc":"2.0","id":"p1","result":"0x89"}` (137 = Polygon).
Confirms a second `RPC_UPSTREAM_*` route resolves independently.

### C4 — Unknown network ⇒ JSON-RPC -32601, still HTTP 200, id echoed
```bash
curl -i -s -X POST http://127.0.0.1:5141/dogecoin \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":7,"method":"eth_blockNumber","params":[]}'
```
Expect: `HTTP/1.1 200 OK`; body
`{"jsonrpc":"2.0","id":7,"error":{"code":-32601,"message":"unknown network 'dogecoin'"}}`.
(Errors ride over HTTP 200, matching upstream.)

### C5 — Malformed JSON body ⇒ parse handling
```bash
curl -i -s -X POST http://127.0.0.1:5141/mainnet \
  -H 'content-type: application/json' \
  -d 'not-json'
```
Expect: axum rejects the bad JSON body before the handler — a 4xx (400 Unprocessable/
Bad Request). (For the WS transport, an invalid frame instead yields a JSON-RPC
`-32700` envelope with `id: null` — see C8.)

### C6 — WS upgrade + read-only call, id preserved (primary explorer path)
Using `wscat` (`npm i -g wscat`) or `websocat`:
```bash
# send one frame, expect one matching reply
wscat -c ws://127.0.0.1:5141/mainnet \
  -x '{"jsonrpc":"2.0","id":99,"method":"eth_blockNumber","params":[]}' -w 5
```
Expect: a single **text** frame `{"jsonrpc":"2.0","id":99,"result":"0x..."}` with
`id` == `99`, then the socket stays open (does not close on its own).

### C7 — WS pipelined calls on one socket
```bash
# interactive: open the socket, paste two requests, see two id-matched replies
wscat -c ws://127.0.0.1:5141/mainnet
> {"jsonrpc":"2.0","id":1,"method":"eth_blockNumber","params":[]}
> {"jsonrpc":"2.0","id":2,"method":"eth_gasPrice","params":[]}
```
Expect: two replies, `id:1` then `id:2`, socket remains open between them (mirrors
`RequestEthMethodWithoutSignatureAsync` reading until `response.id == request.id`).

### C8 — WS bad frame ⇒ -32700, socket survives
```bash
wscat -c ws://127.0.0.1:5141/mainnet -x 'garbage' -w 3
```
Expect: text frame
`{"jsonrpc":"2.0","id":null,"error":{"code":-32700,...}}`; socket NOT torn down (a
subsequent valid frame still works).

### C9 — WS ping/pong keepalive
```bash
# websocat sends a ping; expect a pong (connection kept alive)
websocat -t ws://127.0.0.1:5141/mainnet --ping-interval 2 --ping-timeout 5
```
Expect: pong frames returned; connection stays open with no app traffic.

### C10 — WS unknown network does not silently hang
```bash
wscat -c ws://127.0.0.1:5141/dogecoin \
  -x '{"jsonrpc":"2.0","id":5,"method":"eth_blockNumber","params":[]}' -w 5
```
Expect: handshake succeeds, and the call returns a `-32601` JSON-RPC error frame with
`id:5` (the handler logs a warn but keeps the socket usable rather than dead).

---

## 4. Real-client smoke (dcl-walk — upstream Unity client)

This service is exercised by the **Unity** explorer's `DappWeb3Authenticator`
(WS-based read-only eth calls during login/identity + signed-fetch flows). It is not
on the bevy/godot eth path, so use `dcl-walk` here, not `dcl-bevy`.

1. Apply the Unity edit from section 1 (line 169 → `"ws://127.0.0.1:5141"`).
2. Ensure the service is up (section 2) and C1/C2 pass.
3. Launch and authenticate:
   ```bash
   dcl-walk launch
   dcl-walk auth-sign
   ```
   (See your headless Unity client tooling for the canonical launch/drive
   procedure.)
4. **Pass criteria:**
   - Login/identity completes (no eth-RPC stall during `auth-sign`).
   - In the catalyrst-rpc logs (`RUST_LOG=catalyrst_rpc=info`), observe inbound WS
     upgrades to `/mainnet` (and/or the configured network) and forwarded
     `eth_*` methods from the client's whitelist
     (`eth_getBalance`, `eth_call`, `eth_blockNumber`, `eth_signTypedData_v4`,
     `eth_sendTransaction`).
   - No `unknown network` warnings (confirms the client hits a provisioned
     `RPC_UPSTREAM_*`).
5. Revert the Unity edit when done.

---

## 5. Notes / gotchas

- `wss://` vs `ws://`: the local service has no TLS. When repointing Unity, use
  `ws://` (and the POST consumer correspondingly uses `http://`). A literal string
  (not `$"...{ENV}"`) avoids the env-domain substitution clobbering the host.
- The `/{network}` path is appended by the **client**, so our `GET`/`POST /{network}`
  routes already match the upstream contract; do not add `/{network}` into the Unity
  URL template.
- GET (upgrade) and POST coexist on `/{network}` — axum dispatches by method.
- Networks are env-driven: to test a network not in the current env
  (`arbitrum`, `optimism`, `avalanche`, `binance`, `fantom`, etc.) add
  `RPC_UPSTREAM_<NAME>=...` to the service's environment file (`<ENV_FILE>`) and restart.
