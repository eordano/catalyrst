# Verification — catalyrst-rpc (service "rpc")

Adversarial re-check of the prior re-check findings. Verified against the **committed tree**
(`crates/catalyrst-rpc/`), the upstream contract (`rpc.decentraland.org`, an external eth JSON-RPC
gateway — no local source), and the Unity C# consumers
(`DappWeb3Authenticator`, `ThirdWebEthereumApi`, DTOs in `DCL/Web3/`).

Catalog confirmation: the endpoint is real and called. `unity-net-catalog` records
`DecentralandUrl.ApiRpc = wss://rpc.decentraland.{ENV}` (WS, env-aware) consumed by
`DappWeb3Authenticator.Default.cs`, plus a literal `https://rpc.decentraland.org/{chain}` HTTP POST
consumed by `ThirdWebEthereumApi.cs`. So **both** routes on `/{network}` are reachable from the client,
not just the WebSocket one the prior finding scoped.

## Per-endpoint table

| Endpoint | Shape | Client reaction | Severity | Failure-modes OK | Notes |
|---|---|---|---|---|---|
| `GET /{network}` (WS upgrade) | match | degraded (silent) | minor | partial | `handle_socket` (modules/rpc.rs:32-62) reads `Message::Text` -> `relay::handle_payload`, replies `Message::Text` with the JSON-RPC `Value` envelope, echoing the numeric id; answers `Ping` with `Pong`; `Close` breaks. Raw one-object-per-frame, NOT Socket.IO — matches `DappWeb3Authenticator`'s raw `DCLWebSocket` usage (sends `JsonConvert.SerializeObject(request)` bytes, recv loop matches on `response.id == request.id`, cs:324-336). Shape verdict holds. |
| `POST /{network}` (HTTP) | match | degraded (silent) | minor | yes | `http_rpc` (modules/rpc.rs:15-22) ALWAYS returns `StatusCode::OK` + JSON envelope, even for errors. Reached by `ThirdWebEthereumApi.SendRpcRequestAsync` (cs:149-189). Client checks `IsSuccessStatusCode` (cs:174) and only throws on non-2xx — since we always 200, it never throws; deserializes body into `EthApiResponse` and reads `result` (null on error). Not flagged by the prior finding (gap). |
| `GET /health` | match | n/a | none | yes | `ping.rs:6` returns `"ok"` text/plain 200. Not a client endpoint. |

## Confirmed issues

1. **Error envelopes are invisible to the client DTO (silent degrade).** `EthApiResponse`
   (`DCL/Web3/EthApiResponse.cs:3-8`) is `{ long id; string jsonrpc; object? result }` — **no `error`
   field**. Every catalyrst error reply (`-32600/-32601/-32602/-32603`, built by `rpc_error`,
   relay.rs:25-31) carries `{jsonrpc,id,error}` with **no `result`**. On the WS path the client
   deserializes it, sees the echoed id match (cs:335), and returns `result == null` as a "success".
   On the HTTP path `ThirdWebEthereumApi` returns `rpcResponse.result` (null) the same way. The user-facing
   effect is a null/empty eth result instead of a surfaced error. **Severity: minor**, because this is
   NOT a catalyrst regression: the real upstream `rpc.decentraland.org` returns genuine eth errors in the
   same `{error:{...}}`-without-`result` shape, so the client already behaves identically against
   production. Confirmed real on the committed tree.

## Client-crash risks

None. Verified there is no null-deref or required-field assertion that fires:
- WS path: client returns the `EthApiResponse` struct directly; `result` is `object?` and is never
  dereferenced in `DappWeb3Authenticator`.
- HTTP path: `ThirdWebEthereumApi` reads `rpcResponse.result?.ToString()` (cs:301,313) with the
  null-conditional operator; `result == null` yields a fallback string, no throw.
- `EthApiResponse.id`/`EthApiRequest.id` are non-nullable `long`; a numeric echoed id deserializes
  cleanly. No `JsonConvert` converter on this type asserts presence of any field.

## Failure-mode gaps

1. **The `-32700` "effective hang, not a throw" claim is WRONG.** The prior finding asserts that a
   parse-error frame (`id` hard-set to `Null`, rpc.rs:42-46) deserializes to `id == 0`, never matches a
   nonzero `request.id`, so "the recv loop keeps receiving until the CancellationToken fires — effective
   hang, not a throw." In the committed client, the entire receive call
   `RequestEthMethodWithoutSignatureAsync(...)` is wrapped in `.Timeout(TimeSpan.FromSeconds(30))`
   (`DappWeb3Authenticator.cs:266-267`). So the outcome is a **bounded ~30s stall, then a thrown
   `TimeoutException`** that propagates out of the web3 call — not an unbounded hang, and it *does* end in
   a throw. The finding both over-stated the duration (bounded by the 30s `Timeout`, not the
   CancellationToken) and mis-stated the terminal behavior (throw, not silent hang).

2. **The `-32700` / `id:null` scenario is unreachable from this client anyway.** `-32700` only fires
   when `serde_json::from_str` fails on the inbound text frame — i.e., the client sent malformed JSON.
   `DappWeb3Authenticator` always sends `JsonConvert.SerializeObject(request)` (valid JSON), and never
   sends batches or non-object payloads (the other `id:null` paths: empty-batch `-32600` at relay.rs:99,
   non-object `-32600` at relay.rs:108-112). So the only `id:null` envelopes catalyrst can emit are never
   triggered on this path. The sharp edge is theoretical, not a live concern.

3. **`-32601` (method not on allowlist) is genuinely unreachable — confirmed.** The Rust
   `READ_ONLY_METHODS` (relay.rs:4-19, 14 entries) is byte-identical to the C# `readOnlyMethods` set
   (`DappWeb3Authenticator.Default.cs:45-60`). Moreover, only methods in BOTH the client `whitelistMethods`
   `{eth_getBalance, eth_call, eth_blockNumber, eth_signTypedData_v4, eth_sendTransaction}` (cs:38-44)
   AND `readOnlyMethods` reach the WS relay as read-only (cs:94, 137-138) — i.e. `eth_getBalance`,
   `eth_call`, `eth_blockNumber`, all on the Rust allowlist. The prior finding's `ok:true` for `-32601`
   holds.

4. **Upstream-comparison `ok:false` flags are speculative.** Claims like "raw upstream WS would also
   emit a parse error frame" and "upstream same body shape" cannot be verified — there is no local source
   for the `rpc.decentraland.org` gateway. They are reasonable assumptions but should not be presented as
   verified. The catalyrst-side behavior itself is confirmed.

## Crate-level claims — all confirmed

- **No DB, no LiveKit.** The crate has neither dependency; `build_state` (lib.rs:17-39) only builds a
  `reqwest::Client` and stores the config. Panic-free with empty env.
- **Empty-env boot.** `Config::from_env` (config.rs:28-56) hard-codes `rpc.decentraland.org` defaults for
  all 10 network keys. The only fallible env step is `HTTP_SERVER_PORT.parse()` (config.rs:50-53), which
  yields a graceful `Err` propagated via `?` in `main` (main.rs:20) / `build_rpc` — never a panic.
  Empty upstream URL only `warn!`s (lib.rs:25-27); unknown network name only `warn!`s (lib.rs:28-30).
  `reqwest::Client::builder().build()` is the only other fallible step and surfaces as `Err`.
- **Bundle wiring.** `build_rpc` in `catalyrst-data/src/main.rs:118-122` constructs from the same defaults;
  no extra wiring. (Standalone default port is 5153 per config.rs:51; under the data bundle it is served on
  5146.)
- **Error model coherent and JSON-RPC-2.0-correct.** Every reply — malformed input, unsupported network,
  disallowed method, upstream-down — is HTTP 200 with a body envelope; errors live in the body, matching
  the upstream eth gateway. Codes: `-32700` (WS-only parse error, id forced Null), `-32600` (missing
  method / non-object / empty batch), `-32601` (method not allowed), `-32602` (unsupported network),
  `-32603` (upstream failed / invalid JSON / non-object body / neither result nor error). No 4xx/5xx from
  the relay. Confirmed against relay.rs and the in-crate unit tests (relay.rs:116-164).

## Net assessment

Shape verdict **match** for all endpoints holds on the committed tree. The single real divergence is the
client DTO's lack of an `error` field, causing silent null-result degradation — but it is a pre-existing,
non-regression behavior shared with production upstream, hence **minor**. No crash risk. The prior
finding's headline failure-mode claim (the `-32700` "effective hang, not a throw") is **rejected**: the
client bounds the wait at 30s and ends in a thrown `TimeoutException`, and the scenario is unreachable from
the real client regardless.
