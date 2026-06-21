# Admin Console — Design

The catalyrst admin console is the operator surface for a self-hosted Decentraland
realm. The read-only foundation (`crates/catalyrst-server/src/handlers/console.rs`)
renders server-side HTML for `GET /`, `GET /admin`, and `GET /admin/{service}` from
`AppState` plus a short-TTL probe of sibling bundles over `CATALYRST_SERVICE_URLS`.
On top of it, the **NOW** tranche (implemented in `src/admin/`) adds an Ethereum
auth-chain sign-in and a gated mutation surface (`POST /admin/api/*`). With no admin
env configured the console stays read-only and every mutation 403s (default-safe).

This document describes that console and the roadmap to full server control:

1. an Ethereum auth-chain (SIWE-style) sign-in that gates all mutations,
2. content-local mutation endpoints backed by real `AppState` methods,
3. proxy endpoints to sibling bundles whose backends already exist,
4. a prioritized roadmap separating what is reachable **NOW** from the new
   sibling-service backends that must be built **LATER**.

Design invariants (carried forward from the existing console):

- **SSR-first.** Every value is rendered into HTML server-side. JS only
  progressively enhances (relative timestamps, opt-in auto-refresh, and — new —
  wallet signing + `fetch` for mutations).
- **Every view is a shareable URL** with no hidden client state.
- **Default-safe.** If `ADMIN_ADDRESSES` is unset, *all* mutation endpoints return
  `403` and the UI hides every control. Read-only is the safe default; you must
  opt in to write access.
- `/admin*` is never exposed on the public edge
  (`docs/deploy/nginx-catalyrst-bundles.conf` 404s `/admin`). It is reached on the
  loopback port or over the private network, and now additionally gated by the wallet
  allowlist.

---

## 1. Auth architecture

### Why a session, not per-request signing

Sibling bundles authenticate *users* with a signed-fetch auth-chain
(`x-identity-auth-chain-*` headers, payload `METHOD:PATH:TIMESTAMP:METADATA`,
verified by `catalyrst_crypto::verify::verify_auth_chain`). That works for a
client signing one deploy. For an operator console, signing every button click
with `window.ethereum` is hostile UX. Instead:

1. The operator **signs in once** with an EIP-191 personal-sign over a SIWE-style
   message (one MetaMask prompt).
2. The server verifies the signature recovers an address in `ADMIN_ADDRESSES`.
3. The server mints a **short-lived HMAC-signed session cookie**. No DB, no Redis
   — the cookie is self-describing and integrity-protected by `SESSION_SECRET`.
4. Subsequent mutations are gated by an axum extractor that validates the cookie.

This reuses `catalyrst-crypto` for the one signature and keeps session state
stateless (HMAC), matching the "no hidden client state" invariant.

### Reused crypto

From `catalyrst-crypto` (already a dependency of `catalyrst-server`):

- `catalyrst_crypto::recover::recover_address(message: &[u8], signature: &str) -> Result<String, AuthError>`
  — recovers the signer from the EIP-191 personal-sign over the sign-in message.
  This is the *only* crypto call needed for sign-in (a plain wallet signature, not
  a full chain). Note `recover_address` already applies the EIP-191
  (`hash_message`) prefix internally, matching `personal_sign` / `eth_sign`
  semantics used by `window.ethereum`.
- `catalyrst_crypto::verify::verify_auth_chain` — available if we later want to
  accept a full auth-chain (ephemeral-key) sign-in instead of a raw personal-sign;
  not required for the NOW tranche.

### Session cookie

- Cookie name: `cat_admin`.
- Value: `base64url(payload) "." base64url(hmac_sha256(SESSION_SECRET, payload))`
  where `payload = {addr, exp}` JSON (lowercased address, unix-seconds expiry).
- HMAC via `sha2` (already a workspace dep) + a tiny `hmac` impl, or the `hmac`
  crate. TTL default 12h (`ADMIN_SESSION_TTL_SECS`).
- Flags: `HttpOnly; SameSite=Strict; Path=/; Secure`. `Secure` is **on by default**;
  set `ADMIN_COOKIE_INSECURE=1` only for a plain-HTTP private network with no TLS terminator
  (browsers treat localhost as a secure context, so loopback dev works as-is).

### Env vars

| Env | Meaning | Default |
|---|---|---|
| `ADMIN_ADDRESSES` | comma-separated `0x…` allowlist | unset ⇒ console is read-only, all mutations 403 |
| `SESSION_SECRET` | HMAC key for the session cookie + sign-in nonce | unset ⇒ mutations 403 (treated same as no admin) |
| `ADMIN_SESSION_TTL_SECS` | session lifetime | `43200` (12h) |
| `ADMIN_COOKIE_INSECURE` | drop the cookie `Secure` flag (plain-HTTP private network only) | unset ⇒ `Secure` set |
| `COMMS_MODERATOR_TOKEN` (or `MODERATOR_TOKEN`) | bearer forwarded to comms for ban/warn/unban proxy | unset ⇒ social controls hidden |
| `AB_REGISTRY_ADMIN_TOKEN` (or `API_ADMIN_TOKEN`) | bearer forwarded to ab-registry for re-ingest / flush-cache | unset ⇒ create controls hidden |
| `DEBUGGING_SECRET` | secret injected into the scene-state reload body | unset ⇒ scene controls hidden |
| telemetry `/dash/*` | no token — loopback-trusted (see §3); MUST be firewalled to loopback/private network | — |

> The console forwards each downstream token; it accepts either its own env name or
> the sibling service's own name (e.g. comms reads `MODERATOR_TOKEN`, ab-registry
> reads `API_ADMIN_TOKEN`) so a single-host deploy needn't set the same value twice.

### Auth flow (endpoints, all in `catalyrst-server`)

```
GET  /admin/auth/nonce?address=0x… → { message }     (host+address+expiry-bound SIWE message to sign)
POST /admin/auth/verify            → sets cat_admin cookie (body: {message, signature})
POST /admin/auth/logout            → clears cookie
GET  /admin/auth/me                → { address } | 401 (UI uses this to decide what to render)
```

The sign-in message is bound to the serving **host**, the signing **address**, and a
5-minute **expiry**; its `Nonce:` is `HMAC(SESSION_SECRET, host|address|exp)`, so it
is stateless (no nonce store), cannot be replayed against another host or a different
address, and `verify` re-checks the host, expiry, nonce HMAC, and that the recovered
signer equals the message address before minting the cookie. Mutations additionally
require a same-origin `Origin`/`Referer` when present (defense-in-depth CSRF).

`GET /admin` and `GET /admin/{service}` stay **viewable without auth** (read-only
status is not sensitive and the page must remain shareable). Only the *control
forms* within them render for an authenticated admin, and only the
`POST /admin/api/*` mutation routes enforce the extractor.

### The gate

An axum extractor `AdminSession` (in a new `auth.rs` module) implemented via
`FromRequestParts`:

1. Parse `cat_admin` cookie → verify HMAC with `SESSION_SECRET` → check `exp`.
2. Check `addr ∈ ADMIN_ADDRESSES`.
3. On failure return `403`.

Every `POST /admin/api/*` handler takes `AdminSession` as its first extractor, so
the gate is impossible to forget. If `ADMIN_ADDRESSES` or `SESSION_SECRET` is
unset, the extractor short-circuits to `403` (default-safe).

---

## 2. Cross-service proxy-auth strategy

The console lives in `catalyrst-server` (`:5141`). Many controls target sibling
bundles (`:5143` explore, `:5145` social, `:5146` data, telemetry, …). The console
calls them over `CATALYRST_SERVICE_URLS` (the same map the read-only probe already
uses). Three downstream-auth shapes exist:

1. **Loopback-trusted (no downstream auth).** The bundle's admin endpoint has no
   auth and is meant for loopback/private network (e.g. telemetry `POST /dash/issue/state`,
   `POST /dash/sql`; ab-registry `POST /registry`, `DELETE /flush-cache`). The
   console gates the *operator* with `AdminSession`, then forwards the request
   verbatim. **Trust boundary:** the bundle must not be reachable from the public
   edge; the console is the only authenticated front door.

2. **Bearer-forwarded.** The bundle's admin endpoint is bearer-gated (e.g. comms
   `POST/DELETE /users/{address}/bans`, `POST /users/{address}/warnings` accept
   `Authorization: Bearer <MODERATOR_TOKEN>`). The console holds the token in
   `COMMS_MODERATOR_TOKEN` env and forwards it. The operator never sees the token;
   `AdminSession` is the operator gate.

3. **Signed-fetch-required (NOT proxied in NOW).** The bundle's admin endpoint
   requires a real user auth-chain signature from a privileged address (e.g.
   ab-registry `POST/DELETE /denylist/{id}` checks the signer against
   `denylist_moderators`; places `PUT /…/highlight|rating` check `admin_addresses`).
   The server cannot forge a user signature, and the session cookie is not an
   auth-chain. These are deferred: the right fix is for those bundles to *also*
   accept a bearer (parity work in §4), after which they become case 2. For NOW
   the console links to them but does not proxy them.

Proxy mechanics: reuse the existing `reqwest::Client` pattern in `console.rs`
(2s connect/timeout). Each proxy handler builds the downstream URL from
`service_urls().get(key)`, forwards method+body, attaches the bearer when
required, and returns `{ ok, status, body }` JSON to the browser. If the bundle
key is not configured, return `502 not-configured` so the UI can show "service
offline" rather than a silent failure.

---

## 3. Capability matrix

Status legend: **EB** = exists-backend (method/route already callable),
**ENU** = exists-but-no-ui, **NNB** = needs-new-backend.
Approach: **CL** = content-local (AppState method), **PB** = proxy-to-bundle,
**NER** = new-endpoint-required.
Tranche: **NOW** = in the three work-orders below; **LATER** = roadmap §4.

### content-core (catalyrst-server, local AppState)

| Control | Status | Approach | Tranche | Notes |
|---|---|---|---|---|
| View sync / cluster / failed deployments | EB | CL | NOW (already SSR) | `synchronization_state`, `content_cluster`, `database.get_failed_deployments` |
| View denylist / snapshots / challenge | EB | CL | NOW (already SSR) | read methods exist |
| Flush deployments cache | EB | CL | **NOW** | `AppState.deployments_cache` is `DashMap`; add `.clear()` behind `POST /admin/api/content/flush-cache` |
| Retry failed deployments | NNB | CL | LATER | `Deployer` trait has no retry method |
| Clear failed deployments | NNB | CL | LATER | `Database` trait is read-only |
| Add/remove denylist entry | NNB | CL | LATER | `Denylist` trait only has `is_denylisted()` |
| Trigger snapshot regen | NNB | CL | LATER | `SnapshotGenerator` only has `get_current_snapshots()` |
| Toggle read-only/write | NNB | CL | LATER | `AppState.read_only` is immutable; needs atomic flag + handler gate |
| Refresh challenge | NNB | CL | LATER | `ChallengeSupervisor` has no refresh method |
| Pause/resume/force sync | NNB | CL | LATER | `SynchronizationState` has no control methods |
| Accepting-users allowlist | NNB | CL | LATER | no trait exists |

### explore (catalyrst-explore :5143 — places/events/worlds/archipelago/lists)

| Control | Status | Approach | Tranche | Notes |
|---|---|---|---|---|
| View islands/parcels/peers/hot-scenes/health | EB | CL/PB | NOW (already SSR detail) | probed today |
| Highlight place/world | EB | PB (signed-fetch) | LATER | `PUT /api/places/{id}/highlight` needs admin signature → parity bearer first |
| Set place/world rating | EB | PB (signed-fetch) | LATER | same |
| Set place/world ranking | EB | PB (bearer) | **NOW-eligible** | `PUT /api/places/{id}/ranking` already bearer-gated (data-team token) — proxiable once token is wired; ships in roadmap if token configured |
| Featured place/world on/off | EB | PB (bearer) | **NOW-eligible** | `PUT/DELETE /api/places/{id}/featured` bearer-gated |
| View moderation reports | EB | CL | LATER | persisted but no GET query endpoint |
| Report queue / resolve / export | NNB | CL | LATER | no report query/mutation API |
| POI add/edit/remove/import/audit | NNB | CL | LATER | `/pois` is read-only |
| Event approve/edit/feature/archive | NNB | CL | LATER | `PATCH /api/events/{id}` returns not-implemented |
| World bans / scene bans (view+mutate) | EB | PB | LATER | live in comms-gatekeeper; catalyrst-worlds only reads |
| Adjust island params / eject peer | EB/NNB | CL | LATER | island config read-only; no eject endpoint |

### create (catalyrst-create :5144 — ab-registry/camera-reel/builder)

| Control | Status | Approach | Tranche | Notes |
|---|---|---|---|---|
| View AB build queues / registry status | EB | PB | NOW (already SSR) | `/queues/status`, `/status` |
| Re-ingest registry manifests | EB | PB (loopback/bearer) | **NOW** | `POST /registry` exists (admin bearer) |
| Flush AB manifest/LOD cache | EB | PB (loopback/bearer) | **NOW** | `DELETE /flush-cache` exists |
| View denylist | EB | PB | NOW | `GET /denylist` |
| Add/remove AB denylist entry | EB | PB (signed-fetch) | LATER | `POST/DELETE /denylist/{id}` requires moderator signature → parity bearer first |
| Re-trigger / pause AB build queue | NNB | NER | LATER | no requeue logic |
| Camera-reel moderator delete / flag | NNB | NER | LATER | only owner-delete exists |
| Builder item approve/reject/bulk | NNB | NER | LATER | item routes read-only |

### social (catalyrst-social :5145 — communities/comms/notifications/badges)

| Control | Status | Approach | Tranche | Notes |
|---|---|---|---|---|
| Ban / unban user globally | EB | PB (bearer) | **NOW** | comms `POST/DELETE /users/{address}/bans` accept `MODERATOR_TOKEN` bearer |
| Issue user warning | EB | PB (bearer) | **NOW** | comms `POST /users/{address}/warnings` (bearer) |
| View user bans / warnings | EB | PB (bearer) | **NOW** | comms `GET /users/{address}/bans`, `/warnings` (bearer) |
| List/filter all global bans | EB | CL | LATER | `GET /bans` exists but no filter/pagination |
| Community list / details | EB | PB | NOW (SSR) | `GET /v1/communities` |
| Community suspend/unsuspend | NNB | NER | LATER | no suspension state |
| Broadcast notifications | NNB | NER | LATER | no admin broadcast endpoint |
| Badge grant/revoke | ENU | PB | LATER | badges crate is read-only |
| Scene ban / scene admin add-remove | EB | PB (signed-fetch) | LATER | comms scene routes require SCENE_SIGNER signature |
| Voice: end/kick/mute/promote | EB | PB (bearer when gated) | **NOW-eligible** | comms voice routes bearer-gated when `COMMS_GATEKEEPER_AUTH_TOKEN` set |

### data (catalyrst-data :5146 — market/economy/price/credits/rpc)

| Control | Status | Approach | Tranche | Notes |
|---|---|---|---|---|
| View bundle health | EB | PB | NOW (SSR) | `/health` |
| View bids/orders/trades | ENU | PB | NOW | `GET /v1/federation/*` (read-only) |
| Credits seasons/goals CRUD | NNB | NER | LATER | no admin season routes; needs schema |
| Grant/revoke credits, block user | NNB | NER | LATER | financial; needs admin routes + auth |
| Price override | NNB | NER | LATER | price service is read-only |
| RPC method allowlist / networks | ENU | CL | LATER | hardcoded `READ_ONLY_METHODS`; needs dynamic config |
| Relayer on/off, signer switch | EB | CL | LATER | startup-only |
| Moderate listings/trades, force-cancel | NNB | NER | LATER | needs schema + operator override |
| Federation audit log | NNB | NER | LATER | no audit table |

### realtime (catalyrst-social-rpc :5148 / scene-state :5153 / comms :5145)

| Control | Status | Approach | Tranche | Notes |
|---|---|---|---|---|
| List loaded scenes + connection counts | EB | PB | NOW (SSR) | scene-state `GET /status` |
| Reload/restart a scene | EB | PB (secret) | **NOW** | scene-state `POST /debugging/reload` (DEBUGGING_SECRET) |
| Global ban / warnings / view history | EB | PB (bearer) | **NOW** | via comms (same as social) |
| End private/community voice; kick/mute | EB | PB (bearer) | **NOW-eligible** | comms gatekeeper bearer routes |
| Disconnect user / presence inspect/force | NNB | NER | LATER | social-rpc has no admin query/mutation |
| Friendship graph view / reset | NNB | NER | LATER | social-rpc user-only |
| Kick-all from scene / inspect CRDT / reset state | NNB | NER | LATER | scene-state lacks these |

### explorer-api (catalyrst-explorer-api :5137)

| Control | Status | Approach | Tranche | Notes |
|---|---|---|---|---|
| Query blocklist / flags / auth health | EB | PB | NOW (SSR) | `GET /denylist.json`, `/{app}`, `/auth/health/live` |
| Toggle feature flag (in-memory) | EB | PB | **NOW-eligible** | flags in `RwLock`; needs a `POST` flag-toggle route on explorer-api (small; see §4) — links NOW, mutates LATER |
| Add/remove blocklist wallet | NNB | CL(remote) | LATER | blocklist read-only on explorer-api |
| Reload flags / blocklist from disk | NNB | CL(remote) | LATER | no reload route |
| Realm/upstream config edit, TTL config | EB | CL(remote) | LATER | startup-only config |
| View/revoke auth challenges, identities | EB | CL(remote) | LATER | in-memory, no introspection route |

### telemetry (catalyrst-telemetry)

| Control | Status | Approach | Tranche | Notes |
|---|---|---|---|---|
| View dashboard / run read-only SQL | EB | PB (loopback) | NOW (SSR + form) | `POST /dash/sql` |
| Resolve/ignore/unresolve issue, assign, note | EB | PB (loopback) | **NOW** | `POST /dash/issue/state` |
| Data retention / purge | NNB | NER | LATER | unbounded growth risk |
| Ingest enable/disable, per-project quota | NNB | CL(remote) | LATER | no runtime toggle |
| Bulk delete / export / rate-limit / sampling | NNB | NER | LATER | — |
| Issue audit/history, bulk ops, regroup, releases | NNB | NER | LATER | — |

---

## 4. Roadmap

### NOW — shipped by the three work-orders below

**Foundation (WO-1):** SIWE-style sign-in, HMAC session cookie, `AdminSession`
extractor, `ADMIN_ADDRESSES` allowlist, default-safe 403. Read-only `/admin` views
stay viewable.

**Content-local mutations (WO-2), real AppState methods only:**
- `POST /admin/api/content/flush-cache` → `deployments_cache.clear()`.

**Proxy mutations (WO-2), backends already exist:**
- Telemetry (loopback-trusted): `POST /admin/api/telemetry/issue-state` →
  `POST {telemetry}/dash/issue/state`; `POST /admin/api/telemetry/sql` →
  `POST {telemetry}/dash/sql`.
- AB registry (loopback/bearer): `POST /admin/api/create/registry-reingest` →
  `POST {create}/registry`; `POST /admin/api/create/flush-ab-cache` →
  `DELETE {create}/flush-cache`.
- Comms moderation (bearer-forwarded via `COMMS_MODERATOR_TOKEN`):
  `POST /admin/api/social/user-ban`, `DELETE …/user-ban`,
  `POST /admin/api/social/user-warning` → comms `/users/{address}/bans|warnings`.
- Scene-state (secret): `POST /admin/api/scene/reload` →
  `POST {scene-state}/debugging/reload` with `DEBUGGING_SECRET`.

Each proxy renders a control only when (a) the operator is authenticated and
(b) the target bundle key is configured and (c) the required token/secret env is
present. Otherwise the form is hidden and the endpoint returns 502/403.

### LATER — new backend endpoints needed in sibling services

These are blocked on backend work in the *owning* crate. Grouped by crate so each
can be a self-contained follow-up. The general pattern for each: add a bearer-gated
(`Authorization: Bearer`) admin route in the sibling crate so the console can
proxy it via case-2 auth (avoiding signature forgery).

- **catalyrst-server (content-core):** extend traits in `state.rs` —
  `Database::clear_failed_deployments()`, `Deployer::retry(id)`,
  `Denylist::{add,remove,list}`, `SnapshotGenerator::trigger_regeneration()`,
  `ChallengeSupervisor::refresh()`, `SynchronizationState::{pause,resume,force}`,
  runtime-mutable `read_only` (atomic), and a new `AcceptingUsers` trait.
- **catalyrst-places:** `GET /api/reports` + `PATCH /api/reports/{id}` (moderation
  queue); `DELETE`/soft-delete places; bearer parity on `highlight`/`rating` so the
  console can proxy them; POI CRUD (`POST/PATCH/DELETE /api/pois`).
- **catalyrst-events:** implement `POST/PATCH /api/events` (approve/edit/feature/
  archive) — currently stubbed not-implemented.
- **catalyrst-worlds:** mutation proxies to comms-gatekeeper for world/scene bans;
  access-log query endpoints.
- **catalyrst-ab-registry:** `POST /queues/retry`, queue pause/resume;
  bearer parity on `/denylist`.
- **catalyrst-camera-reel:** moderator `DELETE /admin/images/{id}`, flag/review
  column + `PATCH`.
- **catalyrst-builder:** item curation `PATCH /v1/collections/{id}/items/{item}/status`
  + bulk.
- **catalyrst-communities:** community suspend/unsuspend; admin list filters.
- **catalyrst-notifications:** admin broadcast endpoint.
- **catalyrst-badges:** grant/revoke mutation endpoints.
- **catalyrst-social-rpc:** admin read (friendship graph, presence, active calls)
  + disconnect/force-presence/reset-settings.
- **catalyrst-scene-state:** kick-all, CRDT inspect, reset-state.
- **catalyrst-credits:** seasons/goals CRUD, grant/revoke/block (high-risk; needs
  schema + strict audit).
- **catalyrst-price / economy / rpc:** dynamic config stores (override, method
  allowlist, relayer toggle).
- **catalyrst-market:** moderation flags, dispute status, operator force-cancel,
  audit log (schema work).
- **catalyrst-explorer-api:** flag toggle/reload routes, blocklist mutate/reload,
  runtime config store, auth-challenge introspection.
- **catalyrst-telemetry:** retention/purge, ingest toggle, per-project quota, bulk
  delete/export, issue history audit, regroup, release state.

A shared concern across all LATER items: an **audit log** (who did what, when).
Recommend a single `admin_audit` table in the shared cluster that every mutation
(content-local and proxied) writes to from `catalyrst-server`, keyed by the
authenticated admin address from `AdminSession`.

---

## 5. Implementation status — LATER tranche shipped

The §4 LATER roadmap has now been implemented across the owning crates. Each new
sibling-crate admin route is **bearer-gated** (constant-time compare, fail-closed
when its token env is unset) and proxied by `catalyrst-server` behind the
`AdminSession` gate; the shared **`admin_audit`** table
(`crates/catalyrst-server/migrations/0002_admin_audit.sql`) is implemented and
every console mutation — content-local and proxied — records a row keyed by the
authenticated wallet address.

**Genuinely implemented (real state/DB writes, gated + audited):** content-core
clear-failed-deployments, denylist add/remove/list (in-process store), challenge
refresh, runtime read-only toggle, accepting-users; places reports queue +
resolve + disable + POI CRUD + highlight/rating (bearer parity); events
create/moderate; worlds admin views + enable/disable; ab-registry queue
pause/resume + denylist bearer parity; camera-reel moderator delete + review
flag; builder item curation; communities suspend/unsuspend + filters;
notifications broadcast; badges grant/revoke; credits seasons/goals CRUD +
grant/revoke/block (NUMERIC-exact, transactional, ledgered); price override;
rpc method-allowlist/networks config; social-rpc presence/voice/friendships +
disconnect/force-presence/reset-settings; scene-state kick-all/crdt-inspect/reset;
explorer-api flag toggle/reload + blocklist + runtime config + challenge
introspection; telemetry purge/ingest-toggle/quota/bulk-delete/export/audit/
regroup/release; economy runtime relayer toggle + signer-preference switch.

**Now fully wired (previously partial):** content sync pause/resume/force is
consulted by the sync orchestrator — pause parks every download pass at a clean
boundary (resumable from the persisted frontier, no lost/duplicated deployments),
resume/force wake it immediately. AB queue-retry performs a real DB enqueue
(`build_jobs` rows reset to `pending`, surfaced in `/queues/status`) — note an
**external build runner (abgen) still claims those jobs**; the console enqueues,
it does not itself run the build worker.

**Config-store only (by design):** the price override is stored exactly (NUMERIC,
audited) but is **not applied to the served `/api/v3/simple/price` output** — it
records operator intent; wiring it into the served price would be a separate,
explicit change.

**Backend present but not surfaced in the UI:** content retry-failed-deployments
and snapshot-regeneration return 501 in the read-only live binary (their live
trait impls don't override the default), so their buttons are omitted; the
endpoints remain for a write-capable build.

**Skipped (with reason):** mutating the upstream read-only `squid_marketplace` /
archive `event` corpora (those schemas are SELECT-only / on-chain — admin scope
is limited to this node's local overlay/federation tables); trade force-cancel
(settled on-chain transfers are irreversible — use the dispute lifecycle);
runtime private-key hot-swap for the economy signer; durable persistence of the
economy relayer toggle (process-local by design).

### New per-crate admin token env vars

Each sibling crate's new admin routes check a bearer token from its own env var;
`catalyrst-server` forwards it (accepting the sibling's own name as a fallback):

| Crate | Token env (console → sibling fallback) |
|---|---|
| catalyrst-places | `PLACES_ADMIN_AUTH_TOKEN` |
| catalyrst-events | `CATALYRST_EVENTS_ADMIN_TOKEN` |
| catalyrst-worlds | `CATALYRST_WORLDS_ADMIN_TOKEN` |
| catalyrst-ab-registry | `AB_REGISTRY_ADMIN_TOKEN` → `API_ADMIN_TOKEN` |
| catalyrst-camera-reel | `CATALYRST_CAMERA_REEL_ADMIN_TOKEN` |
| catalyrst-builder | `CATALYRST_BUILDER_ADMIN_TOKEN` |
| catalyrst-communities | `API_ADMIN_TOKEN` |
| catalyrst-notifications | `CATALYRST_NOTIFICATIONS_ADMIN_TOKEN` |
| catalyrst-badges | `CATALYRST_BADGES_ADMIN_TOKEN` |
| catalyrst-social-rpc | `CATALYRST_SOCIAL_RPC_ADMIN_TOKEN` |
| catalyrst-scene-state | `CATALYRST_SCENE_STATE_ADMIN_TOKEN` / `DEBUGGING_SECRET` |
| catalyrst-credits | `CATALYRST_CREDITS_ADMIN_TOKEN` |
| catalyrst-price | `CATALYRST_PRICE_ADMIN_TOKEN` |
| catalyrst-rpc | `CATALYRST_RPC_ADMIN_TOKEN` |
| catalyrst-economy | `CATALYRST_ECONOMY_ADMIN_TOKEN` |
| catalyrst-market | `CATALYRST_MARKET_ADMIN_TOKEN` |
| catalyrst-explorer-api | `CATALYRST_EXPLORER_API_ADMIN_TOKEN` |
| catalyrst-telemetry | `CATALYRST_TELEMETRY_ADMIN_TOKEN` (else loopback-trusted) |
| comms (social) | `COMMS_MODERATOR_TOKEN` → `MODERATOR_TOKEN` |

A control card renders only when the operator is authenticated, the target bundle
is configured in `CATALYRST_SERVICE_URLS`, and its token env is present.

> **Deploy invariant:** sibling admin ports must stay loopback/private-network-only — the
> bearer tokens (and telemetry's token-less loopback surface) are the only thing
> in front of these routes besides network isolation. The console is the
> authenticated front door; do not expose sibling `/admin/*` on the public edge.

### Resolved follow-ups (from earlier security review)

- Audit "actor" is now taken from the console-set `X-Catalyrst-Admin` header
  (telemetry/price/economy/badges/credits) rather than a client-supplied label;
  price and economy runtime toggles now write audit rows. The header is used for
  attribution only (never authz) and is forwarded only from the `AdminSession`
  path after EVM-address validation; behind the sibling bearer gate, attribution
  is best-effort by design (sibling admin ports stay loopback/private-network-only).
- `catalyrst-price` override is now stored as exact NUMERIC (no `f64`).
- `credits` grant accepts an idempotency key; a replay with the same
  address+amount returns the prior result, and a key reused for a *different*
  grant is rejected with `409` (no double-apply, no cross-grant balance leak).
