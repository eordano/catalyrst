# catalyrst-lists — Route inventory

Upstream host: the lists service host. Two admin-curated master lists plus
a healthcheck. All routes are **unauthenticated** (no SignedFetch / AuthChain in
the upstream catalog) — no auth gating is added.

Port: the deployment's assigned port (`5143`).
Backing store: the `places_events` database (point at a PostgreSQL instance);
two tiny tables `lists_poi` / `lists_banned_name`, seeded from the live upstream by
`deploy/sync-lists.sh` (run on a daily schedule). The service connects read-only as
a dedicated reader role (`<DB_USER>`).

| Method | Path            | Status | Notes                                                                                                  |
|--------|-----------------|--------|--------------------------------------------------------------------------------------------------------|
| POST   | `/pois`         | DONE   | PRIMARY explorer route (`DecentralandUrl.POI`). Empty body → `{"data": ["x,y", …]}`. Unity throws on null `.data`. |
| POST   | `/banned-names` | DONE   | Not called by explorer; consumed by marketplace-server / catalyrst-market to filter ENS listings. Empty body → `{"data": [...]}`. |
| GET    | `/status`       | DONE   | status-page healthcheck. → `{"commitHash": "<sha>"}` (build `GIT_REV`, falls back to crate version).   |
| GET    | `/ping`         | DONE   | Liveness; echoes the request path (matches sibling crates).                                            |

## Wire shape

Both list endpoints return the bare `{"data": Vec<String>}` envelope the
explorer + marketplace expect (no `ok` wrapper, unlike the catalyrst-places
API). `/pois` is sorted by `coord`, `/banned-names` by `name`.

## Deferred

- **Curation (write) endpoints** — adding/removing POIs and banned names is
  admin-managed upstream; deferred to a later catalyrst-fed signed-write stage.
- **catalyrst-fed signing** — not needed; the explorer only reads.
- The L2 POI contract is **not** read on-chain here; the curated master is
  pulled from the live upstream and cached (see `deploy/sync-lists.sh`).
