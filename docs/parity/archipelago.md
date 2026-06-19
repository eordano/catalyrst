# Parity report: catalyrst-archipelago vs archipelago-workers

Service: **archipelago** (Rust crate `catalyrst-archipelago`).
Upstream: `decentraland/archipelago-workers` (`stats/` HTTP plane + `ws-connector/` comms plane).
LiveKit minting upstream lives in a separate service, `decentraland/comms-gatekeeper`.

Verification method: read both the Rust structs/handlers and the upstream TS
handlers; cross-checked the Unity net-catalog
(`the Unity net-catalog`) for what the client actually
calls. Live diff was not applicable (upstream archipelago-ws not running locally).

## What the Unity client actually calls (net-catalog, ground truth)

- `GET https://archipelago-ea-stats.decentraland.{ENV}/comms/peers`
- `GET https://archipelago-ea-stats.decentraland.{ENV}/status`
- `GET https://archipelago-ea-stats.decentraland.{ENV}/hot-scenes`
- LiveKit tokens: `comms-gatekeeper.decentraland.{ENV}/...` (NOT archipelago)
- Comms session: `SIGNED_POST` to the archipelago/fixed adapter URL (the ws-connector protobuf plane)

Upstream stats router (`stats/src/controllers/routes.ts`) registers peers/islands/parcels
under both `''` and `/comms` prefixes; `/status`, `/core-status`, `/hot-scenes` at root.
Our routes are mounted with **no prefix** at `/stats/*` (verified `main.rs`, `lib.rs`),
so none of the client's stats URLs (`/comms/peers`, `/hot-scenes`, `/status`) match
ours except `/status` (which the client only probes, see below).

## Per-endpoint summary

| Endpoint (ours) | Shape | Efficiency | Severity | Notes |
|---|---|---|---|---|
| GET /ping | divergent | same | none | Catalyrst-only health route. Not in api_routes. No client impact. |
| GET /status | divergent | same | minor | Path matches client probe, but body fully differs; client only does an availability probe (method OTHER), does not parse fields. Missing CORS header. |
| GET /stats/health | divergent | same | minor | Catalyrst-only path. No `/stats/health` upstream; closest is `/core-status`. No client call. |
| GET /stats/peers | divergent | same | breaks-client | PATH mismatch (client hits `/comms/peers`) + wrapper + per-peer shape diff. Confirmed. |
| GET /stats/islands | divergent | better* | major | PATH + wrapper + per-island shape. "better" is a data-losing shortcut, not a real win. |
| GET /stats/islands/{id} | divergent | better | major | O(1) map lookup vs upstream O(N) linear scan — real structural win, but body still divergent. |
| GET /stats/hot-scenes | divergent | worse | breaks-client | PATH mismatch (client hits `/hot-scenes`) + bare-array-of-scene vs object-of-parcel + no scene resolution. Confirmed. |
| POST /heartbeat | divergent | same | none | Catalyrst-only REST mutation; upstream ingests via WS->NATS. No client REST analog. |
| POST /auth/challenge | divergent | same | minor | Catalyrst-only REST; challenge lacks upstream `dcl-` prefix. No client REST call. |
| POST /auth/livekit-token | divergent | same | major->minor | Different service (gatekeeper); client never calls archipelago for this. Severity overstated (see rejects). |
| POST /gossip/heartbeat | divergent | worse | none | Catalyrst-only inter-node HTTP transport vs upstream NATS. No client/shape analog. |
| GET /gossip/info | divergent | same | none | Catalyrst-only diagnostics. No upstream analog. |
| GET /ws | divergent | same | breaks-client | JSON-text vs binary-protobuf wire protocol. Confirmed not wire-compatible. |

\* `/stats/islands` "better" rejected as an efficiency *win* — see rejects.

## Confirmed shape issues

1. **GET /stats/peers — PATH + body, breaks-client.** Client calls `/comms/peers`;
   we only serve `/stats/peers` with no `/comms` or `/peers` alias, so the client's
   URL 404s against us. Body wrapper differs (`{ok,peers}` upstream vs `{peers,total}`
   ours). Per-peer: upstream `PeerResult{id,address,lastPing,parcel,position}`
   (`peers-handler.ts:4`), ours `PeerOut{address,parcel,position,realm,island_id}`
   (`handlers.rs:94`). Missing `id`, `lastPing`; extra `realm`, `island_id`. Upstream
   supports repeated `?id` filter; ours ignores it. All confirmed.

2. **GET /stats/hot-scenes — PATH + structure, breaks-client.** Client calls
   `/hot-scenes`; we serve `/stats/hot-scenes` (PATH divergence **not noted in the
   original finding** — added here). Upstream returns a **bare array** of per-scene
   `HotSceneInfo{id,name,baseCoords,usersTotalCount,parcels[],thumbnail?,...}`
   (`hot-scenes-handler.ts:7`), resolved via a content-server call. Ours returns
   `{hot_scenes:[{parcel,peers_count,peer_names}],total}` — per-parcel-tile, zero
   scene metadata (`cluster.rs:42`). Different granularity, missing every scene-identity
   field. Confirmed.

3. **GET /ws — wire protocol, breaks-client.** Upstream `/ws` decodes binary
   `ClientPacket` protobuf (`ws-handler.ts:119`), challenge prefixed `'dcl-'`
   (`ws-handler.ts:147`), republishes heartbeats to NATS `peer.*.heartbeat`. Ours is
   JSON text frames with a `welcome/auth_ok/.../heartbeat` envelope (`ws.rs`). The real
   Unity client speaks protobuf and cannot decode our text frames. `island_changed`
   embeds a minted LiveKit grant inline (`cluster.rs:51`), which has no upstream
   protobuf analog. Confirmed. (Minor narrative nit: upstream has 3 handshake stages —
   HANDSHAKE_START / HANDSHAKE_CHALLENGE_SENT / HANDSHAKE_COMPLETED — not 4 as the
   finding loosely phrased; the load-bearing protobuf-vs-JSON claim is correct.)

4. **GET /stats/islands and /stats/islands/{id} — body, major.** Upstream
   `IslandResult{id,peers:[PeerResult],maxPeers,center,radius}` (`islands-handler.ts:12`),
   ours `Island{id,center,radius,peers_count,peers:[address string]}` (`cluster.rs:33`).
   Missing `maxPeers`; `peers` is bare address strings not full peer objects; extra
   `peers_count`. Wrapper differs for the list (`{ok,islands}` vs `{islands,total}`);
   the single-island route has no wrapper either side (200/404). 404: ours returns
   text "island not found", upstream returns no body. PATH: client/upstream use
   `/comms/islands` + `/islands`; ours `/stats/islands` only. Confirmed. The Unity client
   does not appear in the net-catalog calling islands directly, so severity is major
   (not breaks-client) — it is a stats/diagnostic surface.

5. **GET /status — body + CORS, minor.** Upstream `{version,currentTime,commitHash}`
   with `Access-Control-Allow-Origin:*` (`status-handler.ts`). Ours
   `{name,version,healthy,peers_total,...}` snake_case, no CORS header. The client only
   *probes* this path for availability (net-catalog method OTHER), it does not consume
   the body, so client impact is minor.

6. **Semantic: parcel derivation (minor, not in original findings).** Upstream derives
   `parcel = [floor(x/16), floor(z/16)]` from world position (`logic/utils.ts`). Ours
   stores the client-supplied `parcel` array verbatim (`handlers.rs:97`). Both expose a
   `parcel:[x,y]` field but the value is computed differently; could drift if a client
   sends an inconsistent parcel/position pair.

## Confirmed efficiency findings

1. **GET /stats/islands/{id} — "better" (real, structural).** Ours does an O(1) indexed
   `HashMap::get(id)` (`cluster.rs:163`). Upstream linearly scans the islands array to
   find the match, then runs `processIsland` (`islands-handler.ts:82`). Indexed lookup
   vs linear scan is a genuine structural difference, not a language artifact. The win
   is real, though the response body is still divergent (data-losing), so the endpoint
   is flagged divergent overall.

2. **GET /stats/hot-scenes — "worse" (real, structural).** Ours does **zero**
   content-server resolution; it cannot, because it emits a structurally different
   per-parcel result. Upstream makes ONE batched `fetchEntitiesByPointers` call (POST
   /content/entities/active, N+1 avoided — verified `adapters/content.ts:22`) to resolve
   scene metadata + thumbnails. Ours is "cheaper" only by skipping required work and
   returning the wrong shape; correctly classified as worse, not a win.

3. **POST /gossip/heartbeat — "worse" (real, structural, transport-level).** Signed HTTP
   POST fan-out (HMAC-SHA256 verify per batch + per-node seq dedup + O(batch) DashMap
   upserts — `gossip.rs:107,129`) is heavier than upstream's NATS pub/sub fan-out
   (`ws-handler.ts:272` publishes to a persistent broker, no per-message HTTP framing or
   HMAC). This is an architecture/transport difference, not a DB difference; severity is
   none because it is catalyrst-only infra with no client exposure.

## Rejected / corrected during verification

- **`/stats/islands` efficiency "better" — REJECTED as a win.** The basis is that ours
  emits bare address strings and skips the per-peer `peers.get()` join upstream does in
  `processIsland`. That is a data-losing structural shortcut (drops full peer objects),
  not a legitimate efficiency improvement. The original finding already hedged this
  ("not a free win"); I downgrade the verdict: efficiency is effectively N/A, not a
  genuine "better". (The `/stats/islands/{id}` map-lookup-vs-linear-scan win is separate
  and stands.)

- **`/auth/livekit-token` severity "major" — CORRECTED to minor.** The shape comparison
  is against `comms-gatekeeper`, a different service. The net-catalog confirms the Unity
  client mints LiveKit tokens from `comms-gatekeeper.decentraland.{ENV}` and never calls
  archipelago for this. A catalyrst-only path that no client hits cannot be "major" on a
  client-impact axis; it is at most minor.

- **`/auth/livekit-token` shape claim "upstream returns a bare LiveKit AccessToken JWT
  string" — PARTIALLY REJECTED.** Gatekeeper actually returns a wrapper
  `LivekitCredentials{url, token}` (`comms-gatekeeper/src/adapters/livekit.ts:102-105`),
  not a bare JWT. Ours returns `{url, room, identity, token?, expires_at}` — also a
  wrapper, and the embedded `video` grant matches gatekeeper's
  `{roomJoin,canPublish,canSubscribe,canPublishData}` (`livekit.rs:21`). So ours is
  closer to gatekeeper than "wrapper vs bare JWT" implied; the real diffs are the extra
  `room`/`identity`/`expires_at` fields and `token` being optional. The "different
  service" point stands.

- **`/ws` "4-stage handshake" — MINOR CORRECTION.** Upstream has 3 stages
  (HANDSHAKE_START, HANDSHAKE_CHALLENGE_SENT, HANDSHAKE_COMPLETED — `ws-handler.ts`),
  not 4. Does not change the breaks-client verdict (protobuf vs JSON text is the real
  incompatibility).

- **All "no upstream equivalent" claims for `/ping`, `/heartbeat`, `/auth/challenge`,
  `/gossip/*` — CONFIRMED.** Verified against `stats/controllers/routes.ts` (only
  parcels/peers/islands/core-status/status/hot-scenes) and `ws-connector` (protobuf WS +
  NATS). These are genuinely catalyrst-only surfaces.
