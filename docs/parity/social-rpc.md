# Parity: catalyrst-social-rpc (service `social-rpc`)

Adversarial verification of WS-RPC parity for `crates/catalyrst-social-rpc` against
upstream `decentraland/social-service-ea` (dcl-rpc friends/voice protobuf v2).

**Method.** For every flagged divergence I opened the Rust handler (`src/service.rs`,
`src/db.rs`, `src/pubsub.rs`, `src/gatekeeper.rs`), the upstream TS handler
(`social-service-ea/src/controllers/handlers/rpc/*` + `src/logic/friends/*`), the proto
(`protocol/.../social_service_v2.proto`), and — critically — what the **explorers actually
read**: bevy (`bevy-explorer/crates/social/src/client.rs`, `lib.rs`) and Unity
(`unity-explorer/.../DCL/Friends/RPCFriendsService.cs` + friends-panel managers). Diffs that
the clients ignore are downgraded; diffs the clients consume are confirmed.

Upstream client transport confirmed live: Unity connects to
`wss://rpc-social-service-ea.decentraland.{ENV}` (net-catalog). Both explorers speak this
service.

## Per-endpoint table

| Endpoint | Shape | Efficiency | Severity | Notes |
|---|---|---|---|---|
| WS handshake/auth | match | same | none | Not re-checked (flagged as match). |
| getFriends | divergent | better* | minor | Off-by-one `page` real but **clients ignore `page`**; they read `total` (real ✓). Blank profiles real + **client-visible**. |
| getMutualFriends | divergent | better* | minor | Same as getFriends; missing `EthAddress.validate` real but low-impact. |
| getPendingFriendshipRequests | divergent | worse | **major** | `total` hardcoded 0 (service.rs:1340) — **Unity reads it** (RPCFriendsService.cs:409 → request count UI). Confirmed regression. |
| getSentFriendshipRequests | divergent | worse | **major** | Same `total:0` regression; **Unity reads it** (line 457). |
| getFriendshipStatus | divergent | worse | minor | Blocks-table precedence divergence real (upstream consults only last action); +2 SQL real. |
| upsertFriendship | divergent | same/slightly-better | **major** | Missing `isFriendshipBlocked` pre-check real (upstream blocks ALL actions between blocked users). Blank `friend` profile. |
| blockUser | divergent | better | minor | No ProfileNotFound/validate, blank hydration — all real. |
| unblockUser | divergent | better | minor | **Correction:** upstream ALSO emits a friendship DELETE update on unblock; ours emits none. |
| getBlockedUsers | divergent | better | minor | Pagination-semantics diff real but **benign** — both explorers paginate on `total`, which ours supplies. |
| getPrivateMessagesSettings | divergent | worse | **major** | N+1 (db.rs:501-519, 2*N queries) vs upstream 2 batched IN-list queries — confirmed structural. Missing 50-addr guard real. |
| subscribeToFriendshipUpdates | divergent | same | minor | In-proc broadcast vs Redis (single-node only). |
| subscribeToFriendConnectivityUpdates | divergent | same | **major** | Never published (no `SocialEvent::FriendConnectivity` producer) + no initial snapshot — idle stream. |
| subscribeToBlockUpdates | divergent | same | minor | Published; transport differs (in-proc vs Redis). |
| subscribeToPrivateVoiceChatUpdates | divergent | same | minor | Published; transport differs. |
| subscribeToCommunityVoiceChatUpdates | divergent | same | **major** | `SocialEvent::CommunityVoice` never published — idle. |
| subscribeToCommunityMemberConnectivityUpdates | divergent | same | **major** | STUB: yielder dropped immediately (service.rs:712-715). |
| startPrivateVoiceChat | divergent | better | minor | Missing 2 cross-busy gatekeeper checks real. |
| acceptPrivateVoiceChat | divergent | same | minor | Fallback LiveKit URL (gatekeeper.rs:60-67) real. |
| rejectPrivateVoiceChat | divergent | same | minor | No caller/callee auth real (service.rs:882). |
| startCommunityVoiceChat | divergent | same | minor | Fallback creds (gatekeeper 501-deferred) real. |
| joinCommunityVoiceChat | divergent | same | minor | Fallback creds real. |

\* "better" is a **tradeoff, not a free win**: ours skips upstream's `registry.getProfiles`
(Redis mGet + `POST /profiles` HTTP on cache miss, confirmed `adapters/registry.ts:35-90`),
which is the same skip that produces the blank-profile shape gap. Structurally fewer
round-trips, at the cost of correctness the clients notice.

## Confirmed shape issues (real AND client-visible)

1. **Blank friend/blocked profiles** (getFriends, getMutualFriends, getPending/Sent,
   getBlockedUsers, upsertFriendship, blockUser, unblockUser). Ours fills only `address`;
   `name`/`profile_picture_url`/`has_claimed_name`/`name_color` are empty. Upstream hydrates
   via `parseProfilesToFriends`/`parseProfileToBlockedUser` (`registry.getProfiles`).
   **Client-visible:** Unity `ToClientFriendProfile` (RPCFriendsService.cs:605-606) and bevy
   `FriendData` (social/src/lib.rs:377-401) copy these fields straight into the friends-panel
   UI / scene SDK. Result: blank names + missing avatars in the friend list.

2. **`paginationData.total` hardcoded to 0 for pending/sent requests** (service.rs:1340).
   Upstream returns the real received/sent count (`component.ts` `getReceivedFriendshipRequestsCount`,
   returned through the handler). **Client-visible (Unity):** `RPCFriendsService.cs:409/457`
   reads `PaginationData?.Total`, feeds `RequestsRequestManager.FetchDataAsync` (line 188:
   `received.TotalAmount + sent.TotalAmount`) which drives the request-count badge and
   paging. With `total:0` Unity shows zero pending/sent requests in the count even though the
   first page's rows arrive. **Confirmed major.** (Bevy fetches a single 100-row page for
   requests and ignores `total` here, so bevy is unaffected — the regression is Unity-only but
   real.)

3. **getFriendshipStatus consults the blocks table; upstream does not.** Ours runs two
   `is_blocked` lookups first and returns `Blocked`/`BlockedBy` whenever a block row exists
   (service.rs:320-325), regardless of the latest friendship action. Upstream computes status
   purely from `getLastFriendshipActionByUsers` → `getFriendshipRequestStatus`
   (`friendships.ts:223-232`), which only maps a literal `block` *action*. Divergent value when
   a block row coexists with a non-block latest action.

4. **upsertFriendship missing block enforcement.** Upstream
   (`component.ts:210-214`) calls `isFriendshipBlocked` first and throws `BlockedUserError`
   (→ `invalidFriendshipAction`) for **any** action between blocked users. Ours has no such
   pre-check; the action proceeds. (Finding said "rejects a REQUEST" — upstream actually
   rejects all actions; directionally correct, scope was understated.)

5. **MAX_USER_ADDRESSES=50 guard missing** in getPrivateMessagesSettings. Upstream returns
   `invalidRequest` at >50; ours processes any count.

6. **Idle / stub subscription streams.** `subscribeToCommunityMemberConnectivityUpdates`
   drops its yielder immediately (service.rs:712-715). `subscribeToFriendConnectivityUpdates`
   and `subscribeToCommunityVoiceChatUpdates` have no producer (no `SocialEvent::FriendConnectivity`
   / `SocialEvent::CommunityVoice` is ever published; the enum variant exists but is never
   constructed by a handler). These are functional no-ops, not faster paths.

7. **Single-node fan-out.** All subscription streams use an in-process `tokio::broadcast`
   keyed by address (pubsub.rs) instead of Redis channels. Correct for single-node; misses
   cross-node updates in a federated deployment. Same proto shape.

8. **unblockUser missing friendship-update fan-out (NEW — finding was wrong here).** The
   finding claimed upstream's unblock does no friendship reactivation "so this matches." In
   fact `component.ts` unblock records a `DELETE` friendship action inside its tx and publishes
   a `FRIENDSHIP_UPDATES_CHANNEL` event when a friendship existed (lines ~178-195). Ours emits
   only a `BlockUpdate`, no friendship update. Minor, but it is an additional real divergence
   the finding glossed over.

## Confirmed efficiency wins (structural, not language-based)

- **getFriends / getMutualFriends / getBlockedUsers — fewer round-trips.** Upstream runs the
  same 2 SQL (paginated SELECT + COUNT) PLUS `registry.getProfiles`, which is a Redis `mGet`
  and, on cache miss, a `POST {registry}/profiles` HTTP fetch (`adapters/registry.ts:41,58`).
  Ours skips that hop entirely. Structural, verified in both impls — but it is the same skip
  that causes the blank-profile gap, so it is a tradeoff.
- **getBlockedUsers — bounded query.** Upstream `getBlockedUsers` (component.ts) takes NO
  pagination: `friendsDb.getBlockedUsers(addr)` loads the ENTIRE block set every call and
  `total = array.length`; the handler's `pagination` only feeds the display `page`. Ours uses
  SQL `LIMIT/OFFSET` + `COUNT(*)`. Ours is genuinely bounded/cheaper and, because both
  explorers paginate on `total` (bevy client.rs:802-804; Unity reads `.Total`), ours is also
  functionally correct for them — so the "truncated slice" risk in the finding does NOT bite
  the real clients.
- **startPrivateVoiceChat — skips 2 Redis checks.** Upstream runs two parallel
  `commsGatekeeper.isUserInCommunityVoiceChat` calls before the INSERT (start-private-voice-chat.ts:31-34).
  Ours relies on the unique constraint only. Fewer network round-trips, at the cost of the
  missing cross-busy guard.

## Efficiency claims rejected / corrected

- **getPendingFriendshipRequests / getSentFriendshipRequests "worse" — KEPT, label sharpened.**
  Not an efficiency win; the dropped COUNT is a correctness regression (Unity reads `total`).
  Severity major confirmed. The single-SQL "saving" is illusory.
- **getPrivateMessagesSettings "worse" — KEPT.** N+1 confirmed at db.rs:501-519 (per-target
  `get_social_settings` + per-target `is_friend` SELECT = 2*N), vs upstream's two batched
  IN-list queries (`getSocialSettings` + `getFriendsFromList`, get-private-messages-settings.ts:36-39).
  Genuine structural regression.
- **upsertFriendship "same" — corrected to slightly-better-but-tradeoff.** Ours actually skips
  upstream's `isFriendshipBlocked` read AND `registry.getProfile` hydration, so it issues fewer
  queries — but that skip is exactly the missing-block-enforcement + blank-profile shape gap.
  Core tx is equivalent; net it is not "more expensive," so "worse" would be wrong; "same" is
  acceptable.

## Cosmetic / low-impact diffs (real but clients ignore)

- **`page` off-by-one** (0-based `offset/limit` vs upstream 1-based `getPage`). Verified real
  (`pagination.ts:3`). **Neither explorer reads `pagination_data.page`** — bevy and Unity only
  read `total`. No client impact. (Kept in table for completeness; not escalated.)
- **`message` None vs `''`** in friendship requests. Proto field is `optional string`, so it is
  wire-distinguishable; bevy reads it as `Option<String>`. But an empty message vs absent
  message has no behavioral consequence in either client. Cosmetic.
- **Fallback LiveKit URLs** (gatekeeper.rs:66-67) and **deferred gatekeeper participant
  actions** (treated as OK on 404/501): response *shape* is identical; the side effect may be a
  no-op while catalyrst-comms voice routes are 501-deferred. Behavior gap, not shape gap.
