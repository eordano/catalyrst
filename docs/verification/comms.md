# catalyrst-comms (service "comms") — adversarial verification

Upstream: `comms-gatekeeper`. Crate: `crates/catalyrst-comms`.
Verified against the committed tree (branch `feat/service-plane-crates`), upstream TS, and the
unity-explorer C# consumers + net-catalog (`the Unity net-catalog`).

Verdict: all supplied findings are ACCURATE on the committed tree. No false "fixed" claims, no
cosmetic-only items mislabelled as breaking. One additional client-called endpoint that the supplied
findings OMITTED was checked and confirmed shape-safe (GET /users/:address/bans, ban-status).

## Per-endpoint table

| endpoint | shape | client-reaction | severity | failure-modes-ok | notes |
|---|---|---|---|---|---|
| POST /get-scene-adapter | divergent (extra `room`,`identity`; upstream `{adapter}` only) | ok | minor | yes | client-called (GateKeeperSceneRoom.cs:214-218). JsonUtility (`WRJsonParser.Unity`) reads only `adapter`/`fixedAdapter`, ignores extras; our `adapter` always present -> used. Parity gap: skips deny-list, world-access auth, sceneId resolution, presenter sync; consumed shape satisfied. |
| POST /get-server-scene-adapter | divergent (extra `room`,`identity`; identity hardcoded `authoritative-server`) | unknown / not client-called | minor | NO (open-gate) | NOT in net-catalog. Identity gate now real (scene_adapter.rs:93-97) when `AUTHORITATIVE_SERVER_ADDRESS` set; OPEN when unset (boot warn lib.rs:92-96). |
| GET /scene-admin | match (bare array `[{admin,name,canBeRemoved}]`) | ok | minor | NO (1 gap) | client-called (SceneAdmins.cs:132 GET, Newtonsoft, indexes `r.admin`). Bare array matches upstream (list-scene-admins-handler.ts:96-97). `id`/`active` omitted -> Newtonsoft default. Gap: omits land-lease holders + extraAddresses, `canBeRemoved` always true. |
| POST /scene-admin | match (204) | ok (ignores body) | minor | NO (2 gaps) | 204 matches add-scene-admin-handler.ts:112. Client never parses POST body. Skips owner/permission/ban gating + LiveKit metadata sync. |
| DELETE /scene-admin | match (204) | unknown / not client-called | minor | NO (1 gap) | 204 matches remove-scene-admin-handler.ts:89. Not in catalog. |
| GET /scene-bans | match (`{results,total,page,pages,limit}`) | unknown / not client-called | none | NO (1 gap) | Not in catalog. |
| GET /scene-bans/addresses | match (`{results:[addr],...}`) | unknown / not client-called | none | NO (1 gap) | Not in catalog. |
| POST /scene-bans | match (204) | unknown / not client-called | minor | NO (2 gaps) | Not in catalog. Skips place-resolve, permission gate, ban-by-name, kick/event side-effects. |
| DELETE /scene-bans | match (204) | unknown / not client-called | minor | NO (1 gap) | Not in catalog. |
| POST /users/:address/bans | match (201 `{data:UserBan}`) | unknown / not client-called | minor | yes | 201 `{data:ban}` matches ban-player-handler.ts:27-29. Moderator-gated. POST not in catalog. |
| GET /users/:address/bans (ban-status) — NOT in supplied findings | match (`{data:{isBanned,ban}}`) | ok | minor | partial | **client-called** (ModerationDataProvider.cs:23, Newtonsoft -> `GetBanStatusResponse`). Our `{data:{isBanned, ban?}}` (user_bans.rs:35, ports/user_bans.rs:44) matches DTO. Like upstream (ban-status-handler.ts) it has NO moderator gate. DB-down: ours 500 `{ok,message}` vs upstream 500 `{error:"Internal Server Error"}`; `data` null either way; consumer exception-safe. |

## Confirmed issues (all real on committed tree)

1. **Auth-error status divergence on every scene-admin / scene-bans route**: `require_signer`
   failure is mapped to **400 bad_request** (scene_admin.rs:29/64/75, scene_bans.rs:51/84/108/118),
   while upstream throws `UnauthorizedError` -> **401** (logic/utils.ts:23, add-scene-admin-handler.ts:35).
   Status-code + body-key divergence (`{ok,message}` vs `{message}`). The one client-called error path
   (GET /scene-admin) is caught by a catch-all (SceneAdmins.cs:145) and falls back to an empty admin
   set: degrades silently, no crash.

2. **Universal error-body divergence**: every error renders `{ok:false,message}`
   (catalyrst-types/src/error.rs IntoResponse) vs upstream `{message}` / `{error}`. DB errors are
   scrubbed to `"database error"` at 500. Status codes broadly aligned (400/401/403/404/409/503/500).
   No client-parsed route reads the error body -> non-breaking but a real wire diff.

3. **get-server-scene-adapter open gate when `AUTHORITATIVE_SERVER_ADDRESS` unset** (boot warn
   lib.rs:92-96; scene_adapter.rs:93-97 `if let Some(expected)` with no else). Any valid signer mints a
   publish+subscribe token. Prior report's "CLOSED when env configured" is accurate; the residual
   open-in-dev path is real and correctly flagged `ok:false`.

4. **Malformed-JSON body -> 422 plain-text** (axum `Json` extractor rejection, not the `{ok,message}`
   envelope) on POST scene-admin / scene-bans / users-bans. Upstream returns 400 `InvalidRequestError`.
   Real, but none of these POST bodies are parsed back by the Unity client.

5. **Functional parity gaps (non-shape)**, all real, correctly minor: scene-adapter skips deny-list /
   world-access / sceneId-resolution / presenter sync; scene-admin & scene-bans writes skip
   permission/owner/ban gating, ban-by-name, kick + LiveKit metadata/event side-effects;
   GET /scene-admin omits land-lease + extraAddresses so the admin SET is smaller than upstream.

## Client-crash risks

NONE. No flagged endpoint produces a null-deref or unhandled throw in the Unity client.
- get-scene-adapter: JsonUtility tolerant of extra fields; `adapter` always present.
- GET /scene-admin: Newtonsoft tolerant of missing `id`/`active`; `FireRequestAsync` is documented
  exception-free (catch-all SceneAdmins.cs:145), so a 400/500 just yields an empty admin map.
- GET /users/:address/bans (the extra client-called route not in the findings): `GetBanStatusResponse`
  reads only `data`; our shape matches and on failure `data` is null with the caller in a
  guard/notification path — no non-null assertion observed.

## Failure-mode gaps (status / body divergence vs upstream; recoverable, panic-free)

- 400 instead of 401 on missing/invalid auth for ALL scene-admin and scene-bans routes (GET/POST/DELETE).
- 422 plain-text (not `{ok,message}`/400) on malformed JSON body for POST scene-admin / scene-bans / users-bans.
- DB-down -> 500 `{ok:false,message:"database error"}` vs upstream `{error:"Internal Server Error"}` / `{message}`
  (body-key only; status aligned).
- get-server-scene-adapter identity gate OPEN when `AUTHORITATIVE_SERVER_ADDRESS` unset.
- No request-path panic: error model is total, `?`->`Database`->500, `serde_json::to_value(...).unwrap_or(...)`
  fallbacks are safe (user_bans.rs:35/81/127/160/171). Startup hard-requires only `COMMS_PG_CONNECTION_STRING`
  + reachable Postgres + successful migration, else clean `anyhow` exit (no panic). LiveKit/squid/moderator
  config all degrade gracefully.
