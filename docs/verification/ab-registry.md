# Verification — catalyrst-ab-registry (service "ab-registry")

Upstream: `decentraland/asset-bundle-registry`
Crate: `crates/catalyrst-ab-registry`
Bundle: create (5144), member alongside builder + camera-reel.
Method: opened the committed Rust handlers, the upstream TS handlers + router, the
`bearerTokenMiddleware` source, the error model, and the Unity net-catalog.
Verdict: **both flagged findings are accurate and survive scrutiny.** Adding two
minor uncaught divergences on the flush-cache path.

## Scope of flagged items

Only two endpoints were flagged (`POST /registry`, `DELETE /flush-cache`). Both are
**admin-only, bearer-gated, server-side operations.** Neither appears in the Unity
net-catalog as a client call (`the Unity net-catalog` —
the client only hits `/profiles`, `/profiles/metadata`, `/entities/active`,
`/entities/versions`, `/worlds/{world}/manifest`, `/entities/status/{id}`). So there
is **no Unity C# converter on either path** and therefore **no null-crash /
request-throws risk** — `client_reaction: ok` is correct for both. Severities
correctly "minor".

## Per-endpoint table

| endpoint | shape | client-reaction | severity | failure-modes-ok | notes |
|---|---|---|---|---|---|
| POST /registry | divergent (STUB) | ok (not client-called) | minor | partial — see gaps | Echoes posted `entityIds` as `successes`, no catalyst fetch, no persist. `admin.rs:9-31`. |
| DELETE /flush-cache | shape match / semantics+body divergent | ok (not client-called) | minor | partial — see gaps | Invalidates moka manifest cache; upstream flushes redis `jobs:*`. Body string casing differs. `admin.rs:33-40`. |

## Confirmed issues

### 1. POST /registry is a semantic no-op (CONFIRMED, on committed tree)
`admin.rs:16-30` reads `body.entityIds[]`, maps each string to `{entityId}`, returns
`{failures:[], successes:[...]}`. It never touches `state.content` /
`state.manifests` / `state.registry`. Upstream `post-registry.ts:31-93` loops each
id: `catalyst.getEntityById`, fetches mac/windows/webgl bundle+LOD status via
`entityStatusFetcher`, builds `Registry.Bundles`/`Versions`, and calls
`registry.persistAndRotateStates`, pushing per-entity `{entityId,error}` to
`failures`. Our stub produces empty `failures` and persists nothing. Real, not fixed.
Minor only because the endpoint is admin/server-side and absent from the client catalog.

### 2. DELETE /flush-cache flushes a different cache + wrong message casing (CONFIRMED)
- Semantics: ours (`admin.rs:38`) calls `state.manifests.invalidate_all()` on the
  moka manifest cache. Upstream (`flush-cache-handler.ts:10`) does
  `memoryStorage.flush('jobs:*')` against redis. Different cache namespace; our port
  has no redis job store, so this is a reasonable substitute, not equivalent.
- Body: ours returns `{"ok":true,"message":"cache flushed"}` (lowercase c); upstream
  returns `{"ok":true,"message":"Cache flushed"}` (capital C). Cosmetic string
  divergence the finding marked "match"; matches on envelope shape, not byte-identical.
  Non-client, harmless.

## Client-crash risks

**None.** Neither `/registry` nor `/flush-cache` is invoked by the Unity client
(no rows for `registry` POST or `flush-cache` on the asset-bundle-registry host in
the net-catalog). No C# DTO/converter to null-deref, no request to throw.

## Failure-mode gaps

All four `POST /registry` claims and both `flush-cache` claims verified against the
actual error path (`admin.rs:require_admin` + `http/errors.rs` + upstream
`routes.ts:48-51` + `bearer-token-middleware.ts`):

1. **API_ADMIN_TOKEN unset → divergence CONFIRMED (finding `ok:false` is right).**
   Ours: `require_admin` (`admin.rs:42-45`) — `admin_token` is `None` →
   `401 {ok:false,message:"admin token not configured"}`. Upstream `routes.ts:48`
   `if (!!adminToken)` — routes **never registered** when unset → 404. So **401 (ours)
   vs 404 (upstream)** on both admin endpoints. Real; benign (server-side, 4xx either way).

2. **Missing/wrong bearer → MATCH CONFIRMED.** Ours → `401 "invalid admin token"`
   (`admin.rs:53`). Upstream `bearerTokenMiddleware` raises `NotAuthorizedError` for
   missing header or bad token → 401. Status matches; only message text differs.

3. **POST /registry empty entityIds → divergence CONFIRMED.** Ours: empty/absent
   `entityIds` → `unwrap_or_default()` → `200 {failures:[],successes:[]}`
   (`admin.rs:26-30`). Upstream `post-registry.ts:16-24`: size 0 →
   `400 {ok:false,message:"No entity ids provided"}`. **200 vs 400.** Real, minor.

4. **POST /registry valid entityIds → divergence CONFIRMED.** Ours: 200 echoing ids,
   zero persistence. Upstream: 200 only after real catalyst fetch + DB persist with
   genuine per-id failures. Status coincides (200); body fabricated, side effect missing.

### Additional gap not in the original findings
- **422 on malformed body** (crate-level note) applies to the *other*, client-called
  handlers — NOT to these two admin endpoints. `post_registry` takes
  `Option<Json<serde_json::Value>>` (`admin.rs:12`), so a malformed/absent body
  degrades to `None` rather than 422. The flagged-endpoint failure tables are unaffected.

## Crate-level startup/error-model (spot-checked, all CONFIRMED)
- Panic-free startup: `main.rs:11` returns `Result<()>`; `Config::from_env()?` and
  `build_state()?` propagate via `?` → clean non-zero exit, no panic.
- Content DB hard dependency: `config.rs:51-52` `.context()?` errs if
  `POSTGRES_CONTENT_USER` (and `CONTENT_PG_CONNECTION_STRING`) unset; `lib.rs:42-47`
  errs on connect failure. As a 5144 member, content-DB-down blocks bundle boot.
- `AB_REGISTRY_PG_CONNECTION_STRING` optional: `lib.rs:64-69` warns, runs with
  `RegistryStore{pool:None}` (denylist reads empty, spawn overrides off, writes 501).
- Manifest store silent degrade: `manifest_store.rs:55-59` missing platform manifest
  → `None` → `BuildStatus::Pending`, never errors.
- Error model uniform: `http/errors.rs:52-71` → `{ok:false,message}` with fixed status
  map (400/401/403/404/501/500); DB error masked to `"database error"` and logged.

## Bottom line
Both flagged findings are **valid on the committed tree** and not fixed. Correctly
low-severity: admin-only, never client-called, no crash surface. The stub `/registry`
(no fetch/persist, 200-on-empty, 200-on-valid-without-persist) is the substantive one;
`/flush-cache` is a shape-match with a cosmetic message-casing and a benign
cache-namespace difference. The only failure mode both real and worth a one-line
operator note is the **unset-token 401-vs-404** on both endpoints.
