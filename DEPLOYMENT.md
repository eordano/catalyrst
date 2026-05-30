# catalyrst deployment & cutover runbook

`catalyrst` is the Rust port of the catalyst content server. The live binary
(`catalyrst-live`) runs in one of three modes, each strictly more privileged
than the last. **All write/sync behavior is opt-in and fail-closed by default.**

**Requires PostgreSQL 18.** The example NixOS configs pin `pkgs.postgresql_18`;
the bootstrap service (`postgresql-ownership.service`) uses 18-only client
binaries. If you stand up Postgres outside the Nix module, install 18.

| Mode | Reads | Syncs from upstream | Accepts `POST /entities` |
|---|---|---|---|
| read-only (default) | ✓ | ✗ (`SYNC_ENABLED=false`) | ✗ (`ReadOnlyDeployer`) |
| sync replica | ✓ | ✓ (`SYNC_ENABLED=true`) | ✗ |
| write node | ✓ | optional | ✓ (`ENABLE_DEPLOYMENTS=true`) |

Build the release binary first (FHS shell is required on this NixOS host):

```bash
cargo build --release --bin catalyrst-live
# → target/release/catalyrst-live
```

---

## 1. Read-only staging (parity test against the live server)

Run `catalyrst-live` read-only on **:5141** alongside a reference content
server on **:5140**, pointing them at the same content DB + storage. catalyrst
cannot write or sync without explicit env flags, so this is safe.

```bash
# start catalyrst read-only, foreground (Ctrl-C to stop)
HTTP_SERVER_HOST=127.0.0.1 \
CATALYRST_PORT=5141 \
POSTGRES_CONTENT_USER=<user> \
POSTGRES_CONTENT_PASSWORD=<pw> \
POSTGRES_CONTENT_DB=content \
STORAGE_ROOT_FOLDER=<DATA_DIR>/content \
  ./target/release/catalyrst-live
# → "catalyrst-live listening 127.0.0.1:5141"

# parity-check the two servers return the same data
curl -s localhost:5140/about  | jq .healthy
curl -s localhost:5141/about  | jq .healthy
curl -s -XPOST localhost:5140/entities/active -H 'content-type: application/json' -d '{"pointers":["0,0"]}' | jq -S . > /tmp/a.json
curl -s -XPOST localhost:5141/entities/active -H 'content-type: application/json' -d '{"pointers":["0,0"]}' | jq -S . > /tmp/b.json
diff /tmp/a.json /tmp/b.json && echo "PARITY OK"
```

For long-running deployments, write your own systemd unit (or use the example
NixOS module in [`nixos/`](./nixos)) — catalyrst doesn't ship an opinionated
unit file. Set `EnvironmentFile=` to a file that supplies the env vars in §3
below.

To make catalyrst the primary later: repoint nginx (or the `:5140` binding)
at it and stop the reference content server. Keep both running side-by-side
until parity is confirmed over time.

---

## 2. Index third-party registry roots locally (removes external dependency)

By default third-party deployments verify Merkle roots against the external
Decentraland registry subgraph. Indexing the roots locally makes verification
self-contained. **The recommended path is pure-Rust:** catalyrst-live has a
built-in background task that bootstraps and refreshes the
`squid_marketplace.third_party*` tables straight from the registry subgraph,
with no Node squid involvement.

Set the following on the write-node (or sync-replica) host:

```bash
THIRD_PARTY_REFRESH_HOURS=24        # >0 enables the refresher (period in hours)
THIRD_PARTY_ROOT_SOURCE=squid       # read roots from the local index
```

On first run catalyrst creates the tables (if missing) and seeds them from the
subgraph; subsequent runs refresh every `THIRD_PARTY_REFRESH_HOURS` hours.
Subgraph URLs can be overridden via `THIRD_PARTY_REGISTRY_L2_SUBGRAPH_URL` and
`BLOCKS_L2_SUBGRAPH_URL`; see §3 for the full env reference. Verify rows landed:

```bash
psql "$SQUID_DB" -c "select count(*), count(root) from squid_marketplace.third_party;"
```

Once populated, catalyrst reads approved roots from
`squid_marketplace.third_party*` (block-pinned via `third_party_root_change`
when a blocks source is configured, else current head) — no external HTTP at
deploy time.

> If you already run the `marketplace-squid-core` Node indexer and would
> rather have it own these tables, see the **Legacy: Node-squid TPR backfill**
> appendix at the end of this document.

---

## 3. Write node (authoritative `POST /entities`)

**Do not enable against the production content DB.** Use a dedicated scratch DB
+ storage so a bad deploy can't corrupt the synced corpus.

Required env (export in your shell, write to your own env file referenced by
`EnvironmentFile=` in your operator-owned systemd unit, or pass via the
example NixOS module):

```bash
ENABLE_DEPLOYMENTS=true
POSTGRES_CONTENT_DB=content_scratch            # NOT the live `content` DB
STORAGE_ROOT_FOLDER=<DATA_DIR>/content_scratch
ETH_RPC_URL=https://rpc.decentraland.org/mainnet     # EIP-1654 (smart-wallet) sigs
IGNORE_BLOCKCHAIN_ACCESS_CHECKS=false                # default; fail-closed (full ACL). =true only for historical-profile sync
# optional, for access checks:
SQUID_DB_HOST=...  SQUID_DB_USER=squid_ro  SQUID_DB_NAME=marketplace_squid
THIRD_PARTY_ROOT_SOURCE=squid                        # if §2 done; else leave default + set the subgraph URLs
MAX_DEPLOYMENT_SIZE_BYTES=209715200                  # 200 MiB cap on the deploy body
```

What the write path enforces (reference parity): auth-chain signatures incl.
EIP-1654; entity structure / IPFS hashing / ADR-45; per-type size limits;
item-representation + content cross-checks; third-party Merkle proofs; request
TTL (20 min back / 15 min forward); newer-entity rejection; the deploy
rate-limiter (per-type TTL/size + profile unchanged-metadata). On success it
persists with full overwrite parity (`deleter_deployment`, active-pointer
SET/CLEAR).

Smoke test (scratch DB): deploy a self-signed profile via `dcl-cli`/curl
multipart and confirm a 200 + the row in `deployments` + `active_pointers`.

---

## Environment variable reference (catalyrst-live)

| Var | Default | Purpose |
|---|---|---|
| `CATALYRST_PORT` | `5141` | HTTP listen port |
| `HTTP_SERVER_HOST` | `127.0.0.1` | bind host |
| `CONTENT_VERSION` | `7.6.1+rust` | version string emitted by `/about` and `/content/status` |
| `LAMBDAS_VERSION` | `4.12.0+rust` | version string emitted by `/lambdas/status` |
| `COMMIT_HASH` | `unknown` | commit hash emitted by `/about` |
| `ETH_NETWORK` | `mainnet` | selects mainnet vs testnet defaults for subgraph URLs |
| `PUBLIC_URL` | `http://<host>:<port>` | externally-reachable base URL (used to build `CONTENT_URL`, `LAMBDAS_URL`, `CONTENT_SERVER_ADDRESS` defaults) |
| `CONTENT_URL` | `<PUBLIC_URL>/content/` | content base URL in `/about` |
| `LAMBDAS_URL` | `<PUBLIC_URL>/lambdas/` | lambdas base URL in `/about` |
| `CONTENT_SERVER_ADDRESS` | `<PUBLIC_URL>/content` | `contentServerAddress` in `/about` |
| `REALM_NAME` | unset | optional realm name reported by `/about` |
| `PROFILE_CDN_BASE_URL` | `https://profile-images.decentraland.org` | base URL for rebuilt profile snapshot links |
| `POSTGRES_HOST` | `/run/postgresql` | content DB socket/host |
| `POSTGRES_PORT` | `5432` | content DB port |
| `POSTGRES_CONTENT_USER` / `_PASSWORD` | — (required) | content DB creds |
| `POSTGRES_CONTENT_DB` | `content` | content DB name |
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
| `SNAPSHOT_GENERATION_INTERVAL_HOURS` | `6` | how often to regenerate `/content/snapshots` |
| `RETRY_FAILED_ENABLED` | `true` | run the failed-deployment retry worker (sync mode only) |
| `RETRY_FAILED_PRUNE_TTL_DAYS` | `7` | TTL after which `failed_deployments` rows are pruned |
| `ENABLE_DEPLOYMENTS` | `false` | accept `POST /entities` |
| `IGNORE_BLOCKCHAIN_ACCESS_CHECKS` | `false` | skip on-chain ACL checks (fail-closed: only an explicit `=true` bypasses ownership/access enforcement) |
| `ETH_RPC_URL` | `https://rpc.decentraland.org/mainnet` | EIP-1654 eth_call (HTTPS required) |
| `THIRD_PARTY_ROOT_SOURCE` | `subgraph` | `squid` to use the local registry index |
| `THIRD_PARTY_REFRESH_HOURS` | unset (off) | >0 ⇒ pure-Rust background task that bootstraps + refreshes `squid_marketplace.third_party` from the registry subgraph (replaces the Node squid processor; pair with `THIRD_PARTY_ROOT_SOURCE=squid`) |
| `THIRD_PARTY_REGISTRY_L2_SUBGRAPH_URL` / `BLOCKS_L2_SUBGRAPH_URL` | mainnet defaults | external root + block source |
| `MAX_DEPLOYMENT_SIZE_BYTES` | `209715200` | deploy body cap |
| `REQUEST_TIMEOUT_SECS` | `60` | whole-request timeout (set `0` to disable) |
| `ADDITIONAL_DECENTRALAND_ADDRESS` | unset | extra privileged deployer |

---

## nginx X-Accel-Redirect (zero-copy content bytes)

By default the Rust process reads each content blob and streams it to the
client. On a CDN-fronted realm that's pure overhead: nginx can `sendfile()`
the same file straight off disk and skip the userspace copy entirely.

To enable, set `STORAGE_X_ACCEL_BASE` on the catalyrst-live unit (e.g.
`STORAGE_X_ACCEL_BASE=/__protected_storage`) **and** add the matching
internal nginx location pointing at `STORAGE_ROOT_FOLDER/contents`:

```nginx
location /__protected_storage/ {
    internal;
    alias <DATA_DIR>/content/contents/;
    add_header Cache-Control "public, max-age=31536000, immutable" always;
    add_header X-Content-Type-Options "nosniff" always;
    sendfile on;
    tcp_nopush on;
    aio threads;
    output_buffers 1 256k;
}
```

`internal;` ensures only nginx-internal redirects (issued by catalyrst's
`X-Accel-Redirect` header) hit this path — external clients get 404. The
example NixOS module (`nixos/configuration.nix`) wires both pieces up for
you when `services.catalyrst.enable = true;`. When the env var is unset,
catalyrst reverts to streaming the file itself, so dev/docker/podman setups
without nginx keep working unchanged.

---

## Appendix A. Legacy: Node-squid TPR backfill (advanced)

If you already operate `marketplace-squid-core` and would prefer it own the
`third_party*` tables (e.g. to share a single ingest pipeline with other
marketplace data), you can have the Node squid populate them and leave
`THIRD_PARTY_REFRESH_HOURS` unset on the catalyrst side. The schema is
compatible with §2's Rust refresher — pick one writer, not both.

The squid changes are staged on branch `feat/index-third-party-registry` of
your `marketplace-squid-core` checkout (apply them to a checkout separate from
any running indexer so the live process is untouched).

> ⚠️ This touches the **live polygon indexer**. Do it in a maintenance window.
> The migration only adds two new tables; it does not alter existing ones.

```bash
cd <your-marketplace-squid-core>   # the feature-branch worktree
# regenerate canonical artifacts from the hand-authored sources (recommended):
sqd codegen && sqd typegen        # regenerates models + ABI with canonical names
npm ci && npm run build

# apply the migration (adds third_party + third_party_root_change to squid_marketplace)
sqd migration:apply               # or: npx squid-typeorm-migration apply

# Backfill: the TPR contract emits sparse events from block 26_860_700. Either
#  (a) reset only the polygon processor state to re-stream (heavy), or
#  (b) run a TPR-scoped one-off processor from 26.86M→head (light; recommended).
# Then verify rows landed:
psql "$SQUID_DB" -c "select count(*), count(root) from squid_marketplace.third_party;"
```

Then on the catalyrst side set only `THIRD_PARTY_ROOT_SOURCE=squid` (no
`THIRD_PARTY_REFRESH_HOURS`) so the Node squid remains the sole writer.
