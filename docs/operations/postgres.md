# PostgreSQL - ownership, tuning, pgbouncer

Postgres **18** required (DEPLOYMENT.md). Reference deployment: single node, peer auth over a Unix
socket, no TCP listener (`listen_addresses = ""`), `unix_socket_permissions = 0770` with service
users added to the `postgres` group. Principal databases: `content` (catalyrst) and
`marketplace_squid` (squid indexer). Passwords exist only because the binary requires
`POSTGRES_CONTENT_PASSWORD` to be set - auth is actually peer.

Service crates own further databases (created by the deployment, migrated by each crate's sqlx
migrations at `build_state()`): `communities`, `comms_gatekeeper`, `notifications`, `badges`,
`credits`, `ab_registry`, `places_events` (reader), and others - least-privilege role recipe in
[deploy/runbook.md](../deploy/runbook.md).

Single-node tuning:

- `shared_buffers = 3GB`, `effective_cache_size = 8GB`; `work_mem = 32MB`,
  `maintenance_work_mem = 512MB`.
- `max_connections = 300`; per-role `CONNECTION LIMIT` (120 catalyrst / 60 squid) applied by an
  idempotent boot-time ownership service.
- `random_page_cost = 1.1`, `effective_io_concurrency = 200` (SSD).
- `wal_level = minimal`, `max_wal_senders = 0` - no replication on this node; cuts WAL volume.

**Boot-time least-privilege bootstrap:** an idempotent oneshot re-applies on every boot: strip
superuser/createdb/createrole/replication from service roles (undoes any emergency grant),
re-assert DB ownership + `REASSIGN OWNED`, grant-all + default privileges, cross-DB `SELECT` for
catalyrst on `marketplace_squid.squid_marketplace` (third-party Merkle roots + ownership lookups),
and `REVOKE CONNECT ... FROM PUBLIC`.

Separate gotcha: per-role `search_path` settings are dropped by `pg_upgrade` and `pg_restore`; a
second oneshot re-applies `ALTER ROLE squid IN DATABASE marketplace_squid SET search_path =
squid_marketplace, public;` on every boot.

## pgbouncer: catalyrst wants SESSION mode

sqlx caches prepared statements per server-side connection. In pgbouncer's default transaction
mode every query may land on a different backend connection, so the prep cache never warms -
+1-3 ms per query. Run the catalyrst user in session mode, per-user, leaving everyone else
(squid etc. - bursty short-lived queries) on transaction mode:

```ini
[pgbouncer]
pool_mode = transaction
max_client_conn = 1000
default_pool_size = 25

[users]
catalyrst_user = pool_mode=session pool_size=150
```

Each catalyrst client connection then holds a backend connection full-time - size
`default_pool_size` >= the sum of the content core's sqlx pools (env-tunable: `PG_POOL_SIZE` 50
+ `SYNC_PG_POOL_SIZE` 40 + `SQUID_PG_POOL_SIZE` 10 by default; the role's `CONNECTION LIMIT`
is 120). Do NOT enable `server_reset_query`
(`DISCARD ALL`) for the catalyrst user - it wipes the prep cache. Verify with `SHOW POOLS;`: in
session mode `cl_active` equals `sv_active` while connections are open. Most sensitive to
prep-cache loss: pointer-changes / audit / available-content (dynamic WHERE clauses) - re-bench
after any pooling change.

Audit logging: pgaudit is not in nixpkgs `postgresql_18`; adding it requires a `withPackages`
postgres build. Not done yet.
