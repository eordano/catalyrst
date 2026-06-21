# E2E test plan — catalyrst-ab-registry (key=ab-registry)

Native Rust reimplementation of `asset-bundle-registry.decentraland.org`.

- Crate: `catalyrst-ab-registry`
- Port: **5144** (base URL `http://127.0.0.1:5144`)
- Workspace: `<WORKSPACE>`
- Env: the service's environment file (`<ENV_FILE>`)
- Data sources: shared `content` DB (PostgreSQL instance, unix socket) for active-entity / profile
  resolution; abgen on-disk manifests at `{ABGEN_OUT_ROOT}/{entityId}/{platform}.manifest.json`
  for derived AB build status/versions/bundles; optional `ab_registry` DB for denylist + spawn coords.
  No Redis / S3 / SQS.

---

## 1. Unity config — how to repoint this host

The Asset Bundle Registry host is **NOT realm/`/about`-discovered**. In unity-explorer it
is a hardcoded URL emitted by the `RawUrl(...)` switch in
`Explorer/Assets/DCL/NetworkDefinitions/Browser/DecentralandUrlsSource.cs`. To point the
client at our local service you must edit Unity (editing our `/about` response does nothing).

Exact lines (in `RawUrl(DecentralandUrl)`):

```
209:  DecentralandUrl.AssetBundleRegistry        => $"https://asset-bundle-registry.decentraland.{ENV}",
210:  DecentralandUrl.AssetBundleRegistryVersion => $"{Url(DecentralandUrl.AssetBundleRegistry)}/entities/versions",
```

**Repoint:** change line 209 only — line 210 (and the `Profiles`, `ProfilesMetadata`,
`EntitiesActive`, `EntitiesActiveElements`, `WorldEntitiesActive` arms) are all derived from
`Url(DecentralandUrl.AssetBundleRegistry)`, so they follow automatically.

```csharp
DecentralandUrl.AssetBundleRegistry => "http://127.0.0.1:5144",
```

Notes / caveats:
- `{ENV}` is the `ENV` const (`{ENV}`) substituted elsewhere; for a hard override just drop
  it and use a literal base URL with no trailing slash (the derived arms append their own paths).
- `EntitiesActive` / `WorldEntitiesActive` only route to the AB-registry when the
  `ASSET_BUNDLE_FALLBACK` feature flag is enabled **and** launch mode != LocalSceneDevelopment;
  otherwise they fall back to the realm's `realmData.Ipfs.EntitiesActiveEndpoint`. So to exercise
  the registry's `/entities/active` from the client, that FF must be on. `EntitiesActiveElements`
  (wearables/emotes) always goes to the registry.
- These URL arms are cached (`CacheBehaviour.STATIC` by default), so the edit is picked up on
  next process start; no realm-change invalidation applies to `AssetBundleRegistry` itself.

Summary string (also returned as `unity_config`):
`DecentralandUrlsSource.cs:209 RawUrl() arm DecentralandUrl.AssetBundleRegistry => "http://127.0.0.1:5144" (line 210 AssetBundleRegistryVersion + Profiles/ProfilesMetadata/EntitiesActive*/WorldEntitiesActive derive from it; NOT /about-discovered — edit Unity, not the realm /about)`

---

## 2. Bring-up

```bash
# build (an FHS-compatible shell may be required for cargo on some distros, e.g. NixOS)
cargo check -p catalyrst-ab-registry

# run (from the workspace); the content DB password is sourced from the service's environment file at launch
cargo run -p catalyrst-ab-registry &
# or once wired into a service manager:  systemctl --user start catalyrst-ab-registry.service
```

---

## 3. Concrete e2e curl/wscat checks

All against `http://127.0.0.1:5144`. Replace `<ENTITY_ID>` with a real `bafkrei…` / `Qm…`
deployment id from the content DB (see helper query below).

### 3.0 Pick a live entity id for the parameterized checks
```bash
PGPASSWORD=<DB_PASSWORD> \
psql -h <SOCKET_DIR> -p 5433 -U <DB_USER> content -At -c \
  "select entity_id from deployments where entity_type='scene' and deleter_deployment is null limit 1;"
```

### 3.1 Service status — `GET /status`
```bash
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5144/status
# Expect: 200. Body JSON with service/version/health fields (e.g. {"version":...,"commitHash":...}).
```

### 3.2 Queues status — `GET /queues/status`
```bash
curl -s http://127.0.0.1:5144/queues/status | jq .
# Expect: 200, JSON object of queue names -> counts. Since state is derived (no SQS),
# expect empty/zeroed queues, never a 5xx.
```

### 3.3 Active entities by pointer — `POST /entities/active`
```bash
curl -s -X POST http://127.0.0.1:5144/entities/active \
  -H 'content-type: application/json' \
  -d '{"pointers":["0,0"]}' | jq '. | length, .[0].id'
# Expect: 200, JSON array. Each element has id, type, pointers, content[], metadata, and the
# AB fields (assetBundles/versions/status). Only COMPLETE|FALLBACK entities returned; denylisted excluded.
# Empty pointer with no deployment -> 200 with [].
```

### 3.4 World-filtered active entities — `POST /entities/active` (world)
```bash
# Mirrors the WorldEntitiesActive arm: ?world_name=
curl -s -X POST 'http://127.0.0.1:5144/entities/active?world_name=my-world.dcl.eth' \
  -H 'content-type: application/json' -d '{"pointers":["0,0"]}' | jq 'length'
# Expect: 200, array filtered by metadata.worldConfiguration.name == world_name.
```

### 3.5 Entity versions — `POST /entities/versions`
```bash
curl -s -X POST http://127.0.0.1:5144/entities/versions \
  -H 'content-type: application/json' \
  -d '{"pointers":["0,0"]}' | jq .
# Expect: 200, array of {entityId, versions/{windows,mac}} derived from abgen manifests.
# Only COMPLETE|FALLBACK; denylist excluded.
```

### 3.6 Single entity status (public) — `GET /entities/status/:id`
```bash
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5144/entities/status/<ENTITY_ID>
# Expect: 200 with derived status (COMPLETE/FALLBACK/FAILED/PENDING) for a known id;
# 404 for an unknown id.
```

### 3.7 Entities status (signed-fetch) — `GET /entities/status`
```bash
# Unauthenticated -> 401 with ADR-44 message.
curl -s -w '\n%{http_code}\n' http://127.0.0.1:5144/entities/status
# Expect: 401, body mentions "signed fetch request" / "ADR-44".
# Authenticated path: produce a real AuthChain via dcl-walk auth-sign (see §4) and send the
# signed-fetch headers (x-identity-auth-chain-*, x-identity-timestamp, x-identity-metadata).
# A signer of "decentraland-kernel-scene" must be REJECTED (metadataValidator parity).
```

### 3.8 Profiles — `POST /profiles`
```bash
curl -s -X POST http://127.0.0.1:5144/profiles \
  -H 'content-type: application/json' \
  -d '{"ids":["0x0000000000000000000000000000000000000001"]}' | jq 'length'
# Expect: 200, array of profile entities (entity_type='profile') resolved from content DB.
# Unknown address -> 200 with [].
```

### 3.9 Profiles metadata — `POST /profiles/metadata`
```bash
curl -s -X POST http://127.0.0.1:5144/profiles/metadata \
  -H 'content-type: application/json' \
  -d '{"ids":["0x0000000000000000000000000000000000000001"]}' | jq .
# Expect: 200, metadata-only variant of /profiles (no full content listing).
```

### 3.10 World manifest — `GET /worlds/:worldName/manifest`
```bash
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5144/worlds/my-world.dcl.eth/manifest
# Expect: 200 for a deployed world (built from metadata.scene.parcels + base, with optional
# user-set spawn override from world_spawn_coordinates); 404 for an unknown world.
```

### 3.11 Denylist read — `GET /denylist`
```bash
curl -s http://127.0.0.1:5144/denylist | jq .
# Expect: 200, JSON array of entity ids. Empty [] when ab_registry DB is unset/empty.
```

### 3.12 Denylist add/remove (signed-fetch) — `POST` / `DELETE /denylist/:entityId`
```bash
# Unauthenticated -> 401 (ADR-44).
curl -s -o /dev/null -w '%{http_code}\n' -X POST   http://127.0.0.1:5144/denylist/<ENTITY_ID>   # 401
curl -s -o /dev/null -w '%{http_code}\n' -X DELETE http://127.0.0.1:5144/denylist/<ENTITY_ID>   # 401
# With a valid AuthChain (non-scene signer) and AB_REGISTRY_PG_CONNECTION_STRING set:
#   POST   -> 200/204, id appears in GET /denylist and is excluded from /entities/active|versions
#   DELETE -> 200/204, id removed from GET /denylist
# If ab_registry DB is unset, expect a clear error (writes have nowhere to persist) — verify it is
# not a 500 panic.
```

### 3.13 Admin: create registry (bearer) — `POST /registry`
```bash
# No/invalid token -> 401. Route only mounted when API_ADMIN_TOKEN is set.
curl -s -o /dev/null -w '%{http_code}\n' -X POST http://127.0.0.1:5144/registry           # 401
curl -s -o /dev/null -w '%{http_code}\n' -X POST http://127.0.0.1:5144/registry \
  -H "Authorization: Bearer $API_ADMIN_TOKEN" -H 'content-type: application/json' -d '{}'  # 200 (no-op; status is derived)
```

### 3.14 Admin: flush cache (bearer) — `DELETE /flush-cache`
```bash
curl -s -o /dev/null -w '%{http_code}\n' -X DELETE http://127.0.0.1:5144/flush-cache       # 401 without token
curl -s -o /dev/null -w '%{http_code}\n' -X DELETE http://127.0.0.1:5144/flush-cache \
  -H "Authorization: Bearer $API_ADMIN_TOKEN"                                              # 200; clears moka manifest cache
```

### 3.15 Parity spot-check vs upstream (optional)
For a handful of ids, diff our response shape against production
`https://asset-bundle-registry.decentraland.org` for `/entities/active`, `/entities/versions`,
and `/entities/status/:id` (field names, status enum values COMPLETE/FALLBACK/FAILED/PENDING,
`content[]` {file,hash} shape). Expect structurally identical JSON.

---

## 4. Real-client smoke (dcl-bevy / dcl-walk)

The reference client for the AB-registry is **unity-explorer** (the enum lives there), so the
authoritative smoke is via `dcl-walk` (drives the upstream Unity client). dcl-bevy does not
consume this host.

1. Apply the Unity repoint from §1 (line 209 -> `http://127.0.0.1:5144`) in a unity-explorer
   workspace, and ensure the `ASSET_BUNDLE_FALLBACK` feature flag is enabled if you want to
   exercise `/entities/active` from scene loading (wearables/emotes go there regardless).
2. Launch + auth:
   ```bash
   dcl-walk launch
   dcl-walk auth-sign          # produces the AuthChain used for §3.7 / §3.12 signed-fetch checks
   ```
3. Teleport to a parcel known to have abgen-built bundles (a COMPLETE entity) and confirm the
   scene loads asset bundles rather than raw GLBs. Watch the service log for
   `POST /entities/active`, `POST /entities/versions`, and `/profiles` hits originating from the
   client.
4. Confirm avatars render (exercises `Profiles` / `ProfilesMetadata`, which also derive from the
   `AssetBundleRegistry` base URL) — i.e. nearby/own avatar wearables resolve through 5144.
5. Capture a screenshot (`dcl-walk` shot / `dcl-rig shot`) showing the scene + avatar loaded, and
   grep the service log to assert no 5xx during the session.

Pass criteria: client renders an AB-built scene and avatars with all registry traffic served by
127.0.0.1:5144, no 5xx in the service log, and the signed-fetch routes correctly 401 unauthenticated
requests / reject the `decentraland-kernel-scene` signer.
