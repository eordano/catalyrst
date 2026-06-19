# Verification (adversarial re-check) — `catalyrst-communities` (service `communities`, bundle `social` 5145)

Upstream: `social-service-ea` (community REST). Committed tree: branch `feat/service-plane-crates`,
crate at `crates/catalyrst-communities`. Verified read-only against committed Rust, the
`social-service-ea` TypeScript, and the Unity `CommunitiesDataProvider` C# DTOs/converters.
Nothing was run; all conclusions from source. Scope: the 5 supplied findings + crate-level claims.

## Per-endpoint table

| endpoint | shape | client-reaction | severity | failure-modes-ok | notes |
|---|---|---|---|---|---|
| GET /v1/communities/{id}/members | match | ok (no null-crash) | minor | partial | Client-called (catalog confirms incl. `onlyOnline`/`limit=0` probes). 3 asserted C# fields always present. Admin-enum throw is REACHABLE via federation (not client writes). No existence check: we 200-empty vs upstream 404. |
| GET /v1/communities/{id}/bans | match | ok | minor | partial | Client-called. Reuses members converter; our row has NO `role` field -> Populate leaves enum default, no throw. Auth: we 400 vs upstream 401. No existence check. |
| GET /v1/communities/{id}/requests | match | ok | minor | partial | Client-called. `type` constrained on insert to invite/request_to_join (both in enum) -> no throw. Auth: we 400 vs upstream 401. No existence check. |
| GET /v1/members/{address}/communities | match | n/a (never called) | none | n/a | NOT in Unity net-catalog, no C# caller. Self-only 401 gate matches upstream. shape/client-reaction moot. |
| POST /v1/members/{address}/communities | divergent (501 stub) | n/a (never called) | none | partial | NOT client-called. 501 after admin-bearer check; upstream returns data. No consumer -> no client impact. |

## Findings CONFIRMED on the committed tree

1. **No community-existence check on the three list reads (real divergence).**
   `get_members` (handlers/members.rs:11), `get_bans` (handlers/bans.rs:11), `get_community_requests`
   (handlers/requests.rs:20) parse the UUID then query rows directly. Unknown/empty community id ->
   `200 {data:{results:[],total:0,...}}`. Upstream `getCommunityMembers`
   (social-service-ea `src/logic/community/members.ts:63-66`) calls `communityExists` and throws
   `CommunityNotFoundError` -> **404**; handlers re-throw it (get-community-members-handlers.ts:54,
   get-banned-members-handler.ts:43; bans logic also throws at bans.ts:43/125). Confirmed all three.

2. **Auth-failure status divergence on bans + requests (we 400 vs upstream 401).**
   Both reads call `require_signer(...).map_err(|e| ApiError::bad_request(...))` (bans.rs:20-21,
   requests.rs:29-30) -> `MarketplaceApiError::Http(400)`. Upstream wires both routes with
   `signedFetchMiddleware()` (NOT optional) in `src/controllers/routes/http.routes.ts:98,113`;
   `NotAuthorizedError` maps to **401** (platform-server-commons `error-handler.ts:32-40`).
   `GET /members` upstream is `optional:true` and our `get_members` correctly requires no signer -> that one matches.

3. **Latent admin-role enum throw is a REAL reachable path via federation (upgraded from "latent").**
   `community_members.role` is a free `VARCHAR`, no CHECK (migrations/0001_initial.sql:38). The fed
   grant path lands `'admin'` there: `apply_role` (fed/apply.rs:182-195) inserts role into
   `community_role_log`; AFTER-INSERT trigger `crl_apply_trg` (migrations/0002_federation.sql:120-154)
   writes it into `community_role_current`; the same-tx SELECT (apply.rs:197-203) reads it back and
   the `INSERT ... community_members` (apply.rs:215-224) persists role=`'admin'`. `Role::Admin` is a
   first-class authority role (fed/authority.rs:11; `can_grant`: Owner can grant Admin, authority.rs:80-83).
   The members port serializes `community_members.role` verbatim (ports/members.rs:14,33-34). Unity's
   `CommunityMemberRole` enum = {member,moderator,owner,none,unknown} (CommunityMemberRole.cs:7-14) —
   no `admin`. The converter `Populate`s with a default `JsonSerializer` (CommunitiesDTOConverters.cs:73)
   -> a string outside the enum throws `JsonSerializationException`, failing the whole response.
   The data-provider GET (CommunitiesDataProvider.cs:189-191) has NO suppression on the read leg, so
   the request throws. **Not** reachable through Unity-client writes (`update_member_role` client path
   only grants member/mod/owner) but reachable the moment a federated admin grant is replayed.
   Genuine cross-implementation hazard; severity federation-gated, not client-triggerable today.

4. **Two divergent error envelopes in one crate (confirmed; inert for the client).**
   Read handlers + fed writes -> `MarketplaceApiError::into_response`
   (catalyrst-types/src/error.rs:135-148): `{"ok":false,"message":...}`; sqlx -> 500 with literal
   "database error" (real error only `tracing::error!`-logged, error.rs:140-142). Client-facing writes
   use a local `{"message":...}` (no `ok`). Upstream uses `{error,message}`/`{message}`. For Unity GETs
   that throw on non-2xx without parsing the body, the extra `ok` and missing `error` keys are inert.

5. **501 deferred admin batch (writes.rs:989-1005) — divergent but no consumer.**
   Admin-bearer check then `501 {"ok":false,"message":"admin batch read is deferred..."}`. Upstream
   `get-member-communities-by-ids` returns data. Endpoint absent from net-catalog; not Unity-called.

6. **Startup panic-free; clean Err before bind (confirmed).**
   Only `COMMUNITIES_PG_CONNECTION_STRING` is `required()` (config.rs:30). `build_state`
   `.context()?`-bubbles pool connect (lib.rs:65), `migrate!` (lib.rs:70), `Replay::new` (lib.rs:74),
   `ContentStore::init` (lib.rs:78-83) to `main` -> clean error exit, not a Rust panic. Content DB
   unset/unreachable -> `content_pool=None`, enrichment disabled, server still starts (lib.rs:89-110).
   Bogus default content dir `./data/communities/content` (config.rs:20) is real;
   if unset and uncreatable, startup fails — the deployment unit must set `COMMUNITIES_CONTENT_DIR`.

## Skeptic corrections to the supplied findings

- Converter file is **`CommunitiesDTOConverters.cs`**, not `DTOConverters.cs`. Members/bans converter
  `GetCommunityMembersResponseMemberDataConverter` at line 54; request converter
  `CommunityInviteRequestDataConverter` at line 87 (line numbers coincide, filename was wrong).
- The members `role` is NOT added by enrichment — it comes straight from the DB column
  (ports/members.rs:14). `enrich_with_profiles` only adds name/profilePictureUrl/hasClaimedName, and
  inserts them UNCONDITIONALLY even when the content pool is `None` (enrich.rs:33 None-branch -> empty
  strings + false). So the non-null asserts (memberAddress!/hasClaimedName!/profilePictureUrl!,
  CommunitiesDTOConverters.cs:76-79) can never null-crash. memberAddress is from the struct itself.
- Finding said bans rows "carry role default 'none'" — inaccurate: `CommunityBan` has **no role field
  at all** (ports/bans.rs:8-19), so the serialized ban row omits `role`; Newtonsoft `Populate` leaves
  the enum at default 0 = `member`. Conclusion (no throw) is right; reasoning is not. Same for
  `CommunityRequest` (no role field). The member-requests aggregate hardcodes role:"none"
  (requests.rs:124) but that is a different endpoint/shape, not the community-requests read.
- `GET`/`POST /v1/members/{address}/communities` genuinely absent from the net-catalog with no C#
  caller — severity "none" is correct; their shape/client-reaction verdicts are moot.

## Client-crash risks

- **GET /v1/communities/{id}/members** — a returned member row with role=`'admin'` (federation
  admin-grant path, #3) makes Unity `CreateFromJson<GetCommunityMembersResponse>` throw on the enum
  parse, failing the whole members fetch. Not triggerable by client writes today; triggerable by a
  replayed federated admin grant. The one substantive crash vector in this surface.
- All list reads (members/bans/requests) + member-communities THROW in the client on any non-2xx (no
  `.SuppressToResultAsync` on the GET read leg in CommunitiesDataProvider). So 400-instead-of-401
  (bans/requests auth) and 200-empty-instead-of-404 (existence) do not change client behavior beyond
  what upstream would surface — and the 200-empty case actually AVOIDS a throw the client would get
  from upstream's 404 (we are more lenient, divergent).

## Failure-mode gaps (diverge from upstream status / degrade differently)

- Unknown/empty community on GET members|bans|requests: we `200 {results:[]}`; upstream `404`.
- Missing/invalid auth chain on GET bans|requests: we `400 {ok:false,message}`; upstream `401`.
- sqlx error on the main fetch leg: we `500 {ok:false,message:"database error"}` (real error logged
  only). Count-leg / thumbnail-leg failures are swallowed (`.unwrap_or(0)` ports/members.rs:52,
  bans.rs:52, requests.rs:61; thumbnail `.ok().flatten().unwrap_or(false)`) -> degrade to 0/false
  rather than 500. Deliberate, benign divergence.
- Admin-role row in members output: we `200` with a body Unity cannot parse (enum throw); upstream
  never stores `admin` as a `community_members` role so never emits this body.
