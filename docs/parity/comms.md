# Parity report — catalyrst-comms (service "comms" / upstream comms-gatekeeper)

Adversarial verification of the flagged parity findings. Rust crate:
`crates/catalyrst-comms`. Upstream TS:
`decentraland/comms-gatekeeper`. Cross-checked against the
Unity net-catalog (`the Unity net-catalog` + findings-*.jsonl)
to determine which divergences actually reach the explorer.

## Method note on severity

The original findings tagged several GET endpoints `breaks-client`. The Unity
net-catalog shows the explorer only calls a *subset* of these routes:

- **Unity-called comms-gatekeeper routes:** `POST /get-scene-adapter`
  (reads only `.adapter`), `GET /scene-admin` (parsed as `List<AdminInfo>`),
  `GET /users/{address}/bans` (parsed as `GetBanStatusResponse{data:{isBanned,ban}}`),
  `POST /scene-admin`. Also `/ping`, `/status` (URL constants, no body parsed).
- **NOT Unity-called** (admin-UI / service-to-service / LiveKit-to-gatekeeper):
  `GET/DELETE /scene-bans`, `GET /scene-bans/addresses`, `GET /scene-participants`,
  `POST/DELETE /scene-bans`, `POST /livekit-webhook`, `POST /get-server-scene-adapter`,
  `DELETE /scene-admin`.

So `breaks-client` is only justified where the explorer actually deserializes the
shape. For non-Unity routes a real shape divergence is downgraded to `major`
(wrong for *some* consumer, but does not break the explorer client this lane targets).

## Per-endpoint table

| Endpoint | Shape | Efficiency | Severity | Notes |
|---|---|---|---|---|
| GET /status | divergent (confirmed) | same | minor | `{healthy,version,livekit_configured,livekit_host}` vs `{version,currentTime,commitHash}`. Only `version` overlaps. But `/status` is a URL constant in Unity (no body parsed) — cosmetic for the explorer. |
| POST /get-scene-adapter | divergent (cosmetic for client) | better (confirmed, with parity gap) | minor | Extra `room`/`identity`; client reads only `.adapter` (Newtonsoft tolerates extra fields). Efficiency win real but is a parity gap (skips deny-list, world-access auth, sceneId resolution, presenter sync). |
| POST /get-server-scene-adapter | divergent (cosmetic for client) | better (confirmed, security gap) | major | Client reads only `.adapter`. **Security gap confirmed:** upstream gates on `identity === AUTHORITATIVE_SERVER_ADDRESS`; ours has NO such gate — any signer mints a server publish token. Also identity `server:<addr>` vs hardcoded `authoritative-server`. Not Unity-called. |
| GET /scene-participants | divergent (confirmed) | worse (confirmed) | major (was breaks-client) | Hardcoded empty stub `{data:[],note:...}` vs `{ok,data:{addresses}}`. Functionally non-functional, but NOT Unity-called → downgraded. |
| GET /scene-admin | divergent (confirmed) | better (confirmed, parity gap) | breaks-client (confirmed) | Ours `{data:[SceneAdmin{...snake_case...}]}`; upstream a **bare array** of `{admin,name,canBeRemoved}`. Unity parses `List<AdminInfo>` → object envelope + missing `name`/`canBeRemoved` breaks deserialization. CONFIRMED client-breaking. |
| POST /scene-admin | divergent (confirmed) | better (confirmed, security-relevant gap) | minor | Ours 200 `{ok,id}`; upstream 204 no body. Unity calls this but ignores the body. Skips owner/permission/ban gating + LiveKit metadata sync. |
| DELETE /scene-admin | divergent (confirmed) | better (confirmed) | minor | 200 `{ok,removed}` vs 204. Not Unity-called. Skips permission gate + LiveKit sync. |
| GET /scene-bans | divergent (confirmed) | better (confirmed, parity gap) | major (was breaks-client) | `{data:[SceneBan{...}]}` vs `{results:[{bannedAddress,name}],total,page,pages,limit}`. Real divergence but NOT Unity-called → downgraded. |
| POST /scene-bans | divergent (confirmed) | better (confirmed) | minor | 200 `{ok,id}` vs 204. Skips place resolve, permission gate, ban-by-name, kick/event side-effects. Not Unity-called. |
| DELETE /scene-bans | divergent (confirmed) | better (confirmed) | minor | 200 `{ok,removed}` vs 204. Skips place resolve + permission gate. Not Unity-called. |
| GET /scene-bans/addresses | divergent (confirmed) | better (confirmed) | major (was breaks-client) | `{data:[String]}` vs `{results,total,page,pages,limit}`. NOT Unity-called → downgraded. |
| GET /users/{address}/bans | divergent (confirmed) | same (confirmed) | breaks-client (confirmed) | Ours top-level `{banned,reason?,customMessage?,expiresAt?}`; upstream `{data:{isBanned,ban?:UserBan}}`. Unity `GetBanStatusResponse` reads `data.isBanned` + `data.ban`. CONFIRMED client-breaking (missing `data` wrapper + `banned` vs `isBanned`). |
| POST /livekit-webhook | divergent (confirmed) | worse (confirmed) | major | Ours `{ok,event}` + logs only; upstream echoes raw body string AND dispatches real DB/LiveKit side-effects via event-handlers. Functional no-op. NOT Unity-called (LiveKit→gatekeeper webhook). |

## Confirmed shape issues (real + impactful)

1. **GET /scene-admin** — `{data:[...]}` envelope vs bare array; per-item missing
   `name` + `canBeRemoved` and using snake_case (`place_id`,`added_by`,`created_at`).
   Unity (`SceneAdmins.cs:132`) deserializes the response as `List<AdminInfo>` via
   Newtonsoft and reads `r.admin` (and AdminInfo also declares `name`,`canBeRemoved`,
   `id`,`active`). An object envelope where an array is expected breaks the parse.
   **breaks-client.** (Rust: `handlers/scene_admin.rs:33` + `ports/scene_admin.rs:8`.)

2. **GET /users/{address}/bans** — ours returns the status object at top level with
   key `banned`; upstream wraps in `{data:{isBanned,ban?}}`. Unity
   `GetBanStatusResponse` (`ApplicationBlocklistGuard/GetBanStatusResponse.cs`) reads
   `data.isBanned` and `data.ban`. Missing `data` wrapper + `banned`≠`isBanned` ⇒
   client always reads `isBanned=false` (parse miss). **breaks-client.**
   (Rust: `handlers/user_bans.rs:7` + `ports/user_bans.rs:8`; upstream
   `user-moderation/ban-status-handler.ts` + `logic/user-moderation/types.ts:25`.)

3. **POST /get-server-scene-adapter — missing authoritative-server gate (security).**
   Upstream `comms-server-scene-handler.ts` throws `UnauthorizedError` when
   `identity !== AUTHORITATIVE_SERVER_ADDRESS`. Ours (`scene_adapter.rs:82`) derives
   `server:<addr>` from any valid signer and mints a publish+subscribe token with no
   address check. Shape divergence is cosmetic (client reads `.adapter`), but the
   missing gate is a real security parity gap. **major.**

4. **GET /scene-participants** — hardcoded empty stub; never returns the live roster.
   Real shape divergence (`{data:[],note}` vs `{ok,data:{addresses}}`) and a functional
   gap, but not Unity-called. **major.**

5. **POST /livekit-webhook** — functional no-op. Ours verifies HMAC then logs; upstream
   dispatches participant-joined/left, ingress-started/ended, room-started handlers that
   mutate DB + LiveKit. Also echoes raw body vs `{ok,event}`. Not Unity-called. **major.**

6. **Status-code / body divergences on the mutating routes** (POST/DELETE scene-admin,
   POST/DELETE scene-bans): ours 200 `{ok,...}` vs upstream 204 no-body. Tolerated by
   the explorer (POST /scene-admin ignores the body; the rest aren't Unity-called).
   **minor.**

7. **GET /scene-bans, GET /scene-bans/addresses** — `{data:[...]}` vs paginated
   `{results,total,page,pages,limit}` (+ snake_case, missing `name`). Real, but
   admin/service surface, not the explorer. **major.**

## Confirmed efficiency wins (with structural reason)

- **POST /get-scene-adapter (better).** Ours = 2 concurrent local SQL queries
  (`user_bans` SELECT + `scene_bans` COUNT via `tokio::try_join!`, `scene_adapter.rs:47`)
  then in-process LiveKit JWT mint. Upstream (`comms-scene-handler.ts`) does:
  platform-ban DB check, deny-list check, optional `worlds.fetchWorldSceneId` (HTTP),
  `sceneBans.isUserBanned` (DB), for worlds `worlds.hasWorldAccessPermission` (HTTP),
  `places.getPlaceByParcel`/`getWorldByName` (HTTP) + `isSceneOwnerOrAdmin` +
  `cast.addPresenter` (LiveKit RoomService round-trip). Structurally far fewer
  network round-trips — but the win comes from *omitting* deny-list, world-access auth,
  sceneId resolution and presenter sync (parity gap, not pure speed).

- **POST /get-server-scene-adapter (better).** Ours = ZERO DB / no HTTP, direct JWT
  mint. Upstream = deny-list check + optional `worlds.fetchWorldSceneId` (HTTP) +
  config equality gate. Fewer round-trips, but the omission includes the security gate
  (see shape issue 3).

- **GET /scene-admin (better).** Ours = 1 indexed SELECT on `scene_admin` by `place_id`
  (`ports/scene_admin.rs:27`). Upstream = parcel/realm resolve + `places.getPlaceByParcel`
  (HTTP) + admins DB query + `lands.getLeaseHoldersForParcels` (HTTP/subgraph) +
  `names.getNamesFromAddresses` (HTTP batch). Confirmed: upstream really fans out to
  lands + names + places HTTP; ours does none. Win is real but driven by skipped
  enrichment (no name/lease/extra-address). Note: this enrichment is exactly what makes
  the *shape* break the client (missing `name`/`canBeRemoved`).

- **GET /scene-bans, GET /scene-bans/addresses (better).** Ours = 1 SELECT, no count.
  Upstream = places resolve (HTTP) + permission gate + list + count (2 DB) +
  `names.getNamesFromAddresses` (HTTP batch). Confirmed in `list-scene-bans-handler.ts`
  / `list-scene-bans-addresses-handler.ts` (pagination + count). Real, parity gap.

- **POST/DELETE scene-admin, POST/DELETE scene-bans (better).** Ours = single
  INSERT/UPDATE/DELETE. Upstream resolves place (HTTP), permission-gates, then writes +
  LiveKit metadata sync / kick / notification side-effects. Confirmed by reading each
  upstream handler. Real, but again the win is from skipped gating/side-effects.

- **POST /livekit-webhook (worse, confirmed).** Ours skips all event processing; upstream
  does DB + LiveKit side-effects. Cheaper only by being a no-op.

None of the "better" verdicts rest on language choice alone — each is backed by a
concrete count of skipped HTTP/DB/LiveKit round-trips read from both implementations.

## Rejected / corrected during verification

- **Severity overstatement on non-Unity GET routes.** GET /scene-bans,
  GET /scene-bans/addresses, GET /scene-participants were tagged `breaks-client`.
  The net-catalog has NO Unity call to any `/scene-bans*` or `/scene-participants`
  route, so they cannot break the explorer. Corrected to `major` (real divergence on a
  non-explorer surface). The two genuinely client-breaking shapes are
  **GET /scene-admin** and **GET /users/{address}/bans** — both verified against the
  actual Unity deserialization types.

- **POST /livekit-webhook severity vs. client.** Kept `major` but note the shape diff
  (echoed-string vs JSON) does NOT affect the explorer — LiveKit server is the caller.
  The real issue is the functional no-op (skipped side-effects), not the response shape.

- **`world-ban-check-handler.ts` is NOT the `/users/:address/bans` handler.** The flagged
  `GET /users/{address}/bans` shape was verified against the correct upstream handler
  (`user-moderation/ban-status-handler.ts`, route `routes.ts:247`), not the
  bearer-token world ban-status route. The `{data:{isBanned,ban}}` claim holds.

- **GET /status framed as full divergence.** Confirmed divergent, but reduced to
  cosmetic-for-the-client: Unity treats `/status` as a URL constant and parses no body,
  so the missing `currentTime`/`commitHash` and extra fields don't affect the explorer.

- **No "better"-by-language-only claims found.** Every efficiency verdict was traceable
  to a structural difference (skipped HTTP fan-out / DB count / LiveKit round-trip), so
  none were rejected on the language-choice ground.
