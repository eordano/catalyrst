# catalyrst-builder routes

Rust port of `builder-api.decentraland.org` (decentraland/builder-server); serves only the slice the
explorer's `BuilderApiDtos` consume. Listens on the deployment's assigned port (`5144`; see
the deployment's `catalyrst-builder` env file), PostgreSQL at `<DB_HOST>:5433`, dedicated `builder` database.

| Method | Path | Auth | Status | Notes |
|---|---|---|---|---|
| GET | `/ping` | none | done | liveness |
| GET | `/v1/collections/{id}/items` | signed-fetch (AuthChain) | done | primary explorer route. Verifies AuthChain via `catalyrst_crypto`, enforces owner / admin allowlist, returns `{ok,data:[FullItem]}` with `contents:{file->hash}` + `type`. Optional `status`/`mappingStatus`/`synced`/`name`/`page`/`limit` params accepted. |
| GET | `/v1/collections/{id}` | signed-fetch (AuthChain), owner or admin | done | Single-collection lookup (`handlers/collections.rs::get_collection`); `require_signer` is mandatory, then 401 unless the signer is the collection's `eth_address` owner or in `BUILDER_ADMIN_ADDRESSES`. |
| GET | `/v1/collections/curation` | admin (bearer or admin-address signed-fetch) | done | Curation queue listing (`handlers/curation.rs::get_curation_collections`), behind the same `authorize_admin` gate as the curation PATCHes (upstream serves GET /curations behind `withAuthentication`). |
| GET | `/v1/{address}/collections` | none (public) | done | Address-scoped reads off the **marketplace squid** (`squid_marketplace.collection`), NOT the draft builder DB. Returns the address's *published on-chain* collections (creator/owner/manager/minter), shaped for the sites `BuilderCollectionSchema` (`id,name,type,is_published,is_approved,reviewed_at,created_at,updated_at,contract_address,urn,status,count,...`; timestamps in ms). Empty `{ok,data:[]}` when `BUILDER_MARKETPLACE_PG_CONNECTION_STRING` is unset. |
| GET | `/v1/{address}/items` | none (public) | done | Address-scoped reads off `squid_marketplace.item` (joined to `metadata`/`wearable`/`emote` for name+category). Returns the address's on-chain items (by `creator`); `?onlyOrphans=true` filters to collection-less drafts, which never exist on-chain -> empty. Shaped for the sites `OrphanItemSchema` + raw on-chain extras. |
| GET / HEAD | `/v1/storage/contents/{hash}` | none | done | 301-redirect to `{BUILDER_CONTENT_BUCKET_URL}/contents/{hash}` (+`?ts=`), immutable cache-control. Same handler for GET/HEAD (301 has no body). |
| GET | `/v1/storage/contents/{hash}/exists` | none | done | HEAD-style existence check (`handlers/storage.rs::head_storage_content_exists`). |
| POST | `/v1/newsletter` | none | done | `{email,source?}` (source defaults to `Builder`). Email trimmed + lowercased + validated server-side: missing/invalid -> `400 {ok:false,error}`. Valid -> durable upsert into `newsletter_subscriptions` (PK email; conflict updates `source`) -> `200 {ok:true}`; DB failure -> `500` (no silent success). Optional SaaS forward stays best-effort. |
| PATCH | `/v1/collections/{id}/items/{item}/status` | admin (bearer or admin-address signed-fetch) | done | Single-item curation status update (`handlers/curation.rs::patch_item_status`). |
| PATCH | `/v1/collections/{id}/items/status` | admin (bearer or admin-address signed-fetch) | done | Bulk curation status update, up to 1000 items (`handlers/curation.rs::patch_items_status_bulk`). |

## Access control

`/v1/collections/{id}/items` allows the collection owner (eth_address) or any address in
`BUILDER_ADMIN_ADDRESSES` (committee/admin allowlist); upstream's on-chain committee-membership and
merged-collection manager checks are folded into this allowlist - extend `ItemsComponent` for richer
rules. The curation routes use a separate `authorize_admin` gate in `handlers/curation.rs`: either a
timing-safe-compared `Authorization: Bearer <CATALYRST_BUILDER_ADMIN_TOKEN>`, or a signed-fetch
(AuthChain) request from an address in `BUILDER_ADMIN_ADDRESSES`. The sites committee page reads the
queue server-side with that bearer (shared via `sites.env`).

## Schema

`migrations/0001_initial.sql`: `collections`, `items`, `item_contents`, `newsletter_subscriptions`.
A minimal, collapsed shape - not a literal replay of builder-server's node-pg-migrate sequence.
