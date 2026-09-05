-- catalyrst-db: LAND/name lookup indexes on the marketplace_squid DB (NOT the
-- content DB).
--
-- These cover the hot lookups behind:
--   the deploy validator's parcel-access check (and the batch parcels route)
--   /lambdas/users/{addr}/parcels/{x}/{y}/permissions  (and the POST batch)
--   /lambdas/users/{addr}/lands-permissions
--   /lambdas/parcels/{x}/{y}/operators
--   /v2/parcels/{x}/{y} and the LAND-token metadata route (catalyrst-map)
--   the world-authz owner lookups (catalyrst-worlds)
--
-- The squid schema ships primary keys on `id` only; every coordinate,
-- token-id and subdomain lookup below is otherwise a seq scan.
--
-- Run on the squid DB (NOT the content DB):
--   psql "$SQUID_DATABASE_URL" -f 0002_land_lookup_indexes.sql

-- Covers every coordinate lookup on parcels:
--   crates/catalyrst-validator/src/squid_checker.rs:177 (parcel_ownership)
--     WHERE p.x = $1 AND p.y = $2
--   crates/catalyrst-validator/src/squid_checker.rs:220 (ownership_for, the
--   deploy validator's batched unnest join)
--     JOIN squid_marketplace.parcel p ON p.x = t.x AND p.y = t.y
--   crates/catalyrst-land-authz/src/resolve.rs:83 (parcel_subject)
--     WHERE p.x = $1 AND p.y = $2
--   crates/catalyrst-map/src/handlers/meta.rs:89 (get_parcel_inner)
--     WHERE p.x = $1 AND p.y = $2
CREATE INDEX CONCURRENTLY IF NOT EXISTS
  parcel_x_y
  ON squid_marketplace.parcel (x, y);

-- Covers the token-id side of the LAND registry joins and the LAND-token
-- metadata route:
--   crates/catalyrst-land-authz/src/resolve.rs:164 and :195
--   (parcels_with_update_operator / parcels_updatable_by)
--     JOIN squid_marketplace.parcel p ON p.token_id = tr.token_id
--   crates/catalyrst-map/src/handlers/meta.rs:120 (land_token_sql)
--     WHERE p.token_id = $1::numeric
CREATE INDEX CONCURRENTLY IF NOT EXISTS
  parcel_token_id
  ON squid_marketplace.parcel (token_id);

-- Covers the estate arm of the estate-inherited rights lookup:
--   crates/catalyrst-land-authz/src/resolve.rs:200 (parcels_updatable_by)
--     JOIN squid_marketplace.estate e ON e.token_id = et.token_id
CREATE INDEX CONCURRENTLY IF NOT EXISTS
  estate_token_id
  ON squid_marketplace.estate (token_id);

-- Covers the world-authz owner lookups, which all filter the lowered
-- subdomain (the squid stores it verbatim from the registration event):
--   crates/catalyrst-worlds/src/handlers/deploy/authz.rs:22 and
--   crates/catalyrst-worlds/src/handlers/permissions.rs:169
--   (resolve_name_owner_id, both)
--     WHERE n.category = 'ens' AND lower(e.subdomain) = lower($1)
-- The validator's claimed-name batch
-- (crates/catalyrst-validator/src/squid_checker.rs:391) reaches the same rows
-- with a raw-column `e.subdomain = ANY($1)`, which an expression index cannot
-- serve; that query stays bounded by its per-deployment name list and the
-- planner's nft-side `category = 'ens'` arm instead.
CREATE INDEX CONCURRENTLY IF NOT EXISTS
  ens_lower_subdomain
  ON squid_marketplace.ens (lower(subdomain));
