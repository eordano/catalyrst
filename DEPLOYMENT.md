# catalyrst deployment & cutover runbook

`catalyrst-live` (Rust port of the catalyst content server) runs in one of three modes, each
strictly more privileged than the last; all write/sync behavior is opt-in and fail-closed by
default. Requires PostgreSQL 18 (the example NixOS configs pin `pkgs.postgresql_18`; the
bootstrap `postgresql-ownership.service` uses 18-only client binaries).

| Mode | Reads | Syncs from upstream | Accepts `POST /entities` |
|---|---|---|---|
| read-only (default) | yes | no (`SYNC_ENABLED=false`) | no (`ReadOnlyDeployer`) |
| sync replica | yes | yes (`SYNC_ENABLED=true`) | no |
| write node | yes | optional | yes (`ENABLE_DEPLOYMENTS=true`) |

Build the release binary first (FHS shell is required on this NixOS host):

```bash
cargo build --release --bin catalyrst-live
# -> target/release/catalyrst-live
```

## 1. Read-only staging (parity test against the live server)

Run `catalyrst-live` read-only on :5141 alongside a reference content server on :5140, both on
the same content DB + storage; catalyrst cannot write or sync without explicit env flags.

```bash
# start catalyrst read-only, foreground (Ctrl-C to stop)
HTTP_SERVER_HOST=127.0.0.1 \
HTTP_SERVER_PORT=5141 \
POSTGRES_CONTENT_USER=<user> \
POSTGRES_CONTENT_PASSWORD=<pw> \
POSTGRES_CONTENT_DB=content \
STORAGE_ROOT_FOLDER=<DATA_DIR>/content \
  ./target/release/catalyrst-live
# -> "catalyrst-live listening 127.0.0.1:5141"

# parity-check the two servers return the same data
curl -s localhost:5140/about  | jq .healthy
curl -s localhost:5141/about  | jq .healthy
curl -s -XPOST localhost:5140/entities/active -H 'content-type: application/json' -d '{"pointers":["0,0"]}' | jq -S . > /tmp/a.json
curl -s -XPOST localhost:5141/entities/active -H 'content-type: application/json' -d '{"pointers":["0,0"]}' | jq -S . > /tmp/b.json
diff /tmp/a.json /tmp/b.json && echo "PARITY OK"
```

For long-running deployments write your own systemd unit (or use the example NixOS module in
[`nixos/`](./nixos)) with `EnvironmentFile=` supplying the section-3 env vars. To cut over:
repoint nginx (or the `:5140` binding) at catalyrst and stop the reference server, after
parity is confirmed over time running side-by-side.

## 2. Index third-party registry roots locally (removes external dependency)

By default third-party deployments verify Merkle roots against the external Decentraland
registry subgraph. Recommended pure-Rust path: catalyrst-live's built-in background task
bootstraps + refreshes `squid_marketplace.third_party*` from the registry subgraph (no Node
squid). On the write-node (or sync-replica) host:

```bash
THIRD_PARTY_REFRESH_HOURS=24        # >0 enables the refresher (period in hours)
THIRD_PARTY_ROOT_SOURCE=squid       # read roots from the local index
```

First run creates the tables (if missing) and seeds them; then refreshes every
`THIRD_PARTY_REFRESH_HOURS` hours. Override subgraph URLs via
`THIRD_PARTY_REGISTRY_L2_SUBGRAPH_URL` / `BLOCKS_L2_SUBGRAPH_URL`. Verify rows landed:

```bash
psql "$SQUID_DB" -c "select count(*), count(root) from squid_marketplace.third_party;"
```

Once populated, catalyrst reads approved roots from `squid_marketplace.third_party*`
(block-pinned via `third_party_root_change` when a blocks source is configured, else current
head) - no external HTTP at deploy time. For Node-squid ownership see Appendix A.

## 3. Write node (authoritative `POST /entities`)

Do not enable against the production content DB - use a dedicated scratch DB + storage.
Required env (shell export, `EnvironmentFile=` in your systemd unit, or the NixOS module):

```bash
ENABLE_DEPLOYMENTS=true
POSTGRES_CONTENT_DB=content_scratch            # NOT the live `content` DB
STORAGE_ROOT_FOLDER=<DATA_DIR>/content_scratch
ETH_RPC_URL=https://rpc.decentraland.org/mainnet     # EIP-1654 (smart-wallet) sigs
IGNORE_BLOCKCHAIN_ACCESS_CHECKS=false                # default; fail-closed (full ACL). =true only for historical-profile sync
# optional, for access checks:
SQUID_DB_HOST=...  SQUID_DB_USER=squid_ro  SQUID_DB_NAME=marketplace_squid
THIRD_PARTY_ROOT_SOURCE=squid                        # if section 2 done; else leave default + set the subgraph URLs
MAX_DEPLOYMENT_SIZE_BYTES=209715200                  # 200 MiB cap on the deploy body
```

The write path enforces (reference parity): auth-chain signatures incl. EIP-1654; entity
structure / IPFS hashing / ADR-45; per-type size limits; item-representation + content
cross-checks; third-party Merkle proofs; request TTL (20 min back / 15 min forward);
newer-entity rejection; deploy rate-limiter (per-type TTL/size + profile unchanged-metadata).
On success: full overwrite parity (`deleter_deployment`, active-pointer SET/CLEAR).

Smoke test (scratch DB): deploy a self-signed profile via `dcl-cli`/curl multipart and confirm
a 200 + the row in `deployments` + `active_pointers`.

## Environment variable reference (catalyrst-live)

| Var | Default | Purpose |
|---|---|---|
| `HTTP_SERVER_PORT` | `5141` | HTTP listen port |
| `HTTP_SERVER_HOST` | `127.0.0.1` | bind host |
| `CONTENT_VERSION` | `7.6.1+rust` | version string emitted by `/about` and `/content/status` |
| `LAMBDAS_VERSION` | `4.12.0+rust` | version string emitted by `/lambdas/status` |
| `COMMIT_HASH` | `unknown` | commit hash emitted by `/about` |
| `ETH_NETWORK` | `mainnet` | selects mainnet vs testnet defaults for subgraph URLs |
| `PUBLIC_URL` | `http://<host>:<port>` | externally-reachable base URL (used to build `CONTENT_URL`, `LAMBDAS_URL`, `CONTENT_SERVER_ADDRESS` defaults) |
| `CONTENT_URL` | `<PUBLIC_URL>/content/` | content base URL in `/about` |
| `LAMBDAS_URL` | `<PUBLIC_URL>/lambdas/` | lambdas base URL in `/about` |
| `CONTENT_SERVER_ADDRESS` | `<PUBLIC_URL>/content` | `contentServerAddress` in `/about` |
| `REALM_NAME` | unset | optional realm name reported by `/about` and shown on the landing page (`GET /`) |
| `CATALYRST_SERVICE_URLS` | unset | comma-separated `key=baseurl` pairs for the sibling bundles, probed at `{base}/health` to power the live service-health dots on the landing page (`GET /`) and `/admin`. Keys: `explore,create,social,data,ab-cdn,social-rpc,scene-state,profile-images,explorer-api,telemetry`. Unset keys render as "not configured" (never "down"). E.g. `explore=http://127.0.0.1:5143,data=http://127.0.0.1:5146`. `/admin` is loopback/private-network-only - not proxied on the public edge. |
| `ADMIN_ADDRESSES` | unset | comma-separated `0x...` allowlist for the admin console's write controls. Unset => the console is read-only and every `POST /admin/api/*` mutation returns 403 (default-safe). See [docs/operations/admin-console.md](./docs/operations/admin-console.md). |
| `SESSION_SECRET` | unset | HMAC key for the admin session cookie + sign-in nonce. Unset => admin write controls disabled (same as no `ADMIN_ADDRESSES`). Use a long random value. |
| `ADMIN_SESSION_TTL_SECS` | `43200` | admin session lifetime (seconds, default 12h). |
| `ADMIN_COOKIE_INSECURE` | unset | set `1` to drop the cookie `Secure` flag - only for a plain-HTTP private network with no TLS terminator (localhost is already a secure context). |
| `COMMS_MODERATOR_TOKEN` / `MODERATOR_TOKEN` | unset | bearer the console forwards to comms for ban/unban/warn; unset => social controls hidden. |
| `AB_REGISTRY_ADMIN_TOKEN` / `API_ADMIN_TOKEN` | unset | bearer forwarded to ab-registry for registry re-ingest / AB cache flush; unset => create controls hidden. |
| `DEBUGGING_SECRET` | unset | secret injected into the scene-state reload call; unset => scene controls hidden. |
| `PROFILE_CDN_BASE_URL` | `https://profile-images.decentraland.org` | base URL for rebuilt profile snapshot links |
| `POSTGRES_HOST` | `/run/postgresql` | content DB socket/host |
| `POSTGRES_PORT` | `5432` | content DB port |
| `POSTGRES_CONTENT_USER` / `_PASSWORD` | (required) | content DB creds |
| `POSTGRES_CONTENT_DB` | `content` | content DB name |
| `PG_POOL_SIZE` | `50` | main content-DB pool max connections |
| `SQUID_PG_POOL_SIZE` | `10` | squid read-pool max connections |
| `SYNC_PG_POOL_SIZE` | `40` | sync-DB pool max connections |
| `STORAGE_ROOT_FOLDER` | `<DATA_DIR>/content` | content blob root |
| `STORAGE_X_ACCEL_BASE` | unset | when set, catalyrst returns `X-Accel-Redirect: <base>/<sha1[..2]>/<hash>` + empty body on `/content/contents/{hash}`, thumbnail, and image endpoints so nginx serves the bytes via `sendfile` (see "nginx X-Accel-Redirect" below). Leave unset for dev/docker/podman setups without nginx. |
| `SQUID_DB_HOST` / `SQUID_DB_PORT` | inherits `POSTGRES_HOST` / `POSTGRES_PORT` | squid DB socket/host + port |
| `SQUID_DB_USER` | `squid_ro` | squid DB user |
| `SQUID_DB_PASSWORD` | unset | squid DB password (if not peer-auth) |
| `SQUID_DB_NAME` | `marketplace_squid` | squid DB name |
| `SYNC_ENABLED` | `false` | enable upstream sync |
| `SYNC_SOURCE` | `http://127.0.0.1:5140` | upstream peer(s), comma-separated |
| `SYNC_DB_NAME` | `content_rust` | sync replica DB name |
| `SYNC_STORAGE_ROOT` | `<DATA_DIR>/content_rust` | sync replica blob root |
| `CONCURRENT_SYNC_DOWNLOADS` | `200` | parallel content downloads during sync |
| `CONNECTIONS_MAX_IDLE` | `25` | sync HTTP client max idle connections per host |
| `PHASED_SYNC` | `true` | run the phased bootstrap (snapshots, then pointer-changes) |
| `SNAPSHOT_GENERATION_INTERVAL_HOURS` | `6` | how often to regenerate `/content/snapshots` |
| `RETRY_FAILED_ENABLED` | `true` | run the failed-deployment retry worker (sync mode only) |
| `RETRY_FAILED_PRUNE_TTL_DAYS` | `7` | TTL after which `failed_deployments` rows are pruned |
| `SYNC_RETRY_CONCURRENCY` | `10` | parallel retries per retry-worker pass |
| `ENABLE_DEPLOYMENTS` | `false` | accept `POST /entities` |
| `IGNORE_BLOCKCHAIN_ACCESS_CHECKS` | `false` | skip on-chain ACL checks (fail-closed: only an explicit `=true` bypasses ownership/access enforcement) |
| `ETH_RPC_URL` | `https://rpc.decentraland.org/mainnet` | EIP-1654 eth_call (HTTPS required) |
| `THIRD_PARTY_ROOT_SOURCE` | `subgraph` | `squid` to use the local registry index |
| `THIRD_PARTY_REFRESH_HOURS` | unset (off) | >0 => pure-Rust background task that bootstraps + refreshes `squid_marketplace.third_party` from the registry subgraph (replaces the Node squid processor; pair with `THIRD_PARTY_ROOT_SOURCE=squid`) |
| `THIRD_PARTY_REGISTRY_L2_SUBGRAPH_URL` / `BLOCKS_L2_SUBGRAPH_URL` | mainnet defaults | external root + block source |
| `MAX_DEPLOYMENT_SIZE_BYTES` | `209715200` | deploy body cap |
| `REQUEST_TIMEOUT_SECS` | `60` | whole-request timeout (set `0` to disable) |
| `READ_ONLY` | `false` | start with `POST /entities` disabled (toggleable from the admin console) |
| `ENTITIES_CACHE_CONTROL_MAX_AGE` | `10` | `Cache-Control: public, max-age=N` on `/content/entities/{type}` and `/content/entities/active` responses (`0` disables the header) |
| `ADDITIONAL_DECENTRALAND_ADDRESS` | unset | extra privileged deployer |

## Service-crate migrations run automatically at boot

Every service crate that owns a database applies its sqlx migrations in `build_state()` -
upgrading a binary upgrades its schema on next start, with no separate migration step.
One is destructive:

**catalyrst-world-storage `0003_lowercase_world_names`** lowercases every stored
`world_name` across `world_storage`, `player_storage`, and `env_variables`. Rows whose
names collapse to the same lowercase identity are DELETED down to one deterministic
survivor (an already-lowercase row wins, else the most recently updated). Back up the
world-storage DB and check for mixed-case collisions before first booting a binary that
carries it:

```sql
SELECT lower(world_name), place_id, key, count(*) FROM world_storage
GROUP BY 1,2,3 HAVING count(*) > 1;
```

## nginx X-Accel-Redirect (zero-copy content bytes)

To have nginx `sendfile()` content bytes instead of the Rust process streaming them, set
`STORAGE_X_ACCEL_BASE` on the catalyrst-live unit (e.g.
`STORAGE_X_ACCEL_BASE=/__protected_storage`) and add the matching internal nginx location
pointing at `STORAGE_ROOT_FOLDER/contents`:

```nginx
location /__protected_storage/ {
    internal;
    alias <DATA_DIR>/content/contents/;
    etag off;
    add_header ETag $upstream_http_etag always;
    add_header Access-Control-Expose-Headers $upstream_http_access_control_expose_headers always;
    add_header Cache-Control "public, max-age=31536000, immutable" always;
    add_header X-Content-Type-Options "nosniff" always;
    sendfile on;
    tcp_nopush on;
    aio threads;
    output_buffers 1 256k;
}
```

`etag off;` plus the two `add_header ... $upstream_http_*` lines are required for parity: on an
X-Accel-Redirect nginx discards the upstream response headers, and its static-file module would
otherwise generate its default `<mtime>-<size>` ETag — breaking parity with the TS catalyst,
whose ETag is the quoted content CID. Re-emitting `$upstream_http_etag` (still populated from
the proxied response that issued the redirect) restores the app's CID ETag, and the same trick
restores `Access-Control-Expose-Headers`.

`internal;` = only nginx-internal redirects (issued by catalyrst's `X-Accel-Redirect` header)
hit this path; external clients get 404. The example NixOS module (`nixos/configuration.nix`)
wires both pieces when `services.catalyrst.enable = true;`. With the env var unset catalyrst
streams the file itself, so dev/docker/podman setups without nginx keep working.

## Appendix A. Legacy: Node-squid TPR backfill (advanced)

To have `marketplace-squid-core` own the `third_party*` tables, let the Node squid populate
them and leave `THIRD_PARTY_REFRESH_HOURS` unset on the catalyrst side. The schema matches
section 2's Rust refresher - pick one writer, not both. The squid changes are staged on branch
`feat/index-third-party-registry` of your `marketplace-squid-core` checkout (apply to a
checkout separate from any running indexer).

WARNING: this touches the live polygon indexer - do it in a maintenance window. The migration
only adds two new tables; it does not alter existing ones.

```bash
cd <your-marketplace-squid-core>   # the feature-branch worktree
# regenerate canonical artifacts from the hand-authored sources (recommended):
sqd codegen && sqd typegen        # regenerates models + ABI with canonical names
npm ci && npm run build

# apply the migration (adds third_party + third_party_root_change to squid_marketplace)
sqd migration:apply               # or: npx squid-typeorm-migration apply

# Backfill: the TPR contract emits sparse events from block 26_860_700. Either
#  (a) reset only the polygon processor state to re-stream (heavy), or
#  (b) run a TPR-scoped one-off processor from 26.86M->head (light; recommended).
# Then verify rows landed:
psql "$SQUID_DB" -c "select count(*), count(root) from squid_marketplace.third_party;"
```

Then on the catalyrst side set only `THIRD_PARTY_ROOT_SOURCE=squid` (no
`THIRD_PARTY_REFRESH_HOURS`) so the Node squid remains the sole writer.
