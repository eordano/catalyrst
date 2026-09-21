# Operations - networking, admin console, postgres, LiveKit, observability

## Networking, firewall, sandboxing

Module references: [firewall rules](../nixos/firewall.nix),
[reverse proxy](../nixos/web.nix) and [service sandboxes](../nixos/sandbox.nix).
HTTP and WebSocket traffic use the reverse proxy; LiveKit RTC and Pulse use
their own listeners.

| Port | Proto | Service | Notes |
|---|---|---|---|
| 22 | TCP | sshd | key-only + brute-force protection |
| 80/443 | TCP | reverse proxy | add a CDN source restriction in CDN-backed host configurations |
| 7881 | TCP | LiveKit RTC | TCP fallback when UDP fails |
| 7777 | UDP | Pulse (ENet) | authoritative game server |
| 7882 | UDP | LiveKit media | SFU media |

For a CDN-backed deployment, restrict 80/443 to CDN ranges in the host firewall
and feed the same list to the proxy's `real_ip` config. The module's
`openFirewall = true` opens these ports without a CDN source restriction; add
that restriction in the host configuration. The module also opens LiveKit TCP
7880 when comms is enabled, so account for that listener in your edge policy.

"Peers in roster but no remote avatars" (also `/rtc` 502): inbound UDP to the SFU dropped, DTLS times out while signaling stays healthy. Fix: open/forward SFU UDP range, or set LiveKit `node_ip` to a reachable, non-NATed address, then restart the SFU; STUN to a blocking host fails silently.

Cloudflare/CDN IP refresh - two decoupled sources of truth:

- Proxy `real_ip` include: refreshed daily from `https://www.cloudflare.com/ips-v4`/`ips-v6`; fail-soft (HTTP/sanity failure exits 0, last-good snapshot kept - never empty). Sanity: v4 `^[0-9].*/[0-9]+$`, v6 `^[0-9a-fA-F:].*/[0-9]+$`; atomic `mktemp`+`mv`, proxy reload, refresh-timestamp metric. A one-shot seed (the firewall list) lets the proxy start fresh.
- Firewall input rules: hardcoded in declarative host config, updated by hand; `CloudflareIpsStale` (`cloudflare_ips_refresh_timestamp_seconds` > 7d) catches drift.

systemd sandbox carve-outs - four nested hardening profiles (`baseSandbox` -> `commsHardening` -> `noPgSandbox` -> `noJitHardening`). The omissions are deliberate; do not tighten without reading why:

- `PrivateUsers` omitted from `baseSandbox`: child userns hides real UID from Postgres `SO_PEERCRED` peer auth; re-added (`noPgSandbox`) only for non-postgres services.
- `~@resources` unfiltered: carve-out for `mbind`/`set_mempolicy`/`sched_setattr`; `catalyrst-pulse` (Rust) doesn't need it - cleanup candidate.
- `RestrictFileSystems` off: needs the BPF LSM hook; deployed kernel lacks it, services exit 244 if set.
- `MemoryDenyWriteExecute` lands only in `noJitHardening` (LiveKit): the original carve-out was the Node archipelago workers' V8 JIT (W+X pages, else SIGTRAP on first JIT). Those workers are retired; the Rust `catalyrst-archipelago` and Pulse still run without it on `noPgSandbox` - cleanup candidates.
- No IP allowlist on sync/LiveKit/Pulse (rotating pool, arbitrary ICE/STUN peers, public UDP server respectively). Archipelago gets one (loopback+CDN) - its only external dep: one CDN-fronted gatekeeper host.
- No egress pinning on squid RPC providers: pinned IPs are brittle across provider/CDN changes.

## Admin console

SSR HTML on content core: `GET /`, `GET /admin`, `GET /admin/{service}`, gated `POST /admin/api/*` (`routes.rs`: content flush-cache, denylist add/remove, snapshot regeneration, sync pause/resume/force, read-only toggle, telemetry SQL, social user-ban, places moderation, POI CRUD, proxies to siblings).

- Default-safe: `ADMIN_ADDRESSES` or `SESSION_SECRET` unset -> read-only, every mutation 403, controls hidden.
- Never on the public edge: example nginx configs 404 `/admin`; reach it on loopback or private network, gated by the wallet allowlist. Read-only pages stay viewable unauthenticated; only controls + `POST /admin/api/*` need a session.
- `CATALYRST_SERVICE_URLS` (`key=baseurl`; keys `explore,create,social,data,ab-cdn,social-rpc,scene-state,profile-images,explorer-api,telemetry`) powers short-TTL `/health` dots on `/`+`/admin`; unset keys render "not configured", never "down".

Auth - one EIP-191 personal-sign over a SIWE-style message, then a stateless HMAC cookie:

```
GET  /admin/auth/nonce?address=0x...  -> { message }
POST /admin/auth/verify               -> sets cat_admin cookie ({message, signature})
POST /admin/auth/logout               -> clears cookie
GET  /admin/auth/me                   -> { address } | 401
```

`Nonce:` = `HMAC(SESSION_SECRET, host|address|exp)`, 5-minute expiry - no nonce store, not replayable cross-host/address. `verify` re-checks host, expiry, nonce HMAC, recovered signer in `ADMIN_ADDRESSES`, then mints `cat_admin` = `base64url({addr,exp}) . base64url(HMAC)` - `HttpOnly; SameSite=Strict; Secure`. Mutations require same-origin `Origin`/`Referer` when present.

Env vars - `ADMIN_ADDRESSES`, `SESSION_SECRET`, `ADMIN_SESSION_TTL_SECS`, `ADMIN_COOKIE_INSECURE`, `COMMS_MODERATOR_TOKEN`/`MODERATOR_TOKEN` (unset -> social controls hidden), `AB_REGISTRY_ADMIN_TOKEN`/`API_ADMIN_TOKEN` (unset -> create controls hidden), `DEBUGGING_SECRET` (unset -> scene controls hidden) - are defined in the [catalyrst-live env reference](../DEPLOYMENT.md#environment-variable-reference-catalyrst-live).

The console accepts its own env name or the sibling's native name. Unsupported proxy actions return 501, audited "unsupported". Telemetry `/dash/*` pages carry no token - loopback-trusted; must stay firewalled to loopback/private network.

## PostgreSQL

Postgres 18 required. Single node, peer auth over a Unix socket, no TCP (`listen_addresses = ""`), `unix_socket_permissions = 0770`, service users in `postgres` group. Principal DBs `content` (catalyrst) + `marketplace_squid` (squid); `POSTGRES_CONTENT_PASSWORD` exists because the binary requires it - auth is peer. Service crates own more DBs, migrated by each crate's sqlx migrations at `build_state()`: `communities`/`comms`/`notifications`/`badges`/`credits`/`ab_registry`/`places_events` (reader) + others. The database list, role provisioning and ownership/grant SQL are maintained in [nixos/postgresql.nix](../nixos/postgresql.nix); [bundle configuration](../nixos/bundles.nix) supplies each service's connection settings.

Tuning: `shared_buffers=3GB`, `effective_cache_size=8GB`, `work_mem=32MB`, `maintenance_work_mem=512MB`; `max_connections=300` + per-role `CONNECTION LIMIT` (120 catalyrst/60 squid, applied below); `random_page_cost=1.1`, `effective_io_concurrency=200` (SSD); `wal_level=minimal`, `max_wal_senders=0` - no replication, less WAL.

Boot-time least-privilege bootstrap (idempotent, every boot): strip superuser/createdb/createrole/replication from service roles, re-assert DB ownership + `REASSIGN OWNED`, grant-all + default privileges, cross-DB `SELECT` for catalyrst on `marketplace_squid.squid_marketplace`, `REVOKE CONNECT ... FROM PUBLIC`. Gotcha: `pg_upgrade`/`pg_restore` drop per-role `search_path`; a second oneshot re-applies `ALTER ROLE squid IN DATABASE marketplace_squid SET search_path = squid_marketplace, public;` every boot.

pgbouncer: catalyrst wants SESSION mode. sqlx caches prepared statements per connection; transaction mode may land each query on a different backend, so cache never warms (+1-3 ms/query). Put catalyrst in session mode per-user; leave the rest (squid etc., bursty/short-lived) on transaction:

```ini
[pgbouncer]
pool_mode = transaction
max_client_conn = 1000
default_pool_size = 25

[users]
catalyrst_user = pool_mode=session pool_size=150
```

Each catalyrst connection holds a backend full-time - size its pool >= content core's sqlx pools (`PG_POOL_SIZE` 50 + `SYNC_PG_POOL_SIZE` 40 + `SQUID_PG_POOL_SIZE` 10 defaults; role limit 120). Do NOT set `server_reset_query` (`DISCARD ALL`) for catalyrst - wipes prep cache. Verify with `SHOW POOLS;`: session mode -> `cl_active` = `sv_active` while open. Most prep-cache-sensitive: pointer-changes/audit/available-content (dynamic WHERE) - re-bench after pooling changes.

pgaudit isn't in nixpkgs `postgresql_18` - needs `withPackages`.

## Multiplayer v4 rollout

The NixOS module turns v4 on wherever `subServices.comms` is on
(`services.catalyrst.comms.v4.enable`, default `true`). It serves v4 beside v3:
the legacy `/ws` endpoint and `COMMS_PROTOCOL=v3` stay, and a client that never
reads `comms.v4` sees no change. `comms.v4.enable = false` yields exactly the
pre-v4 units. The binaries themselves stay opt-in and fail closed: outside the
module nothing is armed until the variables below are set. A shared database
does not make LiveKit's wallet-only removal API conditional on the old session.

| Module option | Default | Sets |
|---|---|---|
| `comms.v4.audience` | `domain` | `ARCHIPELAGO_CONTROL_V4_AUDIENCE`, `COMMS_CONTROL_V4_AUDIENCE`, `PULSE_V4_AUDIENCE`, `COMMS_V4_CONTROL_AUDIENCE`, `COMMS_V4_PULSE_AUDIENCE` |
| `comms.v4.pulseIssuer` | `pulse-server.<domain>`, or `<domain>` with `exposure = "lan"` | `PULSE_V4_ISSUER` |
| `comms.v4.pulseNativeEndpoint` | `pulse-server.<domain>:<pulse.port>`, or `<domain>:<pulse.port>` with `exposure = "lan"` | `COMMS_V4_PULSE_NATIVE_ENDPOINT` |
| `comms.v4.pulseWebTransportUrl` | unset | `COMMS_V4_PULSE_WEBTRANSPORT_URL`, only when non-empty |
| fixed | `<wsScheme>://<domain>/ws/v4` | `COMMS_V4_CONTROL_URL` |

Both pairs are always written together, so a half-set pair cannot be deployed.
The authority database is `comms_control`. Archipelago owns it as the
`archipelago` role and creates the tables at startup; it runs as the static
`archipelago` user in the `postgres` group, without `PrivateUsers`, because peer
authentication needs a real user on the socket. Comms Gatekeeper reads and
fences as `catalyrst` with `SELECT, UPDATE` only, granted by
`postgresql-comms-control.service`. `catalyrst-comms-control-ready.service` holds
the social bundle until those tables exist and the `catalyrst` role can use
them: a bundle member that fails at startup stays down for the life of the
process, so the bundle never starts ahead of its authority. With
`tls = "none"` on a routable name the server drops the `ws://` control URL and
advertises Pulse only. The module runs no WebTransport listener, so browsers are
offered no v4 Pulse endpoint. A client started with an explicit legacy Pulse
server keeps the legacy handshake.

Pulse enables challenge authentication with `PULSE_V4_ENABLED=true`, an explicit
`PULSE_V4_AUDIENCE` shared by that deployment's clients, and a per-replica
`PULSE_V4_ISSUER`. Challenges are bound to the issuing process and transport
connection; a restart or another replica rejects the captured authentication.
Identical committed requests can be retried on that same live connection.
Defaults are a 15-second challenge lifetime, 8,192 pending challenges, a
4,096-byte frame limit and a 3,072-byte authentication-chain limit. The legacy
handshake and its replay journal remain necessary while legacy clients exist.
Its signed payload names no server, so a captured one replays on another replica
or another deployment inside its 60-second freshness window, and no journal one
process keeps can see that. `PULSE_LEGACY_HANDSHAKE=false` refuses it, with
`AuthFailed` and nothing consumed, once every client of the deployment speaks v4;
Pulse refuses to start with it while `PULSE_V4_ENABLED` is off.

Archipelago's `/auth/livekit-token` mints at most 60-second tokens:
`LIVEKIT_TOKEN_TTL_SECS` is capped there, the bound the comms takeover quarantine
waits out. While the control authority is armed it mints only for the session
that holds the wallet's realm lane, answers 403 to another session of that wallet
and 503 when the authority cannot be read. A lane whose owner stopped renewing
for 90 seconds, or released it on close, fences nobody.

Archipelago's separate `/ws/v4` endpoint uses the `archipelago-v4` WebSocket
subprotocol and binary protobuf frames. V4 authentication chains are typed
protobuf messages; neither control frames nor their authentication payloads
contain embedded JSON. Ownership requires both `ARCHIPELAGO_CONTROL_V4_AUDIENCE` and
`ARCHIPELAGO_CONTROL_PG_CONNECTION_STRING`. All replicas serving one audience
must use the same authority database. Tables are created at startup; the role
needs DDL permission for initialization. Deployment audience, wallet and lane
identify independent ownership rows. Epochs are allocated by PostgreSQL;
replacing an owner clears the previous assignment and advances its fence.
Assignment updates advance their revision, not the owner's fence.

Realm discovery is additive under `/about` -> `comms.v4`; the existing adapter
fields are unchanged. Configure only the endpoints that have been enabled:

| Public metadata | Content-server environment |
|---|---|
| `control.url`, `control.audience` | `COMMS_V4_CONTROL_URL`, `COMMS_V4_CONTROL_AUDIENCE` |
| `pulse.nativeEndpoint` | `COMMS_V4_PULSE_NATIVE_ENDPOINT` (`host:port`) |
| `pulse.webTransportUrl` | `COMMS_V4_PULSE_WEBTRANSPORT_URL` (`https://host:port`) |
| `pulse.audience` | `COMMS_V4_PULSE_AUDIENCE` |

Advertised audiences must exactly match the corresponding endpoint's configured
audience. Incomplete endpoints are omitted, and offline realms omit v4 discovery.
Public URLs must not contain user information, query credentials or fragments.
Discovery does not enable a service, migrate clients or configure a proxy route.
Clients configured to require v4 must fail visibly when it is unavailable, not
retry a legacy authentication path.

## Archipelago broker recovery

Archipelago bounds its immediate announcement queue to 1,024 items, each with at
most 1,024 combined subject/payload bytes. `/stats/health` counts discarded
announcements, including overload and obsolete work. Accepted by the queue does
not mean delivered to the consumer.

The connector uses a private `_INBOX.*` subject to verify broker progress with a
two-second round-trip deadline. Its NATS account must allow publishing and
subscribing to these generated inbox subjects as well as the existing peer/feed
subjects. SDK `flush()` alone only confirms a local write. Broker disconnects,
slow-consumer events and failed probes invalidate readiness.

Every five seconds the connector schedules retries for live sockets still
awaiting their first island assignment. A full sweep, including already-assigned
sockets, becomes due every 30 seconds and immediately after broker restoration:
otherwise a lost later move could strand a client in its old island. Each sweep
finishes before another snapshot replaces it, so the tail cannot be starved by
timer ticks. Large or slow sweeps can take longer than those intervals.

Sweeps bypass the bounded outbox, recheck socket incarnation before sending and
wait at least 10 ms after each confirmed announcement (at most 100 sweep sends
per second per connector). Immediate connect/disconnect announcements retain
their separate bounded queue. A successful WebSocket assignment write stops only
the fast initial retry; it does not establish LiveKit membership. Comms resolves
announcements through the Pulse lookup below and suppresses a matching healthy
occupant. No broker URL still means no feed.

## Archipelago socket backpressure

Each socket retains at most one pending encoded island assignment, capped at
64 KiB, in addition to the writer's current frame. New assignments replace the
pending one: these are absolute room/adapter targets, not incremental presence
deltas. Oversized assignments are rejected before decoding or retaining a copy.
This coalesces arrival order; it does not prove that a late broker message is the
newest Pulse revision. Periodic reconciliation remains necessary, and distributed
stale-assignment protection is still open.

A kick atomically retires the socket, discards its pending assignment and wakes
the writer. Later assignments are rejected. If no write is in progress, a bounded
best-effort kicked packet is sent; a kick during a stalled write aborts the
transport without attempting another frame on a partially written stream.
Every application-controlled write has a five-second deadline, including welcome,
assignment, pong and close packets. Failure retires the registration instead of
retrying an uncertain frame. Incoming WebSocket frames/messages are capped at
64 KiB; the protocol write buffer is capped at 65 KiB. These are per-socket
limits, not a global connection-admission or process-memory bound.

`/stats/health` separates the following counters:

- `feed_delivered`: legacy name for assignment acceptance into a socket mailbox;
  includes replacements and is not a wire receipt.
- `feed_assignment_coalesced`: pending assignments replaced by a later arrival.
- `feed_assignment_oversized`: assignments rejected by the 64-KiB limit.
- `socket_assignments_written`: completed application WebSocket assignment writes,
  not client acknowledgement or LiveKit membership.
- `socket_write_timeout`: application writes that exhausted their deadline.

## Comms broker exchanges

Comms has one broker supervisor and a bounded outbox of 128 queued publishes/queries
(plus one active exchange), each capped at 64 KiB of combined subject/payload
bytes. Excess work is rejected, not accumulated. The two-second exchange budget
includes queue time. Expired/cancelled queued work cannot submit. An in-flight
publish timeout has unknown delivery and forces link reconstruction.

Read-only queries use one private reply subscription per link, unique correlation
subjects and at most 128 pending replies. Replies have a 64-KiB limit, and late,
duplicate or previous-link/revision replies cannot satisfy a new request. Query
timeouts reclaim their slot without declaring a healthy broker broken: a missing
source owner deliberately does not reply. Pending queries do not block publishes.

Subscriptions and readiness are checked by a private `_INBOX.*` round trip at
startup, after registration/link changes, and every five seconds while idle.
The comms NATS account therefore also needs publish/subscribe permission for its
generated inbox subjects. `comms_nats_connected` reflects this round-trip-verified
readiness, not just a TCP socket. Rebuilding a failed connection waits five seconds.
Broker URLs and opaque connection errors are not logged by this supervisor.

`Submitted` means the publish preceded a successful broker round trip; it does
not acknowledge a subscriber, permission to publish the target subject, receipt
at a client, or room admission. `Unconfirmed` means delivery is unknown. Such
exchanges do not update the previous-room hint and increment both
`comms_cluster_publish_failed_total` and its uncertain-delivery subset,
`comms_cluster_publish_unconfirmed_total`. Already submitted bytes may still
arrive late; current-state reconciliation and revision checks remain necessary.
Comms wallet work and Archipelago's downstream socket mailbox have separate
bounds; neither broker acceptance nor a completed socket write establishes a join.

## Comms wallet work and shutdown

The subscriber admits at most 1,024 waiting/running items total and 16 per wallet,
with at most 32 executing at once. Accepted work is FIFO for each wallet; ready
wallets take round-robin turns. New overflow is rejected. Cluster-change work can
remove an SFU participant, so it is not coalesced as if it were merely a replaceable
snapshot. Repeated recovery announcements still coalesce per wallet/session.
Every admitted item has a 150-second budget including queue time; an expired item
is discarded before execution. Feed payloads over 64 KiB, subjects over 256 bytes,
and invalid wallet addresses are rejected before copying/decoding into retained work.

Each start owns a distinct work incarnation. Stop removes its subscriptions,
rejects captured callbacks from that incarnation and cancels recovery attempts.
It allows ordinary work to finish for `CLUSTER_DRAIN_TIMEOUT_MS` (default 5,000),
then drops remaining queued work, cancels running futures and waits for their
local cleanup. A zero budget cancels immediately. A concurrent restart is not
cancelled or drained by the old stop. Grace/deadline bounds assume the async
runtime keeps scheduling; cancellation cannot undo already-submitted NATS bytes
or an SFU removal request.

Monitor these `/metrics` series:

- `comms_cluster_work_rejected_total`: `reason` is `wallet_full`, `global_full`
  or `retired`, with separate series for each admission failure.
- `comms_cluster_work_running` / `comms_cluster_work_queued`: current local depths.
- `comms_cluster_work_expired_total` / `comms_cluster_work_cancelled_total`:
  deadline versus lifecycle retirement.
- `comms_cluster_feed_oversized_total` / `comms_cluster_feed_invalid_subject_total`:
  input rejected before decoding.

Periodic current-source reconciliation retries an assignment lost to overload
after capacity returns. This does not replay every rejected event's side effects,
establish a source-ownership lease, or resolve cross-room/cross-replica takeovers.
These work bounds also do not bound total admitted clients, all SDK buffers or
whole-process memory.

## Pulse current-assignment lookup

Pulse now serves read-only requests on `peer.{wallet}.cluster_lookup`. The body
is the exact 42-byte session address, and the reply subject must be a bounded
`_INBOX.*` subject. Its broker account needs subscribe permission for
`peer.*.cluster_lookup` and publish permission for reply inboxes. The response is
`decentraland.pulse.PeerClusterSnapshot`: wallet, session, post-debounce cluster,
realm, pass number and an opaque source incarnation.

Every completed cluster pass replaces the entire live view. It excludes departed
peers even while their takeover history remains retained. It expires after two
configured pass intervals (at least one second), using a monotonic clock.
Unknown, mismatched or expired owners do not reply; readers must use a bounded
request deadline and must not interpret silence as authoritative absence across
all Pulse instances. The ungrouped lookup subscription allows the matching owner
to answer without a non-owner racing it with an empty reply. It does not resolve
two Pulse instances concurrently claiming the same wallet/session.

The source incarnation survives broker reconnect but changes when the feed owner
is recreated. Pass numbers order observations only within that incarnation, not
across source restarts or different servers. Replies describe a completed pass,
not a lease guaranteeing that the owner cannot change immediately afterward.
The SDK query subscription is bounded to 128 messages and reply submission has a
two-second deadline and a 64-KiB size cap. `pulse_cluster_lookups_total` counts
well-formed queries and `pulse_cluster_lookup_submitted_total` counts reply
submissions, not consumer receipts.

Comms now queries this source on `peer.*.connect`, including with an empty or
expired mirror. Its broker account needs publish permission on the lookup subject
and subscribe permission on its private inbox wildcard. It accepts only an exact
wallet/session reply; malformed or legacy connect payloads and failed queries
cannot fall back to cached ownership. Deploy the lookup-capable Pulse before the
updated comms; older Pulse versions cannot provide reconnect recovery.

Recovery checks LiveKit occupancy before and after minting, then queries Pulse
again. A changed owner, cluster, realm or source incarnation, or a lower pass
number, discards the token. Changed local assignment events and subscriber stop
cancel all affected recovery attempts. Announcements coalesce per wallet/session;
another device's announcement does not prove it owns Pulse and cannot cancel the
current owner's recovery. A changed Pulse owner is still rejected by the final
lookup before publication. Each running recovery has a ten-second budget; expiry
leaves the socket eligible for the connector's next sweep.
`comms_cluster_recovery_stale_total` counts discarded/cancelled candidates.

New island tokens carry `metadata.catalyrstIsland` with version, wallet and
delegated session. Island grants disable `canUpdateOwnMetadata` (including name
and attribute self-updates); scene/voice grants are unchanged. GetParticipant must
return the exact wallet, a participant SID, locked metadata permissions and a
valid owner record before comms attributes an occupant. Missing/legacy/writable
metadata fails closed. A held matching session suppresses recovery; an attributed
different session permits the current Pulse owner to receive a token. LiveKit's
duplicate-identity admission replaces the old same-room session. This recovery
path never calls RemoveParticipant, so there is no read-then-remove race against
a newcomer. It rechecks occupancy and source ownership after minting.

The composed regression now runs the actual tracker -> NATS -> comms -> LiveKit
signalling path, loses the initial change event, and checks replacement and
same-owner suppression. Pulse boards are populated directly and a minimal signal
client handles join/leave: this does not establish signed Pulse admission,
Archipelago delivery, ICE/data/media readiness or native/browser client behavior.
Snapshot checks are not an atomic lease. Stale/cross-replica assignment ordering,
competing Pulse authorities and cross-room cleanup remain open. Same-room old-token
reuse is contained by the quarantine below.

### Self-hosted token quarantine

`RemoveParticipant` on self-hosted LiveKit removes the current participant but
does **not** invalidate its existing token, even with an explicit `revokeTokenTs`.
That feature is [Cloud-only](https://docs.livekit.io/frontends/reference/tokens-grants/#token-revocation).
The protocol revision pinned by LiveKit 1.13.7 also accepts an expired JWT for
[60 seconds of verifier leeway](https://github.com/livekit/protocol/blob/a4f4b5c0c23f167d0c07de1e1d7652125882e73c/auth/verifier.go#L23-L24),
and the server admission path calls that verifier
([LiveKit 1.13.7 source](https://github.com/livekit/livekit/blob/v1.13.7/pkg/service/auth.go#L72-L100)).

`CLUSTER_SELF_HOSTED_TOKEN_QUARANTINE=true` is therefore the default. For a
same-room owner replacement, comms first requires the event to match Pulse's
exact current wallet/session, cluster and realm; a stale event has no removal
side effect. It then removes the old identity, waits token TTL + 60 seconds + a
two-second whole-second/scheduler margin, removes the identity again, revalidates
the same source lineage, mints, revalidates again and only then publishes the
replacement. Token TTL is capped at 60 seconds, making the default quarantine
122 seconds and keeping it within the bounded 150-second wallet-work deadline.
The checks narrow stale-event races but do not provide an atomic lease.
The LiveKit 1.13.7 regression executes that production sequence and then proves
that the original token is refused while the new owner remains.

A newer accepted takeover for the same wallet and room wakes older quarantines,
but does not automatically cancel them: each woken attempt queries Pulse for the
named successor and yields only when that exact session, cluster and realm is
current. Supersession propagates through the bounded per-wallet queue, so a burst
does not spend 122 seconds on every obsolete owner and expire the latest event.
Rejected work, a stale successor and an event for another room cannot cancel the
active quarantine. The real-server regression includes a rapid B-to-C succession
and publishes only C after one complete expiry boundary.

Set the switch to `false` only for LiveKit Cloud deployments where cutoff
revocation is available. If an existing deployment previously issued island
tokens longer than 60 seconds, drain takeovers for the old maximum TTL plus the
60-second verifier leeway before relying on the cap. Cross-room moves do not wait:
the old JWT is room-bound and cannot displace the owner in the new room. It can
still recreate a stale participant in the old room, and removal remains
wallet-targeted rather than conditional on an expected participant SID. Those
cross-room cleanup and distributed ownership races remain open.

## LiveKit

- Dev creds: `catalyrst-comms`/`catalyrst-worlds` FAIL FAST at boot with `LIVEKIT_API_KEY`/`LIVEKIT_API_SECRET` unset, unless `LIVEKIT_ALLOW_DEV_CREDS=1` opts into `devkey`/`devsecret`. `catalyrst-archipelago` boots on them with a warning - JWTs parse locally but a real SFU rejects them; `livekit_configured=false` shows only in `/status`. Set key/secret/host across comms/worlds/archipelago - one SFU (social+explore env files).
- `/rtc` 502/roster-but-no-avatars: media dead while signaling healthy - see the UDP gotcha under Networking.
- Twirp admin API shares LiveKit's signaling listener. The SFU vhost blocks it at the reverse proxy; direct access to TCP 7880 depends on the host firewall (see Networking).

Quarterly rotation (`livekit-rotate.service`, defined in
[nixos/comms.nix](../nixos/comms.nix)) - timer `*-01,04,07,10-01 03:00:00`,
`RandomizedDelaySec=1h`, `Persistent=true`:

1. Snapshot `livekit.yaml` + `livekit-api.env` to `.prev`.
2. Generate `KEY=API<12-hex>`, `SECRET=base64(36 bytes)`.
3. Write both atomically (`mktemp`+`mv`, 0600, root).
4. Restart `livekit.service`; sleep 5.
5. If the SFU restart returned successfully but the SFU is inactive after the wait: restore both `.prev` files, restart `livekit.service` and `catalyrst-archipelago.service`, exit 1. The other consumers have not been restarted with the new credentials at this point and retain the old pair.
6. On success restart every enabled credential consumer in `livekitHolders`: `catalyrst-archipelago.service`, `catalyrst-explore.service` (worlds) when `subServices.explore` is enabled, and `catalyrst-social.service` (comms) when `subServices.social` is enabled. With `comms.v4.applicationRelay`, v4 and the social bundle enabled, also restart `pulse-relay-secret.service` to regenerate the relay credential file, then `pulse.service` (or `podman-pulse.service` when `pulse.sandbox = true`). systemd ordering places credential regeneration before Pulse and the social bundle.
7. Publish `livekit_rotation_timestamp_seconds` via the node-exporter textfile dir - `LiveKitKeyStale` (>100d) catches a stuck timer.

To rotate manually using this procedure, run
`sudo systemctl start livekit-rotate.service` and inspect
`journalctl -u livekit-rotate.service`. A failed restart command aborts the script
immediately; rollback only runs for the inactive-SFU check in step 5. Failures
while restarting consumers also require manual recovery.

For recovery or rotation outside the unit, install a matching `livekit.yaml`
and `livekit-api.env` pair (restore both `.prev` files if rolling back), restart
`livekit.service` and confirm it is active, then restart **every enabled
consumer listed in step 6** against that same pair. When relay is enabled,
restart `pulse-relay-secret.service` before restarting Pulse and social. This
also applies after a partial consumer restart: refreshing only Archipelago can
leave worlds, comms or Pulse using a different key from the SFU.

## Observability

All exporters and Prometheus bind loopback only; tunnel to explore (`ssh -L 9090:127.0.0.1:9090 <host>`).

Scrape targets: `node` `:9100` (node_exporter, `systemd`+`textfile` collectors; textfile dir holds LiveKit-rotation/CDN-IP-refresh metrics); `catalyrst` `:5141/metrics` (content core); `archipelago` `:5139` (`catalyrst-archipelago` - clustering, ws-connector, stats in one Rust binary; comms profile only); `pulse` `:5005/metrics` (`PULSE_METRICS_BIND`; comms profile only); `blackbox_about` - probe of `https://<host>/content/about` via blackbox_exporter `:9115`, module `about_comms_healthy` (200 + body matching `"comms":{"healthy":true`).

`pulse` is inside the `ServiceDown` regex, so its exporter is load-bearing for that alert's credibility, not just for pulse: its unit allows exactly `udp:7777` + `tcp:5005` and pulse refuses to start when it cannot bind the latter. Move `PULSE_METRICS_BIND`, `SocketBindAllow` and the scrape target together - a mismatch pins `up{job="pulse"}` at 0, `ServiceDown` fires forever, and the operator learns to ignore it for `catalyrst` and `node` as well.

| Alert | Expression | For | Severity |
|---|---|---|---|
| AboutDownOrCommsUnhealthy | `probe_success{job="blackbox_about"} == 0` | 3m | critical |
| CertExpiringSoon | `probe_ssl_earliest_cert_expiry - time() < 14d` | 1h | warning |
| ServiceDown | `up{job=~"catalyrst\|archipelago\|pulse\|node"} == 0` | 3m | critical |
| LiveKitKeyStale | rotation timestamp older than 100d | 1h | warning |
| CloudflareIpsStale | refresh timestamp older than 7d | 1h | warning |
| DiskAlmostFull / DiskCritical | rootfs avail < 10% / < 5% | 15m / 5m | warning / critical |
| SyncHeartbeatStale | `time() - catalyrst_sync_heartbeat_timestamp_seconds > 900` | 5m | critical |
| SyncIngestSilent | `increase(catalyrst_sync_deployments_total[2h]) == 0` | 30m | warning |

Sync-liveness on `:5141/metrics`: `catalyrst_sync_heartbeat_timestamp_seconds` beats <=10s per fetched pointer-changes page (liveness signal; `SyncHeartbeatStale` = loop dead); `catalyrst_sync_frontier_timestamp_seconds` = persisted frontier (coarse, advances at phase ends, don't alert on it); `catalyrst_sync_deployments_total` counts ingest (`SyncIngestSilent` = loop beats, nothing lands). Gauges exist only on sync-enabled nodes post-first-beat; nodes with sync disabled never page, `SyncIngestSilent` can't fire until the first post-restart increment. Sync settings are listed in the [environment reference](../DEPLOYMENT.md#environment-variable-reference-catalyrst-live); the [self-host guide](self-host.md#will-it-fit-on-your-box) explains the NixOS module's different sync default.
