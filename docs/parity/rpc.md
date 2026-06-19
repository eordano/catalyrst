# Parity report — `catalyrst-rpc` (service "rpc")

Upstream reference: `rpc.decentraland.org` — a method-filtered read-only EVM
JSON-RPC 2.0 relay. Two client-facing transports on `/{network}`: HTTP POST
(single/batch JSON-RPC) and a raw WebSocket upgrade (Unity
`DappWeb3Authenticator` path). Crate at
`crates/catalyrst-rpc`. Port 5141.

Verification method: static. The Rust crate (`relay.rs`, `modules/rpc.rs`,
`modules/ping.rs`, `config.rs`, `state.rs`, `lib.rs`, `Cargo.toml`) was read in
full and cross-checked against the Unity client contracts
(`DappWeb3Authenticator.cs`, `DappWeb3Authenticator.Default.cs`,
`EthApiResponse.cs`, `ThirdWebEthereumApi.cs`, `ThirdWebAuthenticator.cs`,
`BootstrapContainer.cs`) and the net-catalog
(`the Unity net-catalog`). No live service was run
(catalyrst-rpc is not up by default; upstream `rpc.decentraland.org` is a public
host, not mirrored here) — diff is not-applicable, so all findings are
source-confirmed.

## Per-endpoint table

| Endpoint | Shape | Efficiency | Severity | Notes |
|---|---|---|---|---|
| `POST /{network}` | match | same | none (+ low coverage caveat) | Verbatim forward of provider body; allowlist and JSON-RPC envelope identical. Network-coverage caveat below is conditional and low severity. |
| `GET /{network}` (WS upgrade) | match | same | none | Raw WebSocket (not Socket.IO), one JSON-RPC object per text frame, id echoed verbatim; matched by `response.id == request.id` on the client. |
| `GET /health` | n/a (no client contract) | same | none | Local liveness probe returning constant `"ok"` text/plain. Not in the net-catalog; excluded from `api_router()`. Parity undefined, not divergent. |

## Confirmed shape findings

All three endpoints' shape verdicts from the input set survived verification.

- **`POST /{network}` — match (confirmed).** `forward` (relay.rs:84-91) does
  `r.json::<Value>()` and returns the body untouched: no struct round-trip, no
  renaming, no casing transform. The Unity `EthApiResponse`
  (`EthApiResponse.cs:3-8`) is `{ long id; string jsonrpc; object? result }`.
  Because `result` is a nullable `object?` and Newtonsoft ignores unknown keys,
  our self-generated error bodies `{id, jsonrpc, error:{code,message}}`
  (relay.rs:37-43) deserialize cleanly with `result == null`. The ThirdWeb POST
  consumer reads `rpcResponse.result` directly (`ThirdWebEthereumApi.cs:181-188`)
  and never inspects an `error` field, so the extra `error` key is harmless.
  Allowlist parity is exact: the 14-entry `READ_ONLY_METHODS` (relay.rs:15-30)
  equals the client `readOnlyMethods` HashSet
  (`DappWeb3Authenticator.Default.cs:45-60`) name-for-name. Single-vs-batch
  wrapper preserved (object->object, array->array; empty batch -> single
  `{id:null,error:-32600}`, relay.rs:97-117). Protocol/network errors return
  HTTP 200 with the error in the body (rpc.rs:37-40), matching JSON-RPC server
  convention.

- **`GET /{network}` (WS) — match (confirmed).** The client uses
  `new DCLWebSocket()` + raw text frames (`DappWeb3Authenticator.cs:316-324`),
  NOT Socket.IO — Socket.IO is reserved for the separate auth API
  (`DappWeb3Authenticator.cs:501`). Our handler reads `Message::Text` and replies
  `Message::Text` (rpc.rs:60,70), and answers `Ping` with `Pong`
  (rpc.rs:74-78) so keep-alive holds. The client sends one frame, then loops
  receiving frames and returns only the frame whose `response.id == request.id`
  (`DappWeb3Authenticator.cs:326-336`); a parse-error frame we emit with
  `id:null` (rpc.rs:63-67) simply fails the id match and is skipped rather than
  mis-delivered — safe. id is echoed verbatim (relay.rs:80), `jsonrpc` is the
  literal `"2.0"`.

- **`GET /health` — n/a (confirmed).** Constant `"ok"` text/plain
  (ping.rs:6). No net-catalog entry, no client contract; kept out of
  `api_router()` (lib.rs:16-18) to avoid path collisions when bundled. Parity is
  undefined rather than match/divergent.

## Confirmed efficiency findings

- **All three endpoints: "same" (confirmed). No efficiency WIN claimed.**
  `Cargo.toml` has no `sqlx` and no `moka` — verified; both transports are a
  stateless, method-filtered reverse proxy doing exactly one outbound forward per
  JSON-RPC request to the same chain provider, structurally identical to the
  upstream relay. Our local advantages are a shared pooled keep-alive
  `reqwest::Client` (lib.rs:23-27, `pool_max_idle_per_host=16`), a const-array
  allowlist check, and an O(1) in-memory `HashMap` network lookup loaded once at
  startup (config.rs:32-47). These are comparable to, not structurally better
  than, the upstream proxy's connection reuse — so no "better" verdict is
  warranted (and any "better" resting on language choice alone is rejected).
  Counterweight: batches are forwarded sequentially (relay.rs:104-107) = N
  round-trips with no pipelining/multicall, and idempotent reads are not
  memoized. The Unity WS client never sends batches, so this is moot in practice.
  Net: same.

## Network-coverage caveat (low severity, conditional)

The net-catalog records a SECOND client consumer of `rpc.decentraland.org`: the
ThirdWeb path (`ThirdWebEthereumApi.SendRpcRequestAsync`, lines 149-189), which
HTTP-POSTs to `https://rpc.decentraland.org/{chain}` for NINE chains hardcoded in
`RPC_OVERRIDES` (`ThirdWebAuthenticator.cs:24-35`): mainnet (1), sepolia
(11155111), polygon (137), amoy (80002), **arbitrum (42161), optimism (10),
avalanche (43114), binance/BSC (56), fantom (250)**.

Our `config.rs` (lines 37-43) only configures FIVE networks: mainnet, ethereum,
sepolia, polygon, amoy. The four extra L2/alt chains (arbitrum, optimism,
avalanche, binance, fantom) would return `-32602 "Unsupported network"` from
`handle_single` (relay.rs:71-75).

Why this is LOW severity, not a confirmed client-facing break:
- The ThirdWeb `RPC_OVERRIDES` URLs are hardcoded to the literal public
  `rpc.decentraland.org` host and are NOT env-aware — they do not flow through
  the configurable `DecentralandUrl.ApiRpc` domain. By contrast, the ONLY path
  wired to `ApiRpc` (`BootstrapContainer.cs:201-213`, the `DappWeb3Authenticator`
  WS transport) uses chains the 5-network config already covers.
- Under the standard "repoint `ApiRpc` at catalyrst-rpc" deployment, the
  ThirdWeb POST path keeps hitting the public host and never reaches our relay.
  The crate doc itself scopes the POST transport to server-side consumers
  (marketplace-server, realm-provider; rpc.rs:4-6), not the Unity ThirdWeb
  client.
- The gap only bites if someone DNS/host-overrides `rpc.decentraland.org` itself
  to catalyrst-rpc, at which point ThirdWeb calls for the four uncovered chains
  would fail.

Action if full drop-in replacement is intended: add arbitrum (42161), optimism
(10), avalanche (43114), binance (56), and fantom (250) upstreams (plus their
`chain_id_for` entries) to close the gap. Tracked here as a coverage caveat
rather than a per-endpoint shape divergence because the response SHAPE for the
covered chains is correct and the uncovered chains are unreachable under the
normal deployment model.

## Rejected during verification

- Rejected: any reading of the catalog finding that the client "keys off HTTP
  status / null result, not an error field" as if error handling were purely
  status-driven. The ThirdWeb POST path actually does BOTH — it throws on a
  non-2xx status (`ThirdWebEthereumApi.cs:174-178`) AND reads `result` on success
  (line 181/187). Our relay always returns HTTP 200 (rpc.rs:40), so a provider
  error never trips the throw path and instead surfaces as `result == null`,
  which is the same outcome as the upstream returning a 200 error body. The
  net behavior matches, but the mechanism is not "status-only" as a loose reading
  might suggest.
- Rejected: treating the batch-sequential-forward note as a divergence. It is a
  real structural property (relay.rs:104-107) but matches typical upstream relay
  behavior and the Unity WS client never sends batches, so it changes no
  client-visible result. Recorded under efficiency as "same", not a shape issue.
- Rejected: any "efficiency better" claim. The keep-alive pool and in-memory
  allowlist/HashMap are comparable to upstream, not a structural win; no upstream
  N+1 or missing cache exists to beat.
