# catalyrst-builder routes

Rust port of `builder-api.decentraland.org` (decentraland/builder-server). Serves
only the slice the explorer's `BuilderApiDtos` consume. Listens on the
deployment's assigned port (`5141`) and points at a PostgreSQL instance
(`<DB_HOST>:5433`) with a dedicated `builder` database.

| Method | Path | Auth | Status | Notes |
|---|---|---|---|---|
| GET | `/ping` | none | done | liveness |
| GET | `/v1/collections/{id}/items` | signed-fetch (AuthChain) | done | primary explorer route. Verifies AuthChain via `catalyrst_crypto`, enforces owner / admin allowlist, returns `{ok,data:[FullItem]}` with `contents:{file->hash}` + `type`. Optional `status`/`mappingStatus`/`synced`/`name`/`page`/`limit` params accepted. |
| GET / HEAD | `/v1/storage/contents/{hash}` | none | done | 301-redirect to `{BUILDER_CONTENT_BUCKET_URL}/contents/{hash}` (+`?ts=`), immutable cache-control. Same handler for GET/HEAD (301 has no body). |
| POST | `/v1/newsletter` | none | done | `{email,source}` -> persist to `newsletter_subscriptions`, optional SaaS forward, returns `{ok:true}`. |

## Access control

`/v1/collections/{id}/items` allows the collection **owner** (eth_address) or any
address in `BUILDER_ADMIN_ADDRESSES` (committee/admin allowlist). Upstream's
on-chain committee-membership and merged-collection manager checks are folded into
this allowlist; extend `ItemsComponent` if richer rules are needed.

## Schema

`migrations/0001_initial.sql`: `collections`, `items`, `item_contents`,
`newsletter_subscriptions`. A minimal, collapsed shape — not a literal replay of
builder-server's node-pg-migrate sequence.
