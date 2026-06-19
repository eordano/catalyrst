# Verification — catalyrst-archipelago (service `archipelago`, bundle explore/5143)

Upstream: `archipelago-ws` (= `decentraland/archipelago-workers`, `stats` worker) + archived
`archipelago-service`. Client: `unity-explorer`.

Committed tree verified: branch `feat/service-plane-crates`,
`crates/catalyrst-archipelago/src/{handlers,lib,content,gossip,livekit,main}.rs`.
Deployment topology verified against `docs/deploy/nginx-catalyrst-bundles.conf`.

Method: for each flagged endpoint I opened (a) our current Rust handler, (b) the upstream TS
handler/route table, (c) the Unity C# consumer + the net-catalog row that proves it is (or
isn't) called, and (d) for the bundle gap, the actual nginx location map and the C# health-check
control flow. The pre-existing endpoint map (now replaced by this file) was correct on the
mounting facts and is folded in below. No source files modified except this report.

## Per-endpoint table

| Endpoint | Shape | Client reaction | Severity | Failure-modes OK | Notes |
|---|---|---|---|---|---|
| `GET /hot-scenes` | match (one real divergence: `name`) | degraded, not crash | minor | mostly | Client `GoToChatCommand.FindCrowdAsync` does `hotScenes[0]` with no length guard, BUT the dispatcher (`CommandsHandleChatMessageBus.cs:67-72`) wraps `ExecuteCommandAsync` in `try/catch` → empty array yields an "Error running command" toast, not a crash. `name` divergence is real but client struct ignores `name`. |
| `GET /status` (standalone `routes()` only; NOT in `api_router`/bundle) | match (body) | **bootstrap-blocking on bundle** | **major** | no (bundle) | Real bundle gap. `api_router()` (lib.rs:89-91) = `api_routes()`+ws, omits `/status`. nginx has NO `/status` location → 404 at realm host. Client liveness probe treats 404 as failure → blocks startup. See Confirmed Issue 1. |
| `GET /ping` (standalone only) | divergent (plain text) | n/a | none | yes | Not client-called (absent from net-catalog). Harmless. |
| `GET /stats/health` | divergent (snake_case catalyrst shape) | n/a | none | yes | No client/upstream consumer. Irrelevant. |
| `POST /heartbeat` | divergent (catalyrst-only) | n/a | none | yes | Not client-called (client heartbeats over WS). Open by default (`require_signed_challenge=false`). Malformed-JSON path uses axum default rejection (plain-text 400/422), not `{error}` shape — confirmed. |
| `POST /auth/challenge` | divergent (catalyrst-only) | n/a | none | yes | Not client-called. |
| `POST /auth/livekit-token` | divergent (catalyrst-only) | n/a | none | yes | Not client-called (upstream mints in comms-gatekeeper). `mint` returns grant with `token:None` when unarmed — no panic (livekit.rs:71-74). Verified. |
| `POST /gossip/heartbeat` | divergent (inter-node federation) | n/a | none | yes | No client/upstream analogue. `verify` fails closed (401) when unarmed via `sign()->Disabled` (gossip.rs:98,122). Verified. |
| `GET /gossip/info` | divergent (introspection) | n/a | none | yes | No client/upstream analogue. |

(Also mounted and client-relevant but not flagged: `/comms/peers` — `api_routes()` registers
every prefixed route under both `""` and `/comms`, handlers.rs:26-33; the client hits
`/comms/peers` via `DecentralandUrl.RemotePeers`, proxied to 5143 in nginx. OK.)

## Confirmed issues

### 1. `/status` missing on bundle (5143) — blocks Unity client bootstrap [MAJOR, arguably under-rated]

The finding flagged this as "major / flips the liveness probe semantics." Verification shows the
impact is stronger: it **blocks client bootstrap**, not merely flips a semantic.

Evidence chain (all confirmed on the committed tree + deploy config):

- `handlers::routes()` mounts `/status` + `/ping`; `handlers::api_routes()` does NOT
  (handlers.rs:16-43).
- The explore bundle builds archipelago via `api_router()` (lib.rs:89-91) =
  `api_routes()` + `ws::routes()`, used at `crates/catalyrst-explore/src/main.rs:104`. `/status`
  is therefore not mounted on 5143. Only the standalone binary (`main.rs:22`, port 5139) mounts it.
- nginx (`docs/deploy/nginx-catalyrst-bundles.conf:111-118`) proxies `/comms/peers`,
  `/hot-scenes`, `/core-status`, `/stats/health` to `cat_explore` (5143) but has **no
  `location /status`** and **no catch-all `location /`**. A request to the realm-mapped
  `archipelago-ea-stats` host for `/status` gets nginx 404.
- Unity client: net-catalog `DecentralandUrl.ArchipelagoStatus =
  https://archipelago-ea-stats.decentraland.{ENV}/status`
  (`DecentralandUrlsSource.cs:198`), consumed at `MainSceneLoader.cs:520` inside
  `IsLIvekitDeadAsync`. It builds a
  `MultipleURLHealthCheck(ArchipelagoStatus, GatekeeperStatus).WithRetries(3)`.
- `URLHealthCheck.cs:20-24,42-44`: issues an HTTP **HEAD**, and `ERROR_CODES = {404, 500}` — a
  **404 is an explicit failure**. (Axum auto-serves HEAD for GET routes on the standalone, so
  5139 is fine; the bundle returns 404 because the route is absent.)
- `MultipleURLHealthCheck.cs:21-25` → `ParallelHealthCheck.cs:27-31`: returns the first failing
  result — i.e. **ALL** urls must succeed. One 404 on `ArchipelagoStatus` fails the whole check
  (all 3 retries 404 too).
- `MainSceneLoader.cs:503-504,526,533-535`: a failed check ⇒ `IsLIvekitDeadAsync` returns `true`
  ⇒ the LiveKit-down guard popup is shown and `TryBasicBootstrapAsync` returns `false`
  (bootstrap stops).

Net: on the bundle deployment the client cannot get past the LiveKit liveness gate. Fix: mount
`/status` (and ideally `/ping`) in `api_router`, or add an nginx `location /status` to the
standalone binary, or fold the status route into `api_routes()`.

### 2. `name` always-present vs upstream-omitted on `/hot-scenes` [MINOR, confirmed, harmless]

Real divergence on the committed tree. Upstream
(`stats/.../hot-scenes-handler.ts:48`) sets `name: scene.metadata?.display?.title`; when the
title is absent the JS object key serializes as omitted (`undefined`). Ours does
`scene.name.unwrap_or_default()` (handlers.rs:303) → always emits `name: ""`. Client consumer
`GoToChatCommand.HotScene` (GoToChatCommand.cs:93-97) declares `name` but reads only
`baseCoords`, so the divergence is inert. Newtonsoft tolerates both present-empty and missing.
No action required beyond noting it.

## Client-crash risks

None confirmed. The one candidate — `hotScenes[0]` on an empty array
(`GoToChatCommand.cs:88`, IndexOutOfRangeException) — is neutralized by the surrounding
`try/catch` in the command dispatcher (`CommandsHandleChatMessageBus.cs:67-72`), which converts
any command exception into a "Error running command" system message. So an empty `/hot-scenes`
response degrades `/goto crowd` to a user-visible error toast, not a crash. This is identical
between catalyrst and upstream in the no-peers case (both return `[]`); it is therefore NOT a
catalyrst-specific regression. The finding's `/hot-scenes` "client receives empty array
`ok:false`" item is accurate as a behavioral note but is not a crash and not a divergence from
upstream.

## Failure-mode gaps

- **`/status` 404 on bundle** (above): the one failure-mode divergence with real client impact.
  Upstream `/status` is always 200 (it lives in the same router as `/hot-scenes` —
  `stats/.../routes.ts:23,25`); ours is 200 only on the standalone binary and 404 on the
  bundle/realm topology that the client actually targets. Genuine gap, not cosmetic.

- **Content DB down → `200 []` instead of upstream 500** (confirmed at content.rs:101 pool-None
  early-return and content.rs:147 query-error swallow): catalyrst degrades to an empty array
  where upstream's content component would throw (500-class). Behaviorally this is *more* robust
  on our side and the client tolerates it (caught toast), so it is an intentional, safe
  divergence — not a gap that harms the client. Worth noting for status-code parity only.

- **Axum default extractor rejections** (malformed/missing JSON on the POST endpoints
  `/heartbeat`, `/auth/challenge`, `/auth/livekit-token`): return plain-text 400/415/422 rather
  than the handler `{error: ...}` shape — confirmed inconsistency, but all three endpoints are
  catalyrst-only and not client-called, so no impact.

- **Gossip / livekit unarmed paths**: confirmed fail-safe. `gossip.verify` → `sign()` →
  `Err(Disabled)` ⇒ 401 (fails closed, gossip.rs:98,122); `livekit.mint` returns
  `token: None` (livekit.rs:71-74) with no panic. The `.expect()` calls (livekit.rs:113/119,
  ws.rs:38) are on always-valid JSON/HMAC-key inputs. No 500s from handlers.

- **Startup**: confirmed panic-free across optional deps. `build_state` (lib.rs:29) never
  unwraps a required external resource; content pool falls back to `None` on unset/connect-fail
  (lib.rs:43-68). Hard-fail only on bad `HTTP_SERVER_PORT`, invalid content DB connection string
  (lib.rs:45 `from_str`), and TcpListener bind — all clean exits before serve.

## Rejected / downgraded

- **`/hot-scenes` framed as a client-crash risk** — rejected: caught by the command dispatcher.
- **`/ping`, `/stats/health`, `/heartbeat`, `/auth/*`, `/gossip/*` divergences** — confirmed
  divergent but correctly rated `none`: never client-called (verified absent from net-catalog;
  only `/comms/peers`, `/status`, `/hot-scenes` are client URLs against this host).
- **`name` divergence as a functional bug** — downgraded to cosmetic/minor: client ignores the
  field.

## Summary

10 endpoints reviewed. One real, client-affecting defect confirmed: `/status` is absent from the
explore bundle (5143) and from the nginx map, so the realm-hosted `archipelago-ea-stats/status`
returns 404; the Unity LiveKit liveness probe treats 404 as failure and (because the parallel
check requires all URLs to pass) blocks client bootstrap behind the LiveKit-down guard. This is
arguably more severe than the "major / liveness semantics" framing in the findings. The `name`
field divergence is real but inert (client ignores it). No client-crash risks survive scrutiny —
the `hotScenes[0]` empty-array path is caught by the chat-command dispatcher. All catalyrst-only
endpoints (auth, gossip, heartbeat, stats/health, ping) are correctly rated none: never called by
the client and fail-safe. The content-DB-down → `200 []` and axum default JSON-rejection shapes
are confirmed but harmless.
