# PostgreSQL — ownership, tuning, pgbouncer

> Status: distilled 2026-07-04 from the reference deployment; re-verified
> 2026-07-03 (docs-stale-audit). Postgres **18** is required (DEPLOYMENT.md).

## Topology of the reference deployment

Single-node Postgres 18, peer auth over a Unix socket, no TCP listener
(`listen_addresses = ""`), `unix_socket_permissions = 0770` with service users
added to the `postgres` group. Two principal databases on the content cluster:
`content` (catalyrst) and `marketplace_squid` (squid indexer). Passwords exist
only because the binary requires `POSTGRES_CONTENT_PASSWORD` to be set — auth
is actually peer.

Service crates own further databases (created by the deployment, migrated by
each crate's sqlx migrations at `build_state()`): `communities`,
`comms_gatekeeper`, `notifications`, `badges`, `credits`, `ab_registry`,
`places_events` (reader), and others — see [deploy/runbook.md](../deploy/runbook.md)
for the least-privilege role recipe (read-only roles for query-only bundles,
owner roles where a crate runs DDL migrations).

## Single-node tuning rationale

- `shared_buffers = 3GB`, `effective_cache_size = 8GB` — fit the working set.
- `work_mem = 32MB`, `maintenance_work_mem = 512MB`.
- `max_connections = 300`; per-role `CONNECTION LIMIT` (120 catalyrst /
  60 squid) applied by an idempotent boot-time ownership service.
- `random_page_cost = 1.1`, `effective_io_concurrency = 200` (SSD).
- `wal_level = minimal`, `max_wal_senders = 0` — no replication on this node;
  cuts WAL volume.

## Boot-time least-privilege bootstrap

An idempotent oneshot re-applies on every boot: strip superuser/createdb/
createrole/replication from service roles (undoes any emergency grant),
re-assert DB ownership + `REASSIGN OWNED`, grant-all + default privileges so
new objects inherit, cross-DB `SELECT` for catalyrst on
`marketplace_squid.squid_marketplace` (third-party Merkle roots + ownership
lookups), and `REVOKE CONNECT … FROM PUBLIC`.

Separate gotcha: per-role `search_path` settings are dropped by `pg_upgrade`
and `pg_restore`; a second oneshot re-applies
`ALTER ROLE squid IN DATABASE marketplace_squid SET search_path =
squid_marketplace, public;` on every boot.

## pgbouncer: catalyrst wants SESSION mode

sqlx caches prepared statements **per server-side connection**. In pgbouncer's
default transaction mode every query may land on a different backend
connection, so the prep cache never warms — +1–3 ms per query (parse + plan +
bind every call). Run the catalyrst user in session mode, per-user, leaving
everyone else (squid etc. — bursty short-lived queries) on transaction mode:

```ini
[pgbouncer]
pool_mode = transaction
max_client_conn = 1000
default_pool_size = 25

[users]
catalyrst_user = pool_mode=session pool_size=150
```

Trade-off: each catalyrst client connection holds a backend connection
full-time — size `default_pool_size` ≥ the sqlx pool (default 120). Do NOT
enable `server_reset_query` (`DISCARD ALL`) for the catalyrst user — it wipes
the prep cache. Verify with `SHOW POOLS;`: in session mode `cl_active` equals
`sv_active` while connections are open.

The endpoints most sensitive to prep-cache loss are pointer-changes / audit /
available-content (dynamic WHERE clauses) — re-bench after any pooling change.

## Audit logging

pgaudit is not in nixpkgs `postgresql_18`; adding it requires a
`withPackages` postgres build. Not done yet.
