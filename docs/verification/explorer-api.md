# Verification — catalyrst-explorer-api (service "explorer-api")

Branch: `feat/service-plane-crates` (HEAD `00da66e`). Analyzed from committed tree only; nothing
running. Crate: `crates/catalyrst-explorer-api`.

Adversarial re-check of four flagged endpoints. For each I opened (a) the current Rust handler,
(b) the upstream TS shape (`realm-provider`, `auth-api`/auth-server), and (c) the Unity C# consumer +
DTO, and cross-referenced the Unity net-catalog (`the Unity net-catalog`)
to confirm whether the route is actually on the client path.

## Per-endpoint table

| endpoint | shape | client-reaction | severity | failure-modes-ok | notes |
|---|---|---|---|---|---|
| GET /main/about (alias /about) | divergent (CONFIRMED) | ok (CONFIRMED) | minor | yes | Synthetic About; omits `configurations.localSceneParcels` + `configurations.skybox`; emits harmless extra `comms.fixedAdapter`; `commitHash:""`. Client-tolerated — `ServerAbout.Clear()` resets those before `OverwriteFromJsonAsync`. On client load path. |
| GET /hot-scenes | divergent (CONFIRMED `[]`) | MISATTRIBUTED — not on explorer-api client path | downgraded none | n/a for explorer-api | Unity fetches hot-scenes from `archipelago-ea-stats/hot-scenes` (`DecentralandUrl.ArchipelagoHotScenes`), served by `catalyrst-archipelago` (populated), NOT this stub. explorer-api `/hot-scenes` is a vestigial duplicate the client never hits. |
| GET /status | divergent superset (CONFIRMED) | ok (CONFIRMED) | none | yes | Upstream returns only `{version,currentTime,commitHash}`; we add `healthy,name,lastUpdate,env`. Unity only calls `archipelago-ea-stats/status` + `comms-gatekeeper/status` — not realm-provider/explorer-api `/status`. Not on client path. |
| POST /auth/requests/{id} (legacy alias) | divergent / no spec counterpart (CONFIRMED) | unknown (not Unity-HTTP-called) | none | yes | HTTP alias of v2 outcome; `submit_outcome`. 400 empty sender, 400 already-responded, 422 malformed JSON (axum Json extractor). Unity hits only `GET auth-api/identities/{token}` + browser-opened `/auth/requests`, never `POST /auth/requests/{id}`. |

## Confirmed issues

1. **GET /main/about — synthetic About omits `localSceneParcels` + `skybox`** (severity minor, client OK).
   - Real on committed tree: `realm_provider.rs:61-84` builds `configurations` with `networkId,
     globalScenesUrn, scenesUrn, realmName, map{...}` and no `localSceneParcels`/`skybox`.
   - Upstream `about-main-handler.ts` spreads a *real* proxied `catalystAbout.configurations` (which
     carries both fields). So the omission is a genuine divergence vs upstream.
   - Client tolerates it: `RealmController.SetRealmAsync` calls `serverAbout.Clear()`
     (RealmController.cs:146) BEFORE `OverwriteFromJsonAsync` (line 149). `Clear()`
     (ServerAbout.cs:27-42) sets `localSceneParcels.Clear()` and `skybox = {fixedHour:-1}`. Absent
     JSON fields keep those cleared values. `ParseLocalSceneParcels` returns empty on `Count==0`
     (RealmController.cs:351-354); `skybox.fixedHour:-1` → `skyboxFixedHour=null` (line 155-157).
   - `realmName`: we ALWAYS emit it, so the hard `EnsureNotNull("Realm name not found")`
     (RealmController.cs:161) never trips. CONFIRMED.
   - `comms`: we always emit non-null comms with `protocol:"v3"`. Client is null-safe regardless
     (`result.comms?.protocol ?? "v3"` line 164; `ResolveCommsAdapter` uses
     `about.comms?.adapter ?? about.comms?.fixedAdapter ?? "offline:offline"` line 440).
   - Extra `comms.fixedAdapter`: confirmed harmless — `CommsInfo` (CommsInfo.cs) has `fixedAdapter`;
     upstream omits it but client reads it.
   - Behavioral nuance (not a crash, not in original finding): because we always send non-null
     `comms`, `ResolveHostname` (RealmController.cs:414-432) takes the `new Uri(realm.Value).Host`
     branch for non-ENS realms instead of the "main realm shares comms" branch. Cosmetic for a local
     single-realm node.
   - Failure modes accurate: always 200 healthy:true (upstream 503 if no healthy catalysts — ok:true);
     `?catalyst=` derives publicUrl from supplied host (ok:true).

2. **GET /status — superset** (severity none). Real divergence confirmed
   (`realm_provider.rs:121-135` vs `status-handler.ts:13-19`). Extra fields harmless; route not on
   client path (catalog shows only `archipelago-ea-stats/status` + `comms-gatekeeper/status`). Accept.

3. **POST /auth/requests/{id} legacy alias** (severity none). Error paths confirmed exactly:
   empty sender → 400 (`auth_api.rs:371-373`); already Signed → 400 "already has a response"
   (`auth_api.rs:354-359`); malformed JSON → 422 plain text from axum `Json` extractor before
   handler runs (route `auth_api.rs:191`). Not a Unity-HTTP route. Accept as-is.

## REJECTED / RE-CLASSIFIED finding

- **GET /hot-scenes "request-throws" client crash** — REJECTED as attributed to explorer-api.
  Shape claim (`[]` always) is true on the committed tree (`realm_provider.rs:117-119`), and the C#
  claim (`GoToChatCommand.FindCrowdAsync` does `hotScenes[0]` at GoToChatCommand.cs:88 with no
  empty-check, on `/gotocrowd`) is literally true. BUT the Unity client routes hot-scenes to
  `DecentralandUrl.ArchipelagoHotScenes` = `https://archipelago-ea-stats.decentraland.{ENV}/hot-scenes`
  (DecentralandUrlsSource.cs:199), served in this deployment by **`catalyrst-archipelago`**
  (`crates/catalyrst-archipelago/src/handlers.rs:283`), returning a populated `Vec<HotSceneInfo>`.
  The explorer-api `/hot-scenes` route is a vestigial duplicate the client never calls. So the crash
  is NOT an explorer-api client-crash risk. (The latent `hotScenes[0]` IndexOutOfRange does exist in
  the client and would also fire if archipelago returns `[]` — e.g. zero peers online — but that
  belongs to the catalyrst-archipelago lane, not explorer-api.)

## Client-crash risks (explorer-api specifically)

- NONE confirmed. The one flagged crash (`/hot-scenes` → `hotScenes[0]`) does not route through
  explorer-api. The /about consumer is fully null/absent-tolerant via `ServerAbout.Clear()`, and
  `realmName` (the only hard non-null assertion in that path) is always emitted.

## Failure-mode gaps

- /main/about always returns `healthy:true`/`acceptingUsers:true`; upstream 503
  (`ServiceUnavailableError`) when no healthy catalysts. Documented divergence; for a single local
  realm node "always healthy" is the intended degrade and the client treats 200 About as success. No
  client breakage (finding ok:true — confirmed).
- Pass-through proxies (builder_api, worlds_content_server) synthesize
  `502 {"error":"upstream_unavailable","url"}` on transport failure, leaking the internal upstream URL
  in the body. Outside the four flagged endpoints; info leak, not a client crash, not Unity-called.

## Summary

3 of 4 findings confirmed accurate on the committed tree (about shape divergence + client-tolerance,
status superset, auth legacy-alias error model). The `/hot-scenes` finding is rejected as an
explorer-api client-crash: the route returns `[]` but the Unity client fetches hot-scenes from the
archipelago plane, not explorer-api, so there is no explorer-api crash risk. No client-crash risks
for explorer-api. Crate startup is panic-free/DB-free as described.
