# Verification: catalyrst-worlds (service "worlds") vs worlds-content-server

Adversarial re-check of the prior re-check findings, against the **committed tree**
(`feat/service-plane-crates`). Crate: `crates/catalyrst-worlds`.
Upstream TS: `decentraland/worlds-content-server`.
Unity consumer: `decentraland/unity-explorer`.

Routing is in `crates/catalyrst-worlds/src/lib.rs:64-92`. The findings cited a couple of stale
line refs (`worlds.rs:125-142` is actually `handlers/active.rs` + `ports/worlds.rs:103-143`;
`GateKeeperSceneRoom.cs:218` is correct). Substance verified regardless.

## Per-endpoint table

| Endpoint | Shape | Client reaction | Severity | Failure-modes OK | Notes |
|---|---|---|---|---|---|
| POST /entities/active | match | n/a (not a worlds Unity target) | none | partial | Client posts to **asset-bundle-registry**, not worlds (net-catalog confirms). Parity-only. `#[serde(default)] pointers` -> missing/non-array body yields 200 `[]`, upstream throws InvalidRequestError 400. We SKIP nameDenyListChecker banned-worlds filtering. MAX_POINTERS=50 enforced. id + metadata.owner augmentation matches `getEntityForWorlds`. |
| POST /worlds/{world}/comms | match | ok (status + `{error}` body; no crash) | minor | yes | Response `{fixedAdapter}` exact match to upstream `worldCommsHandler`. Error body `{error:msg}` MATCHES upstream (the comms handler hand-rolls `{error}`, NOT `{error,message}`). Client `WorldPermissionsService.ValidatePasswordAsync` reads status for ok/fail AND parses `{error}` for the 429 message. **Missing 503 capacity gate** (see gaps). |
| POST /worlds/{world}/scenes/{scene}/comms | match | ok (`adapter ?? fixedAdapter` -> fixedAdapter) | minor | yes | Same handler/mint with sceneId branch. Client `GateKeeperSceneRoom.cs:218` `connectionString = IsNullOrEmpty(adapter) ? fixedAdapter : adapter`; `AdapterResponse` is a `[Serializable] struct` of string fields -> no null-crash. SceneNotFound -> 404 matches. **Missing 503 capacity gate + per-scene ban check** (see gaps). |
| GET /contents/{hash} | divergent (impl) / byte-stream parity | ok (raw bytes, no DTO) | minor | partial | Raw binary proxy to CONTENTS_UPSTREAM_URL. We DELEGATE Range/206/416 to upstream (which honors RFC7233 natively); upstream serves locally from @dcl/catalyst-storage. Byte-stream parity preserved. is_ipfs_v2 gate before proxy. Bad-hash -> bare 400 no body — **this actually MATCHES upstream** (`return { status: 400 }`, no body). |
| HEAD /contents/{hash} | match | n/a (parity/availability) | none | partial | Same proxy with Method::HEAD. Mirrors upstream `headContentFile` (200 headers / 404). Bad-hash 400 empty body matches upstream. |
| POST /livekit-webhook | divergent / UNSAFE | n/a (LiveKit -> us, not Unity) | minor (security) | no | NO Authorization header check, NO signature verification (LIVEKIT_WEBHOOK_KEY read but unused), NO NATS publish, no `.dcl.eth` suffix requirement. Presence spoofable by any unauthenticated POST. |

## Confirmed issues

1. **livekit-webhook is unauthenticated (CONFIRMED, real).**
   `handlers/webhook.rs` accepts `{event,room,participant}` and mutates in-process presence
   with zero auth. Upstream (`livekit-webhook-handler.ts`) requires the `Authorization` header
   (throws InvalidRequestError if absent) and calls `livekitClient.receiveWebhookEvent` ->
   LiveKit `WebhookReceiver.receive(body, authorization)`, which cryptographically verifies the
   signature. `config.rs:61` reads `LIVEKIT_WEBHOOK_KEY` into config but it is referenced nowhere
   else in the crate. Severity: server-to-server only (not a Unity target), so no client impact,
   but presence state is forgeable. Also: no NATS `peer.<id>.world.<join|leave>` publish, and
   `world_from_room` does not enforce the `.dcl.eth` suffix upstream requires.

2. **No world-capacity gate -> 503 never returned (NEW, missed by prior findings).**
   Upstream `logic/comms/component.ts:86,103` checks `participantCount >= maxUsersPerWorld` and
   throws `WorldAtCapacityError` -> `world-comms-handler.ts` returns **503**. Our `handlers/comms.rs`
   has `cfg.max_users_per_world` (config.rs:19,62) but NEVER reads it and performs no
   participant-count check. We always mint a token; a full world is never rejected. Functional
   parity gap, not a crash (client treats 503 as a generic retryable failure).

3. **Scene comms omits per-scene ban check (NEW, minor).**
   Upstream scene path calls `assertUserNotBannedFromScene(userAddress, worldName, sceneBaseParcel)`
   (`component.ts:82`). Ours only checks platform ban via `is_wallet_blocked` (comms.rs:66).
   A user banned from a specific scene but not platform-banned would still get a token.

4. **/entities/active diverges on bad input + skips deny-list (CONFIRMED, parity-only).**
   `#[serde(default)] pointers` (active.rs:13) means a missing/non-array `pointers` returns 200 `[]`;
   upstream throws InvalidRequestError 400. Malformed JSON body -> axum `Json` extractor 422 with a
   plain-text body (not `{error}`). We also skip `nameDenyListChecker` banned-worlds filtering. None
   of this reaches the client because the Unity world-realm path posts active-entities to
   asset-bundle-registry, not to worlds (net-catalog: every `/entities/active` call targets
   `asset-bundle-registry.decentraland.{ENV}` or catalyst content, never worlds-content-server).

## Rejected / downgraded findings

- **"Error body diverges from upstream {error,message} -> harmless because no consumer parses it."**
  PARTIALLY WRONG on both halves, net conclusion still benign:
  - The comms handlers in upstream do NOT emit `{error,message}` — `world-comms-handler.ts`
    hand-rolls `body: { error: error.message }`. Our `{error: msg}` therefore MATCHES upstream for
    the comms endpoints.
  - A consumer DOES parse the worlds error body: `WorldPermissionsService.cs:256` deserializes
    `BackendErrorResponse { [JsonProperty("error")] Error }` from the comms 429 response. Our
    `{error: msg}` is exactly that field. The parse is in a try/catch and only used for the 429
    message string, so no crash. Conclusion (client_reaction ok) stands — strengthened, not weakened.

- **"contents bad-hash 400 empty body is inconsistent with the error model (ok:false)."**
  Cosmetic. It is inconsistent with our OWN `{error}` model, but it is byte-for-byte identical to
  upstream (`content-file-handler.ts`: `if (!IPFSv2.validate(...)) return { status: 400 }`, no body).
  Client reads raw bytes only; downgrade to non-issue.

## Client-crash risks

None. Verified:
- Scene comms: `AdapterResponse` is a `[Serializable] struct` of `string adapter/fixedAdapter`;
  `string.IsNullOrEmpty(response.adapter) ? response.fixedAdapter : response.adapter` is null-safe.
  Non-2xx -> `UnityWebRequestException` caught by `CycleStepAsync` (catch-all on non-cancel) and
  rethrown into an upper recovery loop. No null deref, no uncaught throw.
- World comms (ValidatePassword): reads status code; error-body parse wrapped in try/catch with
  nullable field. No crash.
- contents/HEAD: raw byte stream, no DTO to null-deref.

## Failure-mode gaps (error paths that diverge / degrade wrong)

1. **No 503 at capacity** — comms ignores `max_users_per_world`; upstream returns 503
   WorldAtCapacityError. Over-admits users. (Issue 2)
2. **No per-scene ban** — scene comms skips `assertUserNotBannedFromScene`. (Issue 3)
3. **/entities/active accepts malformed/empty pointers as 200 []** instead of upstream 400;
   malformed JSON -> 422 plain-text instead of 400 `{error}`. Parity-only (not a worlds Unity target).
4. **livekit-webhook accepts forged unsigned POSTs** (200 `{ok:true}`, mutates presence) where
   upstream rejects unverified Authorization; also no NATS fan-out and no `.dcl.eth` enforcement. (Issue 1)
5. **DB down = refuses to boot** — `build_state` (lib.rs:36-62) requires Postgres connect + migrations;
   any failure returns Err and the process exits non-zero (not a panic). No graceful degradation.
   At runtime a DB outage surfaces per-request as 500 `{error:"database error"}` (no panic). LiveKit
   unconfigured degrades gracefully to devkey/devsecret (config.rs:28-39): tokens mint with valid
   shape but a real cluster rejects them — functional degradation, not a crash. Presence is in-process
   (DashMap); restart loses all presence.
