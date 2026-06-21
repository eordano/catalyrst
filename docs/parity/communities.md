# Parity report — `catalyrst-communities` (service `communities`)

Upstream baseline: `decentraland/social-service-ea` (community REST routes,
`src/controllers/routes/http.routes.ts` + `src/logic/community/*`).
Rust crate: `crates/catalyrst-communities`.

This report was produced by adversarially re-checking every flagged divergence
against (a) the Rust serialization shape, (b) the upstream TS type, and
(c) **what the Unity explorer actually deserializes** — the DTOs under
`unity-explorer/.../Communities/CommunitiesDataProvider/DTOs/`. Several findings
changed verdict once the client side was consulted: the explorer ignores some
fields the upstream type carries (so the divergence is harmless), and conversely
several "missing field" omissions are *worse* than reported because the Unity
JSON converters use non-null assertions and will throw a `NullReferenceException`
rather than silently degrade.

Net-catalog confirms the explorer actively calls these endpoints
(`the Unity net-catalog`): list, get-by-id,
members (incl. `onlyOnline=true`), bans, places, posts (+like), requests,
invites, voice-chats/active.

## Per-endpoint summary

| Endpoint | Shape | Efficiency | Severity | Notes |
|---|---|---|---|---|
| GET /ping | n/a | same | none | Health probe, no upstream counterpart. |
| GET /v1/communities | divergent | better (structural) | major | Omits `thumbnailUrl`,`ownerName`,`role`,`visibility`,`friends[]`,`voiceChatStatus` value — all read by explorer. Extra `isLive`/`unlisted`/`createdAt` ignored by client. |
| POST /v1/communities | divergent | better (by design) | breaks-client | Signed JSON envelope + `{ok:true}` vs client multipart form + `{data:...}`. |
| GET /v1/communities/{id} | divergent | better (structural) | major | Omits `thumbnailUrl`,`visibility` (read by client). `role:"none"` is VALID. Extra `isLive`/`isBanned`/`unlisted` ignored. |
| PUT /v1/communities/{id} | divergent | better (by design) | breaks-client | Signed JSON vs multipart; `{ok:true}` vs `{data:Community}`. |
| PATCH /v1/communities/{id} | divergent | same | breaks-client | Signed JSON + 200 body vs upstream plain JSON + 204. |
| DELETE /v1/communities/{id} | divergent | same | breaks-client | Requires signed body; 200 body vs 204. |
| GET /v1/communities/{id}/members | divergent | better (structural) | breaks-client | Omits `name`/`profilePictureUrl`/`hasClaimedName` -> client converter throws NRE. |
| POST /v1/communities/{id}/members | divergent | same | breaks-client | Signed envelope + 200 body vs signed-fetch + 204. |
| DELETE /v1/communities/{id}/members/{address} | divergent | same | breaks-client | 200 body vs 204; self-leave only. Path-param name cosmetic. |
| PATCH /v1/communities/{id}/members/{address} | divergent | same | breaks-client | Signed envelope + 200 body vs schema-validated + 204. |
| GET /v1/communities/{id}/bans | divergent | better (structural) | breaks-client | Renames `memberAddress`->`bannedAddress` + omits profile fields -> client (reuses members DTO) throws NRE. |
| POST /v1/communities/{id}/members/{address}/bans | divergent | same | breaks-client | 200 body vs 204. |
| DELETE /v1/communities/{id}/members/{address}/bans | divergent | same | breaks-client | 200 body vs 204. |
| GET /v1/communities/{id}/places | divergent | better (structural) | minor (DOWNGRADED) | Client reads ONLY `id`; extra/missing metadata is harmless. |
| POST /v1/communities/{id}/places | divergent | better (by design) | breaks-client | 200 body vs 204; signed envelope. |
| DELETE /v1/communities/{id}/places/{placeId} | divergent | same | breaks-client | 200 body vs 204. |
| GET /v1/communities/{id}/posts | divergent | better (structural) | breaks-client | Renames `isLikedByUser`->`likedByMe`; omits author profile fields read by client. |
| POST /v1/communities/{id}/posts | divergent | worse (by design) | breaks-client | Signed `content_hash` + FS stat vs inline `{content}`; 200 `{ok}` vs 201 `{data:CommunityPost}`. |
| DELETE /v1/communities/{id}/posts/{postId} | divergent | worse (minor) | breaks-client | 200 body vs 204; two single-row lookups that could be one. |
| POST /v1/communities/{id}/posts/{postId}/like | divergent | same | breaks-client | 200 body vs 201 empty. |
| DELETE /v1/communities/{id}/posts/{postId}/like | divergent | same | breaks-client | 200 body vs 204. |
| GET /v1/communities/{id}/requests | divergent | better (structural) | breaks-client | Client expects request+profile hybrid; we omit `name`/`profilePictureUrl` -> converter NRE on list path. |
| POST /v1/communities/{id}/requests | divergent | worse (stub) | breaks-client | 501, not implemented. |
| PATCH /v1/communities/{id}/requests/{requestId} | divergent | same | breaks-client | Signed envelope + 200 body vs 204. |
| GET /v1/communities/{address}/managed | divergent | same | major (AUTH GAP) | NO auth vs upstream admin-bearer-only mount. Not called by explorer. |
| GET /v1/members/{address}/communities | divergent | same | minor (NOT explorer-facing) | Slim `MemberCommunity`; explorer uses `?onlyMemberOf` list instead. |
| POST /v1/members/{address}/communities | divergent | worse (stub) | breaks-client | 501; client DTO is commented out anyway. |
| GET /v1/members/{address}/requests | divergent | better (structural) | breaks-client | Returns request rows; client reads community-aggregated shape (thumbnail/name/ownerName/...). |
| GET /v1/members/{address}/invites | match | same | none | `{data:[{id,name}]}` matches. |
| GET /v1/community-voice-chats/active | divergent | better (structural) | major | Renames `participants`->? (`participantCount`=0 at client); omits `moderatorCount`/`isMember`/`positions`/`worlds`; reads local table not live RPC. |
| GET /v1/moderation/communities | divergent | better (structural) | major (AUTH GAP) | Any signer reads it; upstream requires `COMMUNITIES_GLOBAL_MODERATORS` FF (403 otherwise). |
| /federation/* (5 endpoints) | n/a | n/a | none | catalyrst-only inter-node gossip; no upstream counterpart. |

## Confirmed shape issues (client-impacting)

These were verified against the Unity DTOs and **will** affect the explorer.

1. **GET /v1/communities — missing render fields.**
   `GetUserCommunitiesData.CommunityData` reads `thumbnailUrl`, `ownerName`,
   `role`, `visibility`, `friends[]`, `voiceChatStatus`. Our `list()` emits none
   of those (`ports/communities.rs:235`). Cards render with no thumbnail, no
   owner name, no join-state (`role`), no friends overlay, no voice indicator.
   Extra `isLive`/`unlisted`/`createdAt` are silently ignored.

2. **GET /v1/communities/{id} — missing `thumbnailUrl`/`visibility`.**
   `GetCommunityResponse.CommunityData` reads `thumbnailUrl`, `visibility`,
   `role`, `voiceChatStatus`. We omit `thumbnailUrl`/`visibility`
   (`ports/communities.rs:99`). `voiceChatStatus:null` is *tolerated* (the
   client's converter maps null -> isActive=false). Extra `isLive`/`isBanned`
   ignored.

3. **GET /v1/communities/{id}/members — hard crash, not just missing data.**
   The client converter
   (`CommunitiesDTOConverters.GetCommunityMembersResponseMemberDataConverter`)
   does `jObject["memberAddress"]!`, `jObject["name"]!`,
   `jObject["profilePictureUrl"]!` with non-null assertions. We emit
   `memberAddress` but NOT `name`/`profilePictureUrl`/`hasClaimedName`
   (`ports/members.rs`), so the converter throws `NullReferenceException`. This
   is more severe than "major" — the member list fails to parse entirely.

4. **GET /v1/communities/{id}/bans — same crash + key rename.**
   Bans are deserialized into the *same* `GetCommunityMembersResponse`
   (`CommunitiesDataProvider.cs:199`). We emit `bannedAddress` (not
   `memberAddress`) and no profile fields -> converter NRE.

5. **GET /v1/communities/{id}/posts — like-state + author break.**
   Client `CommunityPost` reads `isLikedByUser`; we emit `likedByMe`
   (`ports/posts.rs:20`) so the like toggle reads false always. We also omit
   `authorName`/`authorProfilePictureUrl`/`authorHasClaimedName` (author byline
   blank). Base fields + `{data:{posts,total}}` envelope match.

6. **GET /v1/communities/{id}/requests — converter NRE on list path.**
   Client `CommunityInviteRequestDataConverter` reads request fields
   (`id`,`communityId`,`type`,`status`) AND profile fields (`memberAddress`,
   `name`, `profilePictureUrl`). We supply the request fields but not the
   profile fields, so the list path (`GetCommunityInviteRequestAsync`) throws.
   The count-only path (`GetCommunityRequestsAmountAsync`, reads `.total`) works.

7. **GET /v1/members/{address}/requests — disjoint shape.**
   Client `UserInviteRequestData` reads a community-aggregated shape
   (`thumbnailUrl`,`name`,`description`,`ownerAddress`,`ownerName`,`privacy`,
   `membersCount`,`friends[]`,`role`,`active`). We return slim request rows
   (`{id,communityId,memberAddress,status,type,createdAt,updatedAt}`). Almost
   entirely disjoint -> blank rows.

8. **GET /v1/community-voice-chats/active — count reads 0.**
   Client `ActiveCommunityVoiceChat` reads `participantCount`,`moderatorCount`,
   `isMember`,`communityImage`,`positions[]`,`worlds[]`. We emit `participants`
   (so `participantCount` deserializes to 0) + `startedAt`, omitting the rest.
   Also sourced from a local mirror table, not the live comms-gatekeeper RPC.

9. **Write-contract divergence (all mutating verbs).**
   The explorer mutates via signed-fetch (auth headers, plain/empty body or
   multipart form) and expects `{data:...}` / 204 / 201. Every Rust write
   requires an EIP-712 `Signed<T>` JSON *body* envelope and answers
   `200 {ok:true,signature_hash,...}`. This is the deliberate federation write
   path, but it is not the client contract. Affects POST/PUT/PATCH/DELETE on
   communities, members, bans, places, posts, likes, requests.

10. **POST /v1/communities/{id}/posts — out-of-band content + status.**
    Client sends `{content}` inline, expects 201 `{data:CommunityPost}`. We take
    `content_hash` (content stored separately in the content-store) and answer
    200 `{ok:true,content_body_local}`. Both request and response diverge.

11. **AUTH GAP: GET /v1/moderation/communities.**
    Upstream returns 403 unless `verification.auth` is in the
    `COMMUNITIES_GLOBAL_MODERATORS` feature flag
    (`get-all-communities-for-moderation-handler.ts:26`). Ours gates only on
    `require_signer` (`handlers/moderation.rs:13`). Any signed wallet can read
    the full moderation list.

12. **AUTH GAP: GET /v1/communities/{address}/managed.**
    Upstream mounts this *only* behind `bearerTokenMiddleware(API_ADMIN_TOKEN)`
    and only when that token is configured (`http.routes.ts:81-83`). Ours
    (`handlers/members.rs:get_managed_communities`) applies no auth at all. Not
    called by the explorer, but a server-side read-auth gap.

13. **Stubbed endpoints (501).** `POST /v1/communities/{id}/requests` and
    `POST /v1/members/{address}/communities` return 501. The first is a real
    client-facing gap (join-request creation); the second is admin/worlds-server
    only and its Unity DTO is commented out.

## Confirmed efficiency wins (with structural reason)

All "better" verdicts are **structural**, not language artifacts: in every case
the upstream TS handler performs external network fan-out that we elide by
reading only our local Postgres. Verified in `src/logic/community/communities.ts`
and `members.ts`.

- **GET /v1/communities (list):** upstream does the 2 pg queries PLUS
  `communityOwners.getOwnersNames` (catalyst + redis), `registry.getProfiles`
  on the friend set (catalyst), and `commsGatekeeper.getCommunityVoiceChatStatus`
  per page (`communities.ts:261-278`). We do 2 SQL.
- **GET /v1/communities/{id}:** upstream `Promise.all` of comms-gatekeeper voice
  status + `getOwnerName` (redis) + `isHostingLiveEvent` (events-service)
  (`communities.ts:186-201`). We do up to 4 trivial SQL, no RPC.
- **GET .../members, .../bans, .../requests, /members/{addr}/requests:**
  upstream runs `aggregateWithProfiles` -> `registry.getProfiles` batch
  (catalyst + redis); members additionally an optional presence query for
  `onlyOnline`. We do SELECT + COUNT.
- **GET .../places:** upstream enriches each row from the external places
  service; we return ids only (and the client only needs ids — see rejected #2).
- **GET /v1/community-voice-chats/active:** upstream hits comms-gatekeeper RPC
  for live state; we read a mirrored table (1 SQL). Cheaper but possibly stale.
- **GET /v1/moderation/communities:** upstream adds a launchdarkly/redis FF
  lookup before the same 2 SQL — but that lookup is also the authorization gate
  we are missing (see issue #11), so this "win" is not free.

Caveat for all of the above: the efficiency win and the shape gap are the same
coin. We are faster *because* we drop the enrichment the explorer renders.

### Confirmed efficiency regressions ("worse")

- **POST /v1/communities/{id}/posts:** adds a `content_store.exists()` filesystem
  stat (`handlers/writes.rs:495`) and stores body out-of-band (a later
  federation pull) vs upstream's inline `{content}` INSERT. Deliberate
  content-addressing, but more I/O on the write path.
- **DELETE /v1/communities/{id}/posts/{postId}:** issues two separate single-row
  lookups on `community_posts_log` keyed by the same `signature_hash`
  (`community_id_for_post` at writes.rs:534 + `post_author` at writes.rs:541)
  where one query would do. Minor (indexed single-row lookups).

## Rejected during verification

1. **GET /v1/communities/{id} — "role defaults 'none', not a valid enum".**
   REJECTED. The Unity `CommunityMemberRole` enum explicitly contains `none`
   (`CommunityMemberRole.cs`). `role:"none"` deserializes cleanly; it is the
   correct not-a-member sentinel. No client impact from the value.

2. **GET /v1/communities/{id}/places — severity "major" / "lose the metadata the
   explorer renders".** DOWNGRADED to minor. The Unity client
   (`GetCommunityPlacesAsync`, CommunitiesDataProvider.cs:222-238) deserializes
   into `GetCommunityPlacesResult` which contains ONLY `id`, then fetches place
   details separately. Our extra `communityId`/`addedBy`/`addedAt` are ignored
   and the omitted `title`/`positions`/`world` are never read here. Not
   client-breaking.

3. **`isLive` (list + get-by-id) as a "wrong/extra key" that matters.** PARTIALLY
   REJECTED as client-impacting. The key is indeed extra/non-upstream, but no
   Unity community DTO has an `isLive` or `isHostingLiveEvent` field, so the
   client silently ignores it. It is noise, not a break. (Kept as a note, not a
   breaking issue.) Same for our extra `unlisted`/`createdAt`/`updatedAt`/
   `isBanned` — all ignored by the client.

4. **`voiceChatStatus: null` (get-by-id) as "wrong".** SOFTENED. The client's
   `VoiceChatStatusJsonConverter` maps a null token to
   `{isActive:false,participantCount:0,moderatorCount:0}`, i.e. "no active voice
   chat" — a benign, correct-looking default. Not a parse break (unlike the
   list, where the value is also null but the bigger issue is the other omitted
   fields).

5. **GET /v1/members/{address}/communities & /managed — "major" client impact.**
   DOWNGRADED to not-explorer-facing. The explorer obtains the user's
   communities via `GET /v1/communities?onlyMemberOf=true` (the list endpoint),
   not via `/v1/members/{address}/communities`, and never calls `/managed`
   (confirmed: no such URL is constructed anywhere in the Unity client). The
   `MemberCommunity` shape gap is real for other consumers but does not affect
   the explorer. (The `/managed` AUTH gap is kept as a server-side finding.)

6. **"breaks-client" on PATCH/DELETE status-code-only divergences.** KEPT but
   noted as lower-confidence: for the fire-and-forget mutations
   (delete/leave/unban/unlike) the explorer typically discards the body
   (`WithNoOpAsync().SuppressToResultAsync(...)`) and keys off the HTTP status.
   A 200-with-body where 204 is expected is usually tolerated by those call
   sites, so "breaks-client" overstates several of these; the real break is the
   *request* contract (signed JSON body), not the response status.
