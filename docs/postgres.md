# PostgreSQL configuration & ownership

Single-node Postgres 18 with peer auth over a Unix socket. Two databases:
`content` (catalyrst) and `marketplace_squid` (squid). Both are
peer-authenticated; passwords are only set because the binary panics if
`POSTGRES_CONTENT_PASSWORD` is unset.

## Auth

```
local all         all peer
local replication all peer
```

`listen_addresses = ""` — no TCP listener. `unix_socket_permissions = "0770"`,
and the service users (`catalyrst`, `squid`) are added to the `postgres` group
so they can reach the socket directory.

## Single-node tuning rationale

- `shared_buffers = 3GB`, `effective_cache_size = 8GB` — fit working set.
- `work_mem = 32MB`, `maintenance_work_mem = 512MB`.
- `max_connections = 300`; per-role `CONNECTION LIMIT 120` for `catalyrst`,
  `60` for `squid` (applied by `postgresql-ownership.service`).
- `random_page_cost = 1.1`, `effective_io_concurrency = 200` (SSD).
- `wal_level = "minimal"`, `max_wal_senders = 0` — no replication on this
  node, cuts WAL volume.

## Least-privilege bootstrap (`postgresql-ownership.service`)

Idempotent oneshot, runs at boot. Re-applies:

1. `ALTER ROLE catalyrst|squid NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION CONNECTION LIMIT N` — strips any prior emergency superuser grants.
2. `ALTER DATABASE ... OWNER TO ...` and `REASSIGN OWNED BY {postgres,root} TO ...` for both DBs.
3. `GRANT ALL ON SCHEMA / TABLES / SEQUENCES` and `ALTER DEFAULT PRIVILEGES` so new objects inherit.
4. Cross-DB SELECT: `catalyrst` gets `CONNECT + USAGE + SELECT` on
   `marketplace_squid.squid_marketplace` for third-party Merkle roots +
   ownership lookups. (Plus a default-priv so new tables stay readable.)
5. `REVOKE CONNECT ... FROM PUBLIC` on both DBs.

## `squid-search-path.service`

Per-role `search_path` state is dropped by `pg_upgrade` and by
`pg_restore`. The oneshot re-applies
`ALTER ROLE squid IN DATABASE marketplace_squid SET search_path = squid_marketplace, public;`
(and the same for `root`) on every boot.

## Audit logging

pgaudit is not in nixpkgs `postgresql_18`. Adding it requires building
postgres with a `withPackages` build step. Not done yet.

## Recommended pgbouncer config for catalyrst

If pgbouncer is fronting Postgres, the catalyrst user should run in **session
pool mode**, not the default transaction mode. sqlx (catalyrst's Postgres
driver) caches prepared statements per server-side connection. In transaction
mode every query may land on a different backend conn, so the prep cache never
warms — that adds 1-3 ms per query (parse + plan + bind on every call). Session
mode pins one client conn to one backend conn for the lifetime of the client
connection, so the prep cache survives.

### Trade-off

Session mode increases backend connection pressure (each catalyrst client conn
holds a backend conn full-time). Mitigation: tune `max_client_conn` and
`default_pool_size` on the pgbouncer side. For catalyrst's default sqlx pool
of 120 conns, set `default_pool_size = 120` (or slightly higher) for the
catalyrst user.

### Configure per-user, not globally

Other users on the same pgbouncer (squid, marketplace, etc.) keep TRANSACTION
mode — they have different latency vs throughput trade-offs.

```ini
[pgbouncer]
pool_mode = transaction   ; default for everyone else
max_client_conn = 1000
default_pool_size = 25

[users]
; per-user override for catalyrst — session mode preserves sqlx prep cache
catalyrst_user = pool_mode=session pool_size=150
```

### Verification

Confirm session mode is active for the catalyrst user:

```bash
psql -h <pgbouncer> -U <admin> pgbouncer -c 'SHOW POOLS;' | grep catalyrst
```

In SESSION mode `cl_active` equals `sv_active` while connections are open (no
churn).

### What NOT to do

- Don't switch the squid user's pool to session mode — squid has bursty
  short-lived queries where transaction mode is genuinely better.
- Don't enable `server_reset_query` (default `DISCARD ALL`) for the catalyrst
  user — that would wipe the prep cache. `server_reset_query_always = 0` is
  implicit in session mode.

### Expected impact

Per the bench numbers under `.bench/results.json`: most lambdas p50/p99 drops a
few ms; pointer-changes / audit / available-content tail drops more sharply
(those queries are particularly prep-cache-sensitive due to dynamic WHERE
clauses). Re-bench after the change to confirm — the bench tool persists
previous results, so the `Δp50/Δp99` columns will show the delta.
