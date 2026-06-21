# Parity audit — catalyrst-ab-registry vs `asset-bundle-registry`

Service: **ab-registry** (crate `catalyrst-ab-registry`)
Upstream: `decentraland/asset-bundle-registry`
Method: each flagged finding re-verified by reading both the Rust struct/handler and
the upstream TS shape/handler, and cross-checking the **Unity net-catalog**
(`the Unity net-catalog` + `findings-*.jsonl`) and the
actual Unity deserialization structs in `unity-explorer` to confirm the client
really reads the diverging field. Live diff was N/A (upstream TS not running here);
compared statically.

The single most important gating fact, established from the net-catalog, is which
endpoints the **Unity explorer** actually calls at runtime:

| Endpoint | Client-facing? | Evidence |
|---|---|---|
| POST /entities/active | **YES** (hot path) | scene/wearable/emote resolver; drop-in for catalyst `/content/entities/active` |
| POST /entities/versions | **YES** | `AssetBundleRegistryVersionHelper` → `ABVersionsResponse` |
| POST /profiles | **YES** | full-tier profile fetch, parsed as `Profile` |
| POST /profiles/metadata | **YES** | compact-tier, parsed via `ProfileCompactInfoConverter` |
| GET /worlds/:worldName/manifest | **YES** | `WorldManifestProvider` → `WorldManifestDto` |
| GET /entities/status/:id | no (perf-test only) | only appears in a perf-test finding, not a runtime read |
| GET /entities/status | no | not in catalog |
| GET /queues/status | no | ops/observability only |
| GET /denylist, POST/DELETE /denylist/:id | no | admin/moderation |
| POST /registry, DELETE /flush-cache | no | admin |
| GET /status (service) | no | ops/health |

This gating is what separates the genuinely **client-breaking** issues (the five
client-facing endpoints) from the real-but-internal divergences (everything else).

---

## Per-endpoint table

| Endpoint | Shape | Efficiency | Severity (client impact) | Notes |
|---|---|---|---|---|
| POST /entities/active | divergent (real, client-affecting) | worse (confirmed structural) | **breaks-client** | `versions`/`bundles` flat vs Unity-required `assets`-wrapped; Unity reads `versions.assets.{mac,win}.{version,buildDate}` (active path) and `status` enum. `status` casing actually tolerated; `FAILED` value & versions-shape break it. |
| POST /entities/versions | divergent (real, client-affecting) | worse (confirmed structural) | **breaks-client** | `ABVersionsResponse` reads `versions.assets.{mac,win}.{version,buildDate}`; our flat `versions` yields null → AB version pin lost. `bundles`/`status` present but ignored by client on this route. |
| POST /profiles | divergent (real, client-affecting) | better (structural, with caveat) | **major** | upstream strips to `{timestamp,avatars}` + rewrites snapshots to absolute URLs; ours returns full metadata + `content[]` + raw URN snapshots. |
| POST /profiles/metadata | divergent (real, client-affecting) | better (structural, with caveat) | **major** | `thumbnailUrl`: ours emits raw `face256` URN, not absolute `face.png` URL → broken nameplate thumbnail. `nameColor` always omitted. |
| GET /worlds/:worldName/manifest | shape-compatible; **semantically** divergent | worse (confirmed) | **major** | wire shape matches Unity `WorldManifestDto` exactly. Divergence is semantic: single-scene parcels vs union across all world scenes; spawn fallback differs; no 400 on invalid name. |
| GET /entities/status/:id | divergent (real, not client-facing) | worse | minor | `lods` omitted-when-absent vs upstream always-present-PENDING; no historical-table fallback. Not called by explorer at runtime. |
| GET /entities/status | divergent (real, not client-facing) | worse | minor | active-only vs current+historical merge → fewer rows. Not called by explorer. |
| GET /queues/status | match (shape) | worse | minor | shape identical; population synthesized from a bounded 2000-entity scan + disk reads vs upstream live `jobs:*` counter read. Incomplete past the cap. |
| GET /denylist | divergent (real, not client-facing) | same | minor (was major) | `{ok,data:[string]}` vs bare `DbEntity[]` rows. Admin only. |
| POST /denylist/:id | divergent (real, not client-facing) | same | minor (was major) | 200/`{ok}` vs 201/inserted row; ours lacks moderator allowlist. Admin only. |
| DELETE /denylist/:id | divergent (real, not client-facing) | same | minor | always 200 vs 404-on-miss; ours skips manifest-cache invalidation (POST does it). Admin only. |
| POST /registry | divergent (real, not client-facing) | better (confirmed) | minor (was major) | `{ok,note}` no-op vs `{failures,successes}`. Admin ingest; ours derives state, nothing to persist. |
| DELETE /flush-cache | divergent (real, not client-facing) | same | minor | missing additive `message` field. Admin only. |
| GET /status | divergent (real, not client-facing) | same | minor | flat `{ok,service,version,commitHash}` vs `{data:{version,currentTime,commitHash}}`; missing `currentTime`. Ops only. |

---

## Confirmed shape issues (with proof of client impact)

### 1. POST /entities/active — `versions`/`bundles`/`status` shape (BREAKS CLIENT)
Upstream `Registry.DbEntity` (`src/types/types.ts:44-88`):
`versions = { assets: { windows|mac|webgl: { version, buildDate } } }`,
`bundles = { assets: {...: SimplifiedStatus}, lods?: {...} }`, lowercase `Status`,
plus a `deployer` field.

Ours (`types.rs:42-87`): `versions = { windows?,mac?,webgl?: string }` (flat),
`bundles = { windows,mac,webgl: bool }` (flat), `status` UPPERCASE enum incl.
`FAILED`, no `deployer`, plus extra `content`/`metadata` (which are fine — they are
the base Entity fields the client needs).

**Client proof:** the active response is consumed both as the base Entity
(`SceneEntityDefinition[]` / wearable DTO) *and* for AB fields via
`TrimmedEntityDefinitionBase` (`unity-explorer/.../EntityDefinitionBase.cs:39-43`):
`[JsonProperty("versions")] AssetBundleManifestVersion` and
`[JsonProperty("status")] AssetBundleRegistryEnum`. `AssetBundleManifestVersion`
(`AssetBundleManifestVersion.cs`) reads `versions.assets.{mac,windows}.{version,buildDate}`
via `GetAssetBundleManifestVersion()`. Our flat `versions` has no `assets` →
`assets` is null → `IsEmpty()==true` → scene AB version unresolved (falls back to
raw GLBs). Real break.

**Nuance (rejected sub-claim):** the finding said the `status` casing is "wrong."
Newtonsoft deserializes C# enums by name **case-insensitively** by default, so
`"COMPLETE"/"FALLBACK"/"PENDING"` map fine to `AssetBundleRegistryEnum.{complete,
fallback,pending}`. The genuine status hazard is our extra `FAILED` value, which has
no counterpart in the Unity enum `{complete,fallback,pending}` and would fail enum
parse. Either way the `versions.assets` break alone is sufficient. `deployer` is
absent in ours but the client never reads it on this route, so that sub-diff is real
but client-irrelevant.

### 2. POST /entities/versions — `versions.assets` shape (BREAKS CLIENT)
Upstream returns `{ pointers, versions, bundles, status }` with `versions` in the
`assets`-wrapped `{version,buildDate}` form (`get-entity-versions.ts:30-36`).

**Client proof:** `ABVersionsResponse.cs` reads
`element.versions.assets.mac.version/.buildDate` and `.windows.*`, fed into
`AssetBundleManifestVersion.CreateManualManifest(...)`
(`AssetBundleRegistryVersionHelper.cs:87-97`, consumed in
`LoadTrimmedElementsByIntentionSystem.cs:218`). Our flat `versions` →
`assets`==null → empty version strings → wearable/emote AB version pin lost. Real
break. The `bundles`/`status` fields on this route are present in the struct but
**not read** by the helper — so those sub-diffs are real on the wire but client-inert
here.

### 3. POST /profiles — DTO shape (MAJOR)
Upstream `mapEntitiesToProfiles` (`profile-sanitizer.ts:84-89`) returns ONLY
`{ timestamp, avatars }` and rewrites `avatars[].avatar.snapshots.face256/body` to
absolute `PROFILE_IMAGES_URL/entities/{id}/face.png|body.png`. Ours
(`profiles.rs:52-64`) returns the full stored metadata object + injected
`content:[{key,hash}]` + `timestamp`, with snapshots left as raw content-hash URNs.
Extra fields + raw-URN-vs-absolute-URL divergence. Real.

### 4. POST /profiles/metadata — `thumbnailUrl`/`nameColor` (MAJOR)
Upstream `getMetadata` (`profile-sanitizer.ts:44-53`): `thumbnailUrl` is a REQUIRED
absolute `…/face.png` URL and `nameColor` passes through `avatar.nameColor`. Ours
(`profiles.rs:71-102`) emits the raw `snapshots.face256` URN (optional) and
hardcodes `nameColor: None`.

**Client proof:** `ProfileCompactInfoConverter.ReadJson` reads `thumbnailUrl` →
`FaceSnapshotUrl` and `nameColor` → `ClaimedNameColor/UserNameColor`. The converter
tolerates an empty value (no crash), so the effect is a **degraded/broken thumbnail**
and a missing custom name color (only when the `NAME_COLOR_CHANGE` flag is on), not a
parse failure. Real, user-visible, hence major — not breaks-client.

### 5. GET /worlds/:worldName/manifest — semantic divergence (MAJOR)
Wire shape matches Unity `WorldManifestDto` exactly (`WorldManifest.cs:156-165`):
`occupied: string[]`, `total: int`, `spawn_coordinate: {x:int,y:int}` (snake_case).
The reported "divergent shape" is really **semantic**: upstream
(`coordinates/component.ts:277-310` + `db.getWorldManifestData`) unions DISTINCT
parcels across ALL of a world's COMPLETE/FALLBACK non-denylisted scenes and falls back
to centroid spawn; ours (`worlds.rs`) reads a single matching scene's
`metadata.scene.parcels` and falls back to `scene.base`/origin, and skips
`isWorldNameValid` (404 instead of 400). For multi-scene worlds `occupied`/`total`
and the spawn point can differ. `spawn_coordinate` feeds teleport-to-spawn
(`TeleportToSpawnPointOperationBase.cs:112`) and bounds feed
`IsParcelInsideBoundaries`, so wrong values affect world load/teleport. Major.

### 6–13. Internal endpoints (real divergences, NOT client-facing)
All re-verified as real against upstream source, but the net-catalog shows the Unity
explorer never calls them, so client impact is minor/none:
- **/entities/status/:id**: `lods` omitted-when-absent vs upstream always-PENDING
  (`parseRegistryStatus`); no historical-table fallback. assetBundles lowercase
  matches.
- **/entities/status** (signed): active-only vs current+historical merge.
- **/queues/status**: shape identical; semantics differ (see efficiency).
- **/denylist**: `{ok,data:[string]}` vs bare `DbEntity[]` rows (reason/created_by/
  dates dropped); ORDER BY entity_id vs created_at DESC.
- **POST /denylist/:id**: 200/`{ok}` vs 201/inserted row; ours has no moderator
  allowlist (signed-fetch only) and stores no `reason`.
- **DELETE /denylist/:id**: always 200 vs 404-on-miss; ours does NOT invalidate the
  manifest cache on delete though POST does — a real internal inconsistency.
- **POST /registry**: `{ok,note}` no-op vs `{failures,successes}`.
- **DELETE /flush-cache**: missing additive `message` field (200 in both).
- **GET /status**: flat `{ok,service,version,commitHash}` vs `{data:{version,
  currentTime,commitHash}}`; `currentTime` absent.

---

## Confirmed efficiency findings (structural reasons verified in both impls)

### Worse (real structural reasons)
- **/entities/active and /entities/versions** — Upstream serves a **single** SQL on
  the materialized `registries` table where `status,bundles,versions` are
  pre-computed columns populated out-of-band by sync workers, with the denylist
  inlined as `NOT EXISTS` (verified `db.ts:70-117`). Ours
  (`resolve.rs` → `content.rs` + `manifest_store.rs`) does **~3 SQL** (deployments
  array-overlap + `content_files` hydrate + `denylist_set`) **plus per-entity disk
  reads** (up to 4 `std::fs::read_to_string` per entity: windows/mac/webgl/LOD
  manifests), memoized in a 30s-TTL moka cache, and DERIVES status/bundles/versions
  at request time. More queries + per-item disk/cache fan-out vs one indexed
  materialized read. Structurally worse — confirmed. (Also a behavioral divergence on
  /entities/versions: ours applies the denylist; upstream omits `excludeDenylisted`
  for that route and returns denylisted pointers.)
- **/entities/status/:id, /entities/status, /queues/status** — same structural class:
  upstream reads pre-materialized columns (and for /queues/status, live `jobs:*`
  counters in `memoryStorage`), no disk; ours derives status from per-entity disk/moka
  manifest reads. For /entities/status it's an N-iteration per-id disk fan-out vs 2
  parallel indexed queries; for /queues/status it's a bounded 2000-entity scan + N
  disk reads vs an O(1)-ish counter read (and incomplete beyond the 2000 cap).
- **/worlds/:worldName/manifest** — upstream 1 txn / 2 parallel SQL over the
  aggregated registries; ours `resolve_pointers` (2 SQL) + in-process scan + a
  `world_spawn` lookup (~3 SQL) plus per-request JSON metadata parse of a single
  entity. Confirmed worse.

### Better (structural, with a stated caveat)
- **/profiles and /profiles/metadata** — Ours is `resolve_profiles` (1 SQL,
  deployments overlap on `entity_type='profile'`) + `content_files` hydrate (1 SQL),
  reading the authoritative shared content DB directly, **no external hop**. Upstream
  (`profile-retriever.ts:75-130`) is a genuine 3-layer retriever: L1 in-memory cache
  → L2 PG `profiles` table → **L3 fan-out HTTP to Catalyst lambdas** `getProfiles` +
  persist on miss. Structurally fewer moving parts and no cross-service network
  fallback — confirmed *structural* (not language-based). **Caveat:** ours has no
  cache, so against a warm upstream LRU the steady-state read may be comparable; the
  win is on the cold/miss path and on having no out-of-band sync. Also the metadata
  path pays an **unused `content_files` hydrate** (compact path never reads content) —
  a minor self-inflicted inefficiency worth fixing.
- **/registry** — Intentional no-op (`admin.rs:13-23`): registry state is derived, so
  zero data access. Upstream is a heavy admin write per entityId: catalyst
  `getEntityById` + ~6 parallel manifest HTTP fetches + a persist transaction
  (`post-registry.ts`). Ours structurally avoids all of it — confirmed better. Marked
  minor only because the endpoint is admin/internal.

### Same
- /denylist, POST/DELETE /denylist/:id, /flush-cache, /status — single cheap
  SQL/flush/static read on both sides; differences are which cache/table is touched,
  not cost class. Confirmed.

---

## Rejected / corrected during verification

- **"/entities/active status casing is wrong → breaks client."** Partially rejected.
  Newtonsoft maps enum names case-insensitively, so `COMPLETE/FALLBACK/PENDING`
  deserialize fine to the lowercase `AssetBundleRegistryEnum`. The real status hazard
  is the extra `FAILED` value (no Unity counterpart), not casing. The endpoint still
  breaks the client — via the `versions.assets` shape — so the verdict stands, but the
  specific casing rationale is corrected.
- **"/entities/active `deployer` missing → client impact."** Rejected as
  client-affecting. The active response is read as the base Entity + AB `versions`/
  `status`; no Unity struct reads `deployer`. Real wire diff, zero client impact.
- **"/entities/versions `bundles`/`status` shape → breaks client."** Rejected for this
  route. `AssetBundleRegistryVersionHelper` only reads `versions.assets.*`; it never
  reads `bundles` or `status` from the versions response. Those sub-diffs are real on
  the wire but client-inert on /entities/versions. Only `versions.assets` breaks it.
- **"/worlds/:worldName/manifest shape divergent."** Re-characterized. The JSON wire
  shape actually MATCHES Unity `WorldManifestDto` (occupied/total/spawn_coordinate with
  snake_case and numeric x/y). The divergence is purely semantic (parcel-set
  composition + spawn fallback + missing 400 validation), not a deserialization break.
- **Severity inflation on admin/ops endpoints (/denylist GET/POST/DELETE, /registry).**
  The findings rated several "major." Net-catalog confirms none are called by the Unity
  explorer; the divergences are real but not client-facing, so client-impact severity
  is reduced to minor. (They still matter for any non-Unity admin/moderation tooling
  that expects upstream's bare-row shapes and moderator allowlist.)
- **"/profiles efficiency is better" — accepted but caveated, not rejected.** The win
  is structural (no L3 network fallback, no out-of-band sync table), but it is NOT
  unconditional: ours has no cache layer, so it is not strictly faster than a warm
  upstream LRU. Recorded as better-with-caveat, and flagged the unused content_files
  hydrate on the metadata path.
