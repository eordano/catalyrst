# Endpoint inventory

Source of truth: `crates/catalyrst-server/src/routes.rs`.

Catalyrst exposes three URL surfaces from a single process:

| Surface     | Mount point(s)         | Notes                                                                |
|-------------|------------------------|----------------------------------------------------------------------|
| about       | `/about`               | Root only. Mirrors `/about` shape from the reference catalyst.       |
| content     | `/` + `/content/*`     | Mounted twice — legacy direct access at root, prod via `/content/*`. |
| lambdas     | `/lambdas/*`           | Mounted at root only.                                                |
| infra       | `/metrics`             | Prometheus scrape; loopback only via the deployment.                 |
| fallback    | `*`                    | 404 with `text/plain "Not found"` (reference parity).                |

**Conformance coverage column:** ✓ means there's at least one `catalyrst-conformance` case
hitting this endpoint with bootstrapped or hardcoded inputs. — means happy-path only;
edge cases (404 / malformed / pagination boundary / rate-limit / auth-fail) are
follow-up work.

## /about + content/*

| Method | Path                                                | Handler                                | Conformance |
|--------|-----------------------------------------------------|----------------------------------------|-------------|
| GET    | `/about`                                            | `about::get_about`                     | ✓ (root + /content) |
| GET    | `/content/challenge`                                | `get_challenge::get_challenge`         | ✓           |
| GET    | `/content/entities/{type}?pointer=...`              | `get_entities::get_entities`           | ✓ (scene, profile) |
| GET    | `/content/entities/active/collections/{collectionUrn}` | `filter_by_urn::get_entities_by_collection` | ✓     |
| POST   | `/content/entities/active`                          | `active_entities::get_active_entities` | ✓ (pointer + id) |
| GET    | `/content/contents/{hashId}`                        | `get_content::get_content`             | ✓ (bytes diff) |
| HEAD   | `/content/contents/{hashId}`                        | `get_content::get_content`             | —           |
| GET    | `/content/available-content?cid=...`                | `get_available_content`                | ✓           |
| GET    | `/content/audit/{type}/{entityId}`                  | `get_audit::get_audit`                 | ✓ (profile + scene) |
| GET    | `/content/deployments`                              | `get_deployments::get_deployments`     | ✓ (4 entity types + pagination) |
| GET    | `/content/contents/{hashId}/active-entities`        | `get_active_entities_by_hash`          | ✓           |
| GET    | `/content/failed-deployments`                       | `failed_deployments`                   | ✓           |
| GET    | `/content/pointer-changes`                          | `pointer_changes`                      | ✓ (profile, scene) |
| GET    | `/content/snapshots`                                | `get_snapshots`                        | ✓           |
| GET    | `/content/status`                                   | `status::get_status`                   | ✓           |
| GET    | `/content/queries/items/{pointer}/thumbnail`        | `get_entity_thumbnail`                 | ✓ (sample) |
| HEAD   | `/content/queries/items/{pointer}/thumbnail`        | `get_entity_thumbnail`                 | —           |
| GET    | `/content/queries/items/{pointer}/image`            | `get_entity_image`                     | ✓ (sample) |
| HEAD   | `/content/queries/items/{pointer}/image`            | `get_entity_image`                     | —           |
| GET    | `/content/queries/erc721/{chainId}/{contract}/{option}`            | `get_erc721_entity` | ✓     |
| GET    | `/content/queries/erc721/{chainId}/{contract}/{option}/{emission}` | `get_erc721_entity` | ✓     |
| POST   | `/content/entities` (write mode only)               | `create_entity::create_entity_multipart` | — (write path, separate suite) |

## /lambdas/*

| Method | Path                                                                   | Handler                                  | Conformance |
|--------|------------------------------------------------------------------------|------------------------------------------|-------------|
| POST   | `/lambdas/profiles`                                                    | `lambdas::profiles`                      | ✓ (single id) |
| GET    | `/lambdas/profiles/{id}`                                               | `lambdas::profile_by_id`                 | ✓           |
| GET    | `/lambdas/profile/{id}`                                                | `lambdas::profile_alias`                 | ✓           |
| GET    | `/lambdas/collections/wearables`                                       | `lambdas_catalog::collections_wearables_catalog` | ✓   |
| GET    | `/lambdas/collections/emotes`                                          | `lambdas_catalog::collections_emotes_catalog`    | ✓   |
| GET    | `/lambdas/collections/wearables-by-owner/{owner}`                      | `lambdas::wearables_by_owner`            | ✓           |
| GET    | `/lambdas/collections/emotes-by-owner/{owner}`                         | `lambdas::emotes_by_owner`               | ✓           |
| GET    | `/lambdas/status`                                                      | `lambdas::lambdas_status`                | ✓           |
| GET    | `/lambdas/contracts/servers`                                           | `lambdas_contracts::contracts_servers`   | ✓           |
| GET    | `/lambdas/contracts/pois`                                              | `lambdas_contracts::contracts_pois`      | ✓           |
| GET    | `/lambdas/contracts/denylisted-names`                                  | `lambdas_contracts::contracts_denylisted_names` | ✓    |
| GET    | `/lambdas/third-party-integrations`                                    | `lambdas_contracts::third_party_integrations`   | ✓    |
| GET    | `/lambdas/users/{address}/wearables`                                   | `lambdas_user_items::user_wearables`     | ✓           |
| GET    | `/lambdas/users/{address}/emotes`                                      | `lambdas_user_items::user_emotes`        | ✓           |
| GET    | `/lambdas/users/{address}/third-party-wearables`                       | `lambdas_user_items::user_third_party_wearables` | ✓   |
| GET    | `/lambdas/users/{address}/third-party-wearables/{collectionId}`        | `lambdas_user_items::user_third_party_collection_wearables` | — |
| GET    | `/lambdas/users/{address}/names`                                       | `lambdas_land::user_names`               | ✓           |
| GET    | `/lambdas/users/{address}/lands`                                       | `lambdas_land::user_lands`               | ✓           |
| GET    | `/lambdas/users/{address}/lands-permissions`                           | `lambdas_land::user_lands_permissions`   | ✓           |
| GET    | `/lambdas/users/{address}/parcels/{x}/{y}/permissions`                 | `lambdas_land::parcel_permissions`       | ✓           |
| GET    | `/lambdas/names/{name}/owner`                                          | `lambdas_land::name_owner`               | ✓ (existing + 404) |
| GET    | `/lambdas/parcels/{x}/{y}/operators`                                   | `lambdas_land::parcel_operators`         | ✓ (3 parcels) |
| GET    | `/lambdas/explorer/{address}/wearables`                                | `lambdas_explorer::explorer_wearables`   | ✓           |
| GET    | `/lambdas/explorer/{address}/emotes`                                   | `lambdas_explorer::explorer_emotes`      | ✓           |
| GET    | `/lambdas/nfts/collections`                                            | `lambdas_catalog::nfts_collections`      | ✓           |
| GET    | `/lambdas/outfits/{id}`                                                | `lambdas_catalog::outfits`               | ✓           |

## Cross-cutting concerns (apply to many endpoints)

| Concern                            | Where it lives                          |
|------------------------------------|-----------------------------------------|
| CORS preflight + actual-request    | `crate::cors::cors_middleware`          |
| Prometheus HTTP labels             | `crate::metrics::track_http`            |
| Body-size cap (200 MiB)            | `MAX_DEPLOYMENT_SIZE_BYTES`, `POST /entities` only |
| Request timeout (opt-in)           | `REQUEST_TIMEOUT_SECS` env var          |
| 404 fallback body                  | `text/plain "Not found"` (reference parity) |
| Per-endpoint error body shape      | `crate::errors::AppError` — `{ error, message? }` |

## Known volatile fields (skipped by the diff)

Defined in `crates/catalyrst-conformance/src/diff.rs::IGNORED_FIELDS`.
Current set: `currentTime`, `commitHash`, `version`, `realmName`, `publicUrl`,
`url`, `challengeText`, `lastSyncWithDAO`, `synchronizationTime`,
`generationTimestamp`, `id`, `scope`, `next`, `self`, `previous`.

Numbers that look like Unix-ms timestamps (`> 1e12`, `< 3e12`) are compared
with a 1 s tolerance.

## Suite running tips

```bash
# Local catalyrst (5141) vs local TS catalyst (5140), default:
cargo run -p catalyrst-conformance

# Two public peers:
cargo run -p catalyrst-conformance -- \
  --baseline https://peer.decentraland.org \
  --candidate https://peer-eu1.decentraland.org

# Local catalyrst vs a public peer (baseline = source of truth):
cargo run -p catalyrst-conformance -- \
  --baseline https://peer.decentraland.org \
  --candidate http://127.0.0.1:5141

# Only specific sections:
cargo run -p catalyrst-conformance -- --only about,status,deployments
```

The `--only` accepts comma-separated section names: `about`, `status`,
`challenge`, `snapshots`, `failed-deployments`, `deployments`,
`active-entities`, `entities`, `audit`, `pointer-changes`, `content`,
`available-content`, `active-entities-by-hash`, `thumbnail`, `image`,
`erc721`, `entities-by-collection`, `lambdas-status`, `contracts`,
`third-party-integrations`, `collections`, `nfts-collections`, `profiles`,
`user-items`, `collections-by-owner`, `explorer`, `parcel`, `name-owner`,
`outfits`.

## What's missing (follow-up work)

- **HEAD-method parity** for `/contents/*`, `/queries/items/*/thumbnail|image` —
  same status + headers, empty body.
- **404 / malformed-input / oversized-body** edge cases per endpoint.
- **Pagination boundary** cases (last page, empty page, cursor at exact
  boundary).
- **Rate-limit handling** — public peers return 429 under load; the suite
  should back off + retry rather than fail.
- **Write-path conformance** (`POST /content/entities`) — needs a canned
  test wallet and signed deploy fixtures; out of scope for the read suite.
- **Fixture capture + replay** so CI doesn't depend on live peer health.
