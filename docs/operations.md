# Operations - networking, admin console, postgres, LiveKit, observability

## Networking, firewall, sandboxing

Reference: `nixos/configuration.nix`. Everything rides the TLS reverse proxy except UDP media/game traffic (proxies can't forward UDP).

| Port | Proto | Service | Notes |
|---|---|---|---|
| 22 | TCP | sshd | key-only + brute-force protection |
| 80/443 | TCP | reverse proxy | accept only from the CDN's published v4/v6 ranges |
| 7881 | TCP | LiveKit RTC | TCP fallback when UDP fails |
| 7777 | UDP | Pulse (ENet) | authoritative game server |
| 7882 | UDP | LiveKit media | SFU media |

Restrict 80/443 to CDN ranges in firewall AND feed the same list to the proxy's `real_ip` config.

**"Peers in roster but no remote avatars"** (also `/rtc` 502): inbound UDP to the SFU dropped, DTLS times out while signaling stays healthy. Fix: open/forward SFU UDP range, or set LiveKit `node_ip` to a reachable, non-NATed address, then restart the SFU; STUN to a blocking host fails silently.

**Cloudflare/CDN IP refresh - two decoupled sources of truth:**

- Proxy `real_ip` include: refreshed daily from `https://www.cloudflare.com/ips-v4`/`ips-v6`; fail-soft (HTTP/sanity failure exits 0, last-good snapshot kept - never empty). Sanity: v4 `^[0-9].*/[0-9]+$`, v6 `^[0-9a-fA-F:].*/[0-9]+$`; atomic `mktemp`+`mv`, proxy reload, refresh-timestamp metric. A one-shot seed (the firewall list) lets the proxy start fresh.
- Firewall input rules: hardcoded in declarative host config, updated by hand; `CloudflareIpsStale` (`cloudflare_ips_refresh_timestamp_seconds` > 7d) catches drift.

**systemd sandbox carve-outs** - four nested hardening profiles (`baseSandbox` -> `commsHardening` -> `noPgSandbox` -> `noJitHardening`); deliberate omissions, don't tighten without reading why:

- `PrivateUsers` omitted from `baseSandbox`: child userns hides real UID from Postgres `SO_PEERCRED` peer auth; re-added (`noPgSandbox`) only for non-postgres services.
- `~@resources` unfiltered: carve-out for `mbind`/`set_mempolicy`/`sched_setattr`; `catalyrst-pulse` (Rust) doesn't need it - cleanup candidate.
- `RestrictFileSystems` off: needs the BPF LSM hook; deployed kernel lacks it, services exit 244 if set.
- `MemoryDenyWriteExecute` excludes archipelago workers: V8 JITs need W+X pages, else SIGTRAP on first JIT. Pulse's exclusion is unneeded (Rust) - cleanup candidate.
- No IP allowlist on sync/LiveKit/Pulse (rotating pool, arbitrary ICE/STUN peers, public UDP server respectively). Archipelago gets one (loopback+CDN) - its only external dep: one CDN-fronted gatekeeper host.
- No egress pinning on squid RPC providers: pinned IPs are brittle across provider/CDN changes.

## Admin console

SSR HTML on content core: `GET /`, `GET /admin`, `GET /admin/{service}`, gated `POST /admin/api/*` (`routes.rs`: content flush-cache, denylist add/remove, snapshot regeneration, sync pause/resume/force, read-only toggle, telemetry SQL, social user-ban, places moderation, POI CRUD, proxies to siblings).

- Default-safe: `ADMIN_ADDRESSES` or `SESSION_SECRET` unset -> read-only, every mutation 403, controls hidden.
- Never on the public edge: example nginx configs 404 `/admin`; reach it on loopback or private network, gated by the wallet allowlist. Read-only pages stay viewable unauthenticated; only controls + `POST /admin/api/*` need a session.
- `CATALYRST_SERVICE_URLS` (`key=baseurl`; keys `explore,create,social,data,ab-cdn,social-rpc,scene-state,profile-images,explorer-api,telemetry`) powers short-TTL `/health` dots on `/`+`/admin`; unset keys render "not configured", never "down".

**Auth** - one EIP-191 personal-sign over a SIWE-style message, then a stateless HMAC cookie:

```
GET  /admin/auth/nonce?address=0x...  -> { message }
POST /admin/auth/verify               -> sets cat_admin cookie ({message, signature})
POST /admin/auth/logout               -> clears cookie
GET  /admin/auth/me                   -> { address } | 401
```

`Nonce:` = `HMAC(SESSION_SECRET, host|address|exp)`, 5-minute expiry - no nonce store, not replayable cross-host/address. `verify` re-checks host, expiry, nonce HMAC, recovered signer in `ADMIN_ADDRESSES`, then mints `cat_admin` = `base64url({addr,exp}) . base64url(HMAC)` - `HttpOnly; SameSite=Strict; Secure`. Mutations require same-origin `Origin`/`Referer` when present.

| Env | Meaning | Default |
|---|---|---|
| `ADMIN_ADDRESSES` | comma-separated `0x...` allowlist | unset -> read-only |
| `SESSION_SECRET` | HMAC key for cookie + nonce | unset -> read-only |
| `ADMIN_SESSION_TTL_SECS` | session lifetime | 43200 (12h) |
| `ADMIN_COOKIE_INSECURE` | `1` drops the `Secure` flag (plain-HTTP private nets only; localhost is already a secure context) | unset |
| `COMMS_MODERATOR_TOKEN` / `MODERATOR_TOKEN` | bearer forwarded to comms for ban/warn/unban | unset -> social controls hidden |
| `AB_REGISTRY_ADMIN_TOKEN` / `API_ADMIN_TOKEN` | bearer forwarded to the registry for re-ingest / cache flush | unset -> create controls hidden |
| `DEBUGGING_SECRET` | secret injected into scene-state reload | unset -> scene controls hidden |

The console accepts its own env name or the sibling's native name. Unsupported proxy actions return 501, audited "unsupported". Telemetry `/dash/*` pages carry no token - loopback-trusted; must stay firewalled to loopback/private network.

## PostgreSQL

Postgres **18** required. Single node, peer auth over a Unix socket, no TCP (`listen_addresses = ""`), `unix_socket_permissions = 0770`, service users in `postgres` group. Principal DBs `content` (catalyrst) + `marketplace_squid` (squid); `POSTGRES_CONTENT_PASSWORD` exists because the binary requires it - auth is peer. Service crates own more DBs, migrated by each crate's sqlx migrations at `build_state()`: `communities`/`comms_gatekeeper`/`notifications`/`badges`/`credits`/`ab_registry`/`places_events` (reader) + others - role recipe in [deploy.md](./deploy.md).

Tuning: `shared_buffers=3GB`, `effective_cache_size=8GB`, `work_mem=32MB`, `maintenance_work_mem=512MB`; `max_connections=300` + per-role `CONNECTION LIMIT` (120 catalyrst/60 squid, applied below); `random_page_cost=1.1`, `effective_io_concurrency=200` (SSD); `wal_level=minimal`, `max_wal_senders=0` - no replication, less WAL.

**Boot-time least-privilege bootstrap** (idempotent, every boot): strip superuser/createdb/createrole/replication from service roles, re-assert DB ownership + `REASSIGN OWNED`, grant-all + default privileges, cross-DB `SELECT` for catalyrst on `marketplace_squid.squid_marketplace`, `REVOKE CONNECT ... FROM PUBLIC`. Gotcha: `pg_upgrade`/`pg_restore` drop per-role `search_path`; a second oneshot re-applies `ALTER ROLE squid IN DATABASE marketplace_squid SET search_path = squid_marketplace, public;` every boot.

**pgbouncer: catalyrst wants SESSION mode.** sqlx caches prepared statements per connection; transaction mode may land each query on a different backend, so cache never warms (+1-3 ms/query). Put catalyrst in session mode per-user; leave the rest (squid etc., bursty/short-lived) on transaction:

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

## LiveKit

- Dev creds: `catalyrst-comms`/`catalyrst-worlds` FAIL FAST at boot with `LIVEKIT_API_KEY`/`LIVEKIT_API_SECRET` unset, unless `LIVEKIT_ALLOW_DEV_CREDS=1` opts into `devkey`/`devsecret`. `catalyrst-archipelago` boots on them with a warning - JWTs parse locally but a real SFU rejects them; `livekit_configured=false` shows only in `/status`. Set key/secret/host across comms/worlds/archipelago - one SFU (social+explore env files).
- `/rtc` 502/roster-but-no-avatars: media dead while signaling healthy - see the UDP gotcha under Networking.
- Twirp admin API shares the `/rtc` port; the edge 404s `/` on the SFU vhost so it never reaches the internet.

**Quarterly rotation (`livekit-rotate.service`)** - timer `*-01,04,07,10-01 03:00:00`, `RandomizedDelaySec=1h`, `Persistent=true`:

1. Snapshot `livekit.yaml` + `livekit-api.env` to `.prev`.
2. Generate `KEY=API<12-hex>`, `SECRET=base64(36 bytes)`.
3. Write both atomically (`mktemp`+`mv`, 0600, root).
4. Restart `livekit.service`; sleep 5.
5. If SFU isn't active: restore `.prev`, restart SFU AND `archipelago-core` (mints tokens against whichever key won), exit 1.
6. On success restart `archipelago-core`.
7. Publish `livekit_rotation_timestamp_seconds` via the node-exporter textfile dir - `LiveKitKeyStale` (>100d) catches a stuck timer.

Rotating by hand: replicate step 5's pairing - SFU and every token-minting service must agree on the key or comms dies quietly.

## Observability

All exporters and Prometheus bind loopback only; tunnel to explore (`ssh -L 9090:127.0.0.1:9090 <host>`).

Scrape targets: `node` `:9100` (node_exporter, `systemd`+`textfile` collectors; textfile dir holds LiveKit-rotation/CDN-IP-refresh metrics); `catalyrst` `:5141/metrics` (content core); `archipelago` `:5000/:5001/:5002` (core, ws-connector, stats); `blackbox_about` - probe of `https://<host>/content/about` via blackbox_exporter `:9115`, module `about_comms_healthy` (200 + body matching `"comms":{"healthy":true`).

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

Sync-liveness on `:5141/metrics`: `catalyrst_sync_heartbeat_timestamp_seconds` beats <=10s per fetched pointer-changes page (liveness signal; `SyncHeartbeatStale` = loop dead); `catalyrst_sync_frontier_timestamp_seconds` = persisted frontier (coarse, advances at phase ends, don't alert on it); `catalyrst_sync_deployments_total` counts ingest (`SyncIngestSilent` = loop beats, nothing lands). Gauges exist only on sync-enabled nodes post-first-beat; read-only nodes never page, `SyncIngestSilent` can't fire until the first post-restart increment. Sync keys: [content-sync.md](./content-sync.md).
