# Verification — catalyrst-server (service "content", bundle 5141 / stub 5140)

Adversarial re-check of the four flagged endpoints plus the crate-level
startup/error-model claims. Verified against the committed tree on
`feat/service-plane-crates`, upstream catalyst (`content-server` middlewares +
`lamb2` handlers), and the Unity consumer (`unity-explorer`).

Bottom line: the only **real, production-relevant** defect is the `/about`
comms-health coupling. Almost every other flagged "divergence" is either a
stub-only artifact (the 5140 `main.rs` test harness, not the live 5141 binary)
or an *exact port* of upstream that the finding misread as divergent.

## Per-endpoint table

| endpoint | shape | client-reaction | severity | failure-modes-ok | notes |
|---|---|---|---|---|---|
| `GET /about` | matches upstream lamb2 ADR-250 shape | request-throws on 503 (real) | **breaks-client** (comms path only) | NO (comms gating) / stub-only (Synced) | Real bug: `healthy = content_healthy && comms_healthy` always, comms never optional. Upstream makes comms OPTIONAL. 503 here aborts realm change. |
| `POST /lambdas/profiles` | **convergent** (bare `Profile[]`, identical to lamb2 `profilesHandler`) | ok | none/minor | YES (one nit: 304 body) | Finding's "divergent" verdict is wrong; bare array matches upstream. 304 returns `Json(null)` vs upstream empty body — cosmetic. |
| `GET /lambdas/profiles/{id}` | **convergent** (single metadata obj; 404 on miss) | ok (404 explicitly caught) | none | YES | Identical to lamb2 `profileHandler` (404 `NotFoundError('Profile not found')`). Client catches `NOT_FOUND` -> returns null. |
| `GET /lambdas/profile/{id}` | **convergent** (exact port; 200 `{avatars:[],timestamp:0}` on miss) | n/a (not client-called) | none | YES | Byte-identical to lamb2 `profileAliasHandler`; upstream's own comment documents the 200+stub contract. Finding labeled it "divergent default" — it is a faithful port. |

Endpoints total in router: 46 `.route(...)` calls (`/about`, content set mounted
at root + under `/content`, full lambdas set, `/metrics`).

## Confirmed issues

### 1. `/about` couples realm health to the comms probe (REAL, breaks-client)
`crates/catalyrst-server/src/handlers/about.rs:307-322`:
```rust
fn content_is_healthy(sync_state: &str) -> bool { sync_state == "Syncing" }
...
let healthy = content_healthy && comms_healthy;   // comms ALWAYS gates
...
comms: Some(comms),                               // comms ALWAYS present
```
Upstream `lamb2/src/controllers/handlers/about-handler.ts:73-93` does:
```ts
let healthy = contentStatus.healthy && lambdasStatus.healthy
let comms = undefined
if (archipelagoPublicUrl) {           // comms is OPTIONAL
  ...
  healthy = healthy && archipelagoStatus.healthy
  comms = {...}
}
```
So upstream, with no `ARCHIPELAGO_URL` configured, returns **200 with no comms
field** even when no comms backend exists. Our crate unconditionally probes
`COMMS_WS_CONNECTOR_URL` (default `http://127.0.0.1:5001/status`) and
`COMMS_STATS_URL` (default `.../core-status`); if either is down/unreachable
`probe_comms()` -> `healthy=false` -> overall `healthy=false` -> **HTTP 503 with
full About body**. There is no env gate to make comms optional.

Client impact is real and confirmed:
- `unity-explorer/.../RealmController.cs:148-149` fetches `{realm}/about` via
  `webRequestController.GetAsync(...).OverwriteFromJsonAsync(...)`. A 503 sets
  UnityWebRequest `result == ProtocolError`, throwing `UnityWebRequestException`
  in `WebRequestController.SendAsync` (`WebRequestController.cs:89-114`); it is
  not in `ShouldIgnoreResponseError`, is re-thrown after the retry policy, and
  is caught by `RealmController.SetRealmAsync`'s `catch (Exception e)`
  (line 196) which rethrows `RealmChangeException`. The realm change aborts.

This is the genuine production hazard: a single down/unconfigured comms sidecar
takes down realm entry, where upstream would degrade to comms-less 200.

## Client-crash risks

None that null-crash the C# converters. Verified:
- `ServerAbout.Clear()` (`NetworkDefinitions/ServerAbout.cs:27-42`) pre-seeds
  `realmName=""`, `localSceneParcels=[]` empty list, `skybox.fixedHour=-1`,
  `comms=null`, then `FromJsonOverwrite` only overwrites present fields. Our
  omission of `configurations.localSceneParcels` and `configurations.skybox`
  therefore renders genesis defaults — no NRE.
- `realmName.EnsureNotNull(...)` (RealmController.cs:161) passes on `""` (Clear
  sets empty string, not null), and we emit `realmName` when set.
- `ResolveCommsAdapter` (RealmController.cs:434-441) is null-safe:
  `about.comms?.adapter ?? about.comms?.fixedAdapter ?? "offline:offline"`, so a
  null/omitted comms is tolerated. Our 200-path always emits comms anyway.
- `ProfileConverter.ReadJson` (`Profiles/SharedAPI/ProfileConverter.cs:55-81`)
  handles both `{metadata:{avatars}}` and `{avatars}`, and throws
  `ArgumentException` only when `avatars` is missing/not an array. Our
  `ensure_profile_shape` (`profile_processing.rs:299-309`) guarantees
  `avatars:[]` is always present, so no throw.
- 404 on `GET /lambdas/profiles/{id}` is explicitly caught:
  `RealmProfileRepository.ExecuteSingleGetAsync` line 373,
  `catch (UnityWebRequestException e) when (e is { ResponseCode: NOT_FOUND })`
  -> `ReportProfileNotFound` -> `return null`. Not a crash.

## Failure-mode gaps

1. **`/about` comms-down -> 503 (CONFIRMED gap, breaks-client).** See issue #1.
   The finding's `ok:false` is correct; this is the one true divergence in the
   `/about` failure set.

2. **`/about` "sync state != 'Syncing' -> 503" is a STUB-ONLY artifact, NOT a
   production gap.** The finding cites the stub `main.rs`
   (`StubSynchronizationState::get_state()=="Synced"`, `main.rs:141-146`) — that
   is the 5140 test harness. The LIVE 5141 binary (`bin/live.rs:1604-1612`)
   maps the real `catalyrst_sync::SyncState` enum, which has **only**
   `Bootstrapping | PartiallySynced | Syncing` variants
   (`catalyrst-sync/src/lib.rs:91-94`) — there is **no `Synced` variant**.
   `PartiallySynced` and `Syncing` both map to the string `"Syncing"`, and
   `None`/uninitialised also maps to `"Syncing"`. So in production
   `content_is_healthy` returns true once past bootstrapping, and the 503 the
   finding describes cannot occur from "Synced". Upstream's identical
   `synchronizationStatus === 'Syncing'` gate confirms the gate itself is
   correct; the stub merely feeds it a string the live enum never produces.
   Reclassify: stub-only, non-issue for the shipped service.

3. **`POST /lambdas/profiles` 304 body divergence (cosmetic).** Our handler
   returns `(NOT_MODIFIED, Json(Value::Null))` (`lambdas.rs:155`); upstream
   returns `{status:304}` with no body (`profiles-handler.ts:33-37`). A `null`
   JSON body on a 304 is harmless (304 bodies are ignored by HTTP clients).
   Cosmetic.

## Rejected findings

- **`POST /lambdas/profiles` shape_verdict "divergent" — REJECTED.** Upstream
  `profilesHandler` returns a bare `Profile[]` (`profiles-handler.ts:39-42`),
  same as ours. No wrapper divergence. Downgrade to convergent.
- **`GET /lambdas/profiles/{id}` "divergent" — REJECTED.** Identical 404
  contract to lamb2 `profileHandler` (`profiles-handler.ts:45-57`). Convergent.
- **`GET /lambdas/profile/{id}` "divergent default" — REJECTED.** Exact port of
  lamb2 `profileAliasHandler` (`profiles-handler.ts:65-73`): 200 +
  `{avatars:[],timestamp:0}` on miss; upstream's own source comment documents
  this as the intended contract. Not client-called per net-catalog (client uses
  plural `/profiles/{id}`). Convergent and harmless.
- **Crate-level error_model "AppError shape-divergent from upstream (missing
  `message`)" — REJECTED.** The two parallel conventions mirror the two distinct
  upstream services this binary fuses:
  - Content endpoints use `AppError` -> `{error:<text>}` with no `message`.
    This is **byte-identical** to the catalyst **content-server** handler
    (`catalyst/content/src/controllers/middlewares.ts:34-48`:
    `{ error: error.message }`, 500 -> `{ error: 'Internal Server Error' }`).
  - Lambdas profile endpoints use `bad_request`/`not_found`/
    `internal_server_error` -> `{error:"Bad request",message}` /
    `{error:"Not Found",message}` / `{error:"Internal Server Error"}`. This is
    **byte-identical** to the **lamb2** handler
    (`lamb2/src/controllers/handlers/errorHandler.ts:6-48`).
  Both conventions match their respective upstreams exactly. Not a divergence.
- **Startup "Panic-free under stub main.rs" — CONFIRMED ACCURATE (retained).**
  Stub wires all `Stub*` impls + `squid_pool: None`; only `expect`/panic sites
  are HTTP_SERVER_PORT parse, TcpListener bind, `axum::serve` — standard
  fail-fast on misconfig. Live binary surfaces DB/squid failures per-request
  (`.unwrap_or_default()` on profile/item paths -> `[]`; `AppError::Internal` ->
  500 on content reads), not at boot.

## Summary

Of the four endpoint findings, three (`POST /lambdas/profiles`,
`GET /lambdas/profiles/{id}`, `GET /lambdas/profile/{id}`) are mislabeled as
"divergent" — they are faithful, byte-level ports of upstream lamb2 and are
convergent. The crate-level "AppError is shape-divergent" claim is also rejected:
the two error conventions correctly mirror content-server vs lamb2 upstreams.

The single confirmed, production-relevant defect is `/about`: it unconditionally
emits and health-gates on a comms probe, so a down/unconfigured comms sidecar
yields HTTP 503, which `RealmController` rethrows as `RealmChangeException`,
breaking realm entry — whereas upstream treats comms as optional and would
return 200. The "Synced -> 503" failure mode the finding flagged is a
5140-stub-only artifact (the live `SyncState` enum has no `Synced` variant) and
does not affect the shipped 5141 service.
