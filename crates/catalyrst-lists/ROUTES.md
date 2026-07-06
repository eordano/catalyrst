# catalyrst-lists - Route inventory

Two admin-curated master lists plus a healthcheck; all routes unauthenticated (no SignedFetch /
AuthChain in the upstream catalog). Port: the deployment's assigned port (`5143`). Backing store: the
`places_events` database, tables `lists_poi` / `lists_banned_name`, seeded from the live upstream by
`deploy/sync-lists.sh` (daily schedule); the service connects read-only as a dedicated reader role (`<DB_USER>`).

| Method | Path            | Status | Notes                                                                                                  |
|--------|-----------------|--------|--------------------------------------------------------------------------------------------------------|
| POST   | `/pois`         | DONE   | PRIMARY explorer route (`DecentralandUrl.POI`). Empty body -> `{"data": ["x,y", ...]}`. Unity throws on null `.data`. |
| POST   | `/banned-names` | DONE   | Not called by explorer; consumed by marketplace-server / catalyrst-market to filter ENS listings. Empty body -> `{"data": [...]}`. |
| GET    | `/status`       | DONE   | status-page healthcheck. -> `{"commitHash": "<sha>"}` (build `GIT_REV`, falls back to crate version).   |
| GET    | `/ping`         | DONE   | Liveness; echoes the request path (matches sibling crates).                                            |

Both list endpoints return the bare `{"data": Vec<String>}` envelope the explorer + marketplace expect
(no `ok` wrapper, unlike the catalyrst-places API). `/pois` sorted by `coord`, `/banned-names` by `name`.

Deferred: curation (write) endpoints - admin-managed upstream, deferred to a later catalyrst-fed
signed-write stage; catalyrst-fed signing not needed (the explorer only reads). The L2 POI contract is
NOT read on-chain here; the curated master is pulled from the live upstream and cached (`deploy/sync-lists.sh`).
