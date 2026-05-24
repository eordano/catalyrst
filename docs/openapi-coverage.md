# OpenAPI coverage map

`docs/openapi.yaml` documents catalyrst's HTTP surface at two depths.

## Tier 1 — full schemas

Request body (POST), all query/path params, full success-response schema, and
documented error responses (400/404/500):

| Method | Path                                          |
|--------|-----------------------------------------------|
| GET    | `/about`                                      |
| GET    | `/content/status`                             |
| GET    | `/content/snapshots`                          |
| GET    | `/content/deployments`                        |
| GET    | `/content/pointer-changes`                    |
| POST   | `/content/entities/active`                    |
| GET    | `/content/contents/{hashId}`                  |
| GET    | `/content/audit/{type}/{entityId}`            |
| GET    | `/content/available-content`                  |
| POST   | `/lambdas/profiles`                           |
| GET    | `/lambdas/profiles/{id}`                      |
| GET    | `/lambdas/users/{address}/wearables`          |

Schemas were derived from the actual Rust handlers in
`crates/catalyrst-server/src/handlers/` (return types, `serde_json::json!`
literals, query-string parsing). Reference-parity quirks (e.g. `outfit`/`outfits`
rejected at audit/entities, `/names/{name}/owner` 404 with empty body) are
called out in the path-level `description`.

## Tier 2 — stubs

Method, summary, one-sentence description, path/query params, and a generic
`200 -> application/json -> { type: object }` response. Sufficient for tooling
to enumerate the surface but not for client-code generation. Covers everything
else routed by `crates/catalyrst-server/src/routes.rs`:

- All other `/content/*` routes (challenge, entities by type & collection,
  contents/{hash}/active-entities, failed-deployments, queries/items thumbnails
  + images, queries/erc721/*, write-path `POST /entities`).
- All other `/lambdas/*` routes (profile alias, collections wearables/emotes,
  *-by-owner, status, contracts/*, third-party-integrations, users/* emotes +
  third-party + names + lands + permissions, parcels operators, explorer/*,
  nfts/collections, outfits).
- `/metrics`.

HEAD operations on `/content/contents/{hashId}` and the two
`/queries/items/{pointer}/{thumbnail|image}` endpoints are listed as `head:`
entries on the same path (status + headers, empty body).

## Reusable component schemas

Defined under `components.schemas`:

- `Entity` — content entity envelope (`version`, `id`, `type`, `pointers`,
  `timestamp`, `content`, `metadata`).
- `Deployment` — `Entity` + `auditInfo` + `localTimestamp` flattened, mirroring
  `filter_deployment_fields` in `get_deployments.rs`.
- `AuthChain` — `array of AuthChainLink`.
- `AuthChainLink` — `oneOf` over the three variants `SIGNER`,
  `ECDSA_EPHEMERAL`, `ECDSA_SIGNED_ENTITY` with a `type` discriminator.
- `Profile` — avatar shape returned by `/lambdas/profiles*` (timestamp +
  avatars[] with snapshots, eyes/hair/skin color, wearables/emotes URNs).
- `Pagination` — `{ offset, limit, moreData, next?, lastId? }`.
- `ErrorBody` — `{ error, message? }` (matches
  `crates/catalyrst-server/src/errors.rs`).
- `EntityType` — enum (`scene|profile|wearable|emote|store|outfit`). The
  description notes the plural alias and audit/entities rejection for outfit.
- `AboutResponse` — full nested shape of `GET /about` (content, lambdas,
  configurations.map, comms, bff).

Two reusable response refs: `BadRequest` and `InternalError`, both wrapping
`ErrorBody`.

## Partial / punted

- **`POST /content/entities`** (write path) — the multipart layout is
  acknowledged but the per-part schema is left as
  `multipart/form-data: { entityId, authChain, additionalProperties: binary }`.
  Fully expanding the signed-entity envelope was out of scope for this pass.
- **Snapshot file format** — `/content/snapshots` documents the manifest
  envelope only, not the line-delimited entity dump that the `hash` link
  points at.
- **Lambdas collection / catalog response shapes** (`/lambdas/collections/*`,
  `/lambdas/nfts/collections`, `/lambdas/contracts/*`,
  `/lambdas/third-party-integrations`, `/lambdas/explorer/*`,
  `/lambdas/outfits/{id}`) — stubs only. These call into off-chain graph
  providers whose response shapes vary; a follow-up pass should capture them
  from live responses.
- **HEAD-method response headers** — present as separate `head:` entries but
  not exhaustively spec'd (parity with GET).
