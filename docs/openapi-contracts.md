# OpenAPI route contracts -- pilot services and the per-service adoption recipe

Three pilot crates (`catalyrst-events`, `catalyrst-places`, `catalyrst-worlds`) carry
typed route contracts: every route in the service router is annotated with
`#[utoipa::path]`, the axum router is built with `utoipa-axum`'s `OpenApiRouter` +
`routes!` so the path string in the annotation is the single source for both the
mounted route and the spec, and each service serves its spec at `GET /openapi.json`.
The generated specs and TypeScript clients are committed under
`catalyrst/ui3/src/generated/catalyst/openapi/` and consumed by sites through
`catalyrst/sites/packages/data/src/lib/catalyst/typed.ts` (`eventsApiPath` / `placesApiPath` /
`worldsApiPath`). A server-side route rename therefore fails `tsc` in sites at every
stale call site.

Errors use one shared envelope: `catalyrst_types::ApiErrorBody`
(`{ ok: false, error, message, federation_adr? }`, feature `openapi` adds the
`ToSchema` derive). It is a superset of the three pilots' previous bodies
(`error`, `message`, bare `error`), so existing readers keep working.

## Pipeline

- `cargo test -p <crate> export_bindings` also runs `export_bindings_openapi`
  (a `#[cfg(test)]` test in each pilot's `lib.rs`) which writes
  `$TS_RS_EXPORT_DIR/openapi/<service>.openapi.json` from `api_router_with_spec()`.
- `catalyrst/sites/scripts/gen-openapi-ts.mts` converts each `*.openapi.json` in that dir to a
  `.ts` types file (openapi-typescript 6.7.6, exact-pinned, ignore-scripts per
  `catalyrst/sites/.npmrc`).
- `npm run gen:types` (sites) regenerates everything into the committed dir;
  `npm run gen:types:check` and the tracked pre-commit hook diff a fresh
  regeneration against the committed files, so hand-editing either the spec JSON or
  the generated TS fails the gate.

## Adopting the next service

1. Cargo.toml: add `utoipa = { workspace = true }`, `utoipa-axum = { workspace = true }`,
   and switch `catalyrst-types` to `features = ["openapi"]`. Add a `ts = []` feature
   if the crate has none (the gen scripts pass `--features <crate>/ts`).
2. Route the error responses through `catalyrst_types::ApiErrorBody` in the crate's
   `IntoResponse for ApiError` (keep any extra fields by extending the shared type,
   not by forking the envelope).
3. Derive `utoipa::ToSchema` on every serialized request/response struct the routes
   name. `serde_json::Value` fields need `#[schema(value_type = Object)]` (or a
   tighter override); chrono types are covered by the workspace `utoipa` `chrono`
   feature.
4. Annotate every handler with `#[utoipa::path(method, path, params, request_body,
   responses)]`, spelling error statuses with `body = ApiErrorBody`. Paths are
   relative to the router they are registered on; `OpenApiRouter::nest` prefixes
   both the routes and the spec.
5. Rebuild the crate's router as `api_router_with_spec() -> (Router<State>, OpenApi)`
   using `OpenApiRouter::with_openapi(ApiDoc::openapi()).routes(routes!(...))` +
   `split_for_parts()`; handlers sharing a path go in one `routes!` call. Per-route
   layers use `UtoipaMethodRouterExt::layer`. Keep `api_router()` mounting
   `/openapi.json` from the spec.
6. Copy the `export_bindings_openapi` test from a pilot's `lib.rs`, changing the
   output filename to `<service>.openapi.json`.
7. Declare the spec in the crate's own `Cargo.toml` -- this is what enrols it in
   the gate, there is no list to edit elsewhere:

   ```toml
   [package.metadata.generated]
   gate = "catalyrst/sites/scripts/gen-ts-check.sh"
   openapi = "catalyrst/ui3/src/generated/catalyst/openapi/<service>.openapi.json"
   ```

   The declared set is enforced on the way out too: `gen-openapi-ts.mts` reads it
   from `generated-artefacts.mjs --list catalyrst-openapi` and exits 3 if the
   produced specs differ, so a crate can't start or stop emitting a spec without
   the declaration and the committed artefacts moving with it. Then run
   `npm run gen:types` in sites and commit the regenerated
   `catalyrst/ui3/src/generated/catalyst/openapi/` files and `catalyrst/ui3/src/generated/INDEX.json`.
8. In sites, add a `servicePath<...>` export to `catalyrst/sites/packages/data/src/lib/catalyst/typed.ts`
   with the service's mount prefix, then replace the lib's string-URL fetch paths
   with typed calls (`xApiPath("get", "/spec/path/{param}", { param })`). Behavior
   is unchanged: the helper only builds the same URL string, but the path/method
   pair must exist in the generated spec.
9. Verify: crate tests green, `npm run typecheck` green, then drive the live page
   that consumes the service and confirm real data.

## Route-level contract gate (standing)

Each pilot carries `tests/contract_gate.rs`: one DB-integration test that boots the
spec'd router against a scratch schema/database on the reference cluster
(`CATALYRST_<CRATE>_TEST_PG`, `:5434`, created and dropped per run, skipped silently
when unset like every other DB test), drives every spec'd operation through
`tower::oneshot` (a success case plus one documented-error case each), and
validates every response against the live `api_router_with_spec()` spec: expected
status, status documented, declared content type, and JSON-schema conformance
(components-resolved, via `jsonschema`). A coverage ledger fails the test if any
spec'd operation was never hit, lacks a success case, or lacks an error case.

The shared harness is `crates/catalyrst-contract-gate`: scratch-PG setup
(`pg::ScratchSchema` / `pg::ScratchDb` -- the worlds gate needs its own database for
the `squid_marketplace.ens` NAME-ownership fixture), a `Case` request builder with
signed-fetch (`signed` / `signed_meta`), bearer and multipart support, and the
`Gate` validator/ledger. Unreachable documented statuses are explicit waivers in
the test (`waive_error` / `waive_success` with a reason string), not silent gaps.

Run: `cargo test -p <crate> --test contract_gate` with
`CATALYRST_<CRATE>_TEST_PG="postgres://postgres@%2F<pg-socket-dir>:5434/postgres"`;
each service's gate finishes in ~1s once built. Gate-catch proven:
dropping `data` from the events categories response fails the test with
`"data" is a required property`. First catches, fixed in the annotations: admin
bearer gates answer 403 not 401 (events moderation, places reports, all worlds
admin routes), places social pages serve text/html, worlds deploy errors use the
legacy `{ "errors": [...] }` body, scene delete answers 200 not 204, and
ban-status requires an `address` query and answers 503 without a comms-gatekeeper.

## sqlx offline query data (`crates/catalyrst-server/.sqlx`)

catalyrst-server's static content-table statements are `sqlx::query!` /
`query_as!` / `query_scalar!` macros: each one is type-checked against a real
database at compile time. With `DATABASE_URL` unset (every CI job, `nix build`,
any machine without postgres) the macros resolve instead from
`crates/catalyrst-server/.sqlx/` -- committed per-query JSON descriptions of the
prepared statement. Without that directory the crate does not compile offline.

After editing any macro query (or a migration a query depends on), regenerate
it against a scratch database with all of `crates/catalyrst-server/migrations/`
applied in order:

```bash
cd crates/catalyrst-server
DATABASE_URL=postgres://... cargo sqlx prepare -- --all-targets
```

(`sqlx-cli` pinned to the workspace's sqlx minor, 0.9.x; `--all-targets` covers
queries reachable only from the `catalyrst-live` bin and the integration
tests.) Commit the changed `.sqlx/` files with the query edit. CI's
`sqlx-prepare-check` job replays exactly this against a migrated postgres:16
service and fails on any drift between the sources and the committed data; the
`schema_conformance` integration test independently PREPAREs every extracted
SQL literal -- macro or not -- against the migrated schema.

## Verified

- 88 routes spec'd across the three pilots (`events` 21, `places` 35, `worlds` 32).
- Contract-catch proven: renaming `/api/events/categories` server-side in a scratch
  worktree and regenerating made sites `tsc` fail at the stale call site
  (`app/lib/catalyst/places/events.ts: error TS2554`).
- Live pages confirmed with real data after adoption: `/whats-on`, `/places`,
  `/creator-hub/deploy-world` (Worlds server "Online" probe).
- `/openapi.json` goes live on each service at its next (human-gated) restart; the
  spec'd router is otherwise route-for-route identical.
