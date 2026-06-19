# Verification: catalyrst-badges (service key `badges`)

Adversarial re-check of the prior re-check findings on the **current committed tree**
(branch `feat/service-plane-crates`). Findings list to scrutinize was empty (`[]`),
so this lane confirms (a) the crate-level startup/error-model claims, and (b) that the
four endpoints actually carry no client-facing shape divergence or crash risk.

Verdict: **prior findings are accurate. Zero confirmed shape divergences, zero
client-crash risks on the happy path.** Two latent, data-dependent notes (not protocol
bugs) and one doc/config cosmetic discrepancy are recorded below.

## Sources cross-referenced

- Rust crate: `crates/catalyrst-badges/src/{lib,config,main}.rs`,
  `handlers/badges.rs`, `http/{errors,response}.rs`, `ports/{badges,types}.rs`.
- Bundle mount: `crates/catalyrst-social/src/main.rs` (port 5145).
- Unity consumer: `decentraland/unity-explorer/Explorer/Assets/DCL/BadgesAPIService/`
  (`BadgesAPIClient.cs`, `BadgesResponse.cs`, `LatestAchievedBadgesResponse.cs`,
  `CategoriesResponse.cs`, `TiersResponse.cs`, `BadgesUtils.cs`),
  plus `WebRequests/GenericDownloadHandlerUtils.cs` (`CreateFromJsonOp`).
- Call catalog: `the Unity net-catalog` — all 4 endpoints present,
  `IWebRequestController` GET, no others.
- Upstream TS: **no `decentraland/badges` repo exists** in either mirror tree, so the
  upstream wire shape is unverifiable; parity is asserted only against the Unity DTOs
  (which are the authoritative consumer contract here).

## Per-endpoint table

| endpoint | shape | client-reaction | severity | failure-modes-ok | notes |
|---|---|---|---|---|---|
| GET `/categories` | `{"data":{"categories":[string]}}` — matches `CategoriesResponse.data.categories: List<string>` | reads `badgesResponse.data.categories` directly; happy path fine | none | yes | moka cache (cap 1, TTL 300s). DB error → 500 `{"error":"database error"}` → client request-throws (see below) |
| GET `/users/{address}/preview` | `{"data":{"latestAchievedBadges":[{id,name,tierName,image}]}}` — matches `LatestAchievedBadgesData` | `?? Array.Empty` guards null array; `.data` accessed unconditionally but always present | none | yes | `latest_achieved` clamps `limit.max(0)`, returns top-5 by completed-at desc |
| GET `/users/{address}/badges?includeNotAchieved={true\|false}` | `{"data":{"achieved":[BadgeData],"notAchieved":[BadgeData]}}` — matches `ProfileBadgesData` | `foreach` over `data.achieved`/`data.notAchieved`; Rust always emits arrays (never null) so no NRE | none | yes | bool parse is case-insensitive; missing/garbage query → defaults false (no 400) |
| GET `/badges/{badge_id}/tiers` | `{"data":{"tiers":[{tierId,tierName,description,assets,criteria{steps}}]}}` — matches `TiersData`/`TierData`/`BadgeTierCriteria` | `?? Array.Empty` guards null; `.data` accessed unconditionally | none | yes | **404** `{"error":"no tiered badge found with id: ..."}` when badge absent or `is_tier=false` → client request-throws (intended; matches upstream 404) |
| GET `/ping` | `pong` | not called by client (health only) | none | yes | not in net-catalog; bundle health is `/health` on 5145 |

### Field-level shape confirmation (BadgeData)

Rust `ports/types.rs` serde rename map is byte-for-byte aligned with C# `BadgeData` /
`BadgeProgressData` / `AchievedTierData` / `BadgeAssetsData`:

- `isTier`, `completedAt`, and the full `progress` block (`stepsDone`, `nextStepsTarget`,
  `totalStepsTarget`, `lastCompletedTierAt`, `lastCompletedTierName`,
  `lastCompletedTierImage`, `achievedTiers[{tierId,completedAt}]`) all match.
- C# `nextStepsTarget` is `int?` (nullable); Rust emits `Option<i32>` → `null`. Aligned.
- C# `stepsDone`/`totalStepsTarget` are non-nullable `int`; Rust always emits an `i32`. Aligned.
- `assets` uses `[JsonProperty("2d")]`/`[JsonProperty("3d")]` on the C# side; Rust passes
  the raw `assets` jsonb value straight through, and `tier_image()` reads `2d.normal`. Aligned.
- **Timestamps** (`completedAt`, `lastCompletedTierAt`, `achievedTiers[].completedAt`) are
  emitted by `epoch_ms()` as Unix-epoch-**millisecond strings**. Confirmed this is required:
  `BadgesUtils.FormatTimestampDate` does `long.Parse(timestampString)` then
  `FromUnixTimeMilliseconds` — an ISO/RFC3339 string would throw `FormatException`. The
  crate gets this right.

## Crate-level claims — confirmed

- **Startup is panic-free.** `build_state` (lib.rs:45-67) requires
  `BADGES_PG_CONNECTION_STRING` (config.rs:15), opens a 10-conn pool with
  statement/idle timeouts, then runs `sqlx::migrate!("./migrations")`. Missing env, bad
  URL, DB down, and migration failure all return `anyhow::Err` via `.context(...)` — no
  `unwrap`/`expect`/`panic` on any startup path. Confirmed.
- **Bundle degradation is correct.** In `catalyrst-social/main.rs`, `mount` (lines 61-78)
  catches the `Err`, logs `warn!("member unavailable, serving without it")`, pushes
  `("badges", false)`, and serves the rest of the bundle. `/health` then reports
  `status:"degraded"` with `members.badges:"down"` (string, not bool) and the four badge
  routes are simply never merged → axum default 404. Confirmed. (Minor wording nit vs.
  prior finding: the health value is the string `"down"`, not a boolean.)
- **Standalone exit is clean.** `main.rs:20-21` propagates the `Err` out of `main() ->
  Result<()>`; process exits non-zero without panic. Confirmed.
- **No LiveKit, no other optional config, no auth middleware.** Config has only host/port +
  the one DB URL; router has no auth layer; routes are `auth:none`. Confirmed.
- **Two moka caches are pure memory.** `categories_cache` (cap 1, TTL 300s) and
  `tiers_cache` (cap 512, TTL 300s) have no startup dependency. Per-user reads are
  uncached. Confirmed.
- **Error model.** `ApiError` (http/errors.rs) serializes to the single-field envelope
  `{"error": message}` with: 400 BadRequest (message passthrough), 404 NotFound (message
  passthrough), 500 Database (`tracing::error!` logged, body masked to `"database error"`),
  500 Internal (body = message). Coherent and recoverable. Confirmed.

## Key consumer fact — confirmed (request-throws, NOT null-crash)

`CreateFromJsonOp.ExecuteAsync` (`GenericDownloadHandlerUtils.cs:212-266`) deserializes
the body but does **not** inspect `responseCode`; HTTP-status enforcement happens earlier
in the controller's `SendAsync`, and here `ignoreErrorCodes` is `null` and `suppressErrors`
is `false` for every badges call. Therefore any non-2xx (the 404 from `/tiers`, or any 500)
surfaces as a thrown `UnityWebRequestException` **before** the JSON op runs — the malformed
or masked error body shape (`{"error":...}`) is never deserialized into a DTO. So:

- The single-field error envelope shape is irrelevant to the client (it never parses it).
- The reaction to every error is request-throws, propagated up to the passport
  controller's awaiter. None of the four `BadgesAPIClient` methods wrap the call in
  try/catch, so the exception escapes to the caller (passport module), which is the
  intended degrade path. This is **not** a null-deref crash.

## Confirmed issues

None. All four endpoints' shapes match the Unity DTOs on the committed tree; all are
actually called (net-catalog confirms); error/failure paths degrade rather than panic.

## Client-crash risks

None on the happy path. The only crash surface is the universal request-throws on non-2xx
(by design, matches upstream), which is caught/awaited as degradation rather than a hard
crash, and is not a shape bug.

## Failure-mode gaps / latent notes (data-dependent, not protocol divergences)

1. **`GetProgressPercentage` division-by-zero (latent, data-only).**
   `BadgesUtils.cs:60` computes `stepsDone * 100 / (nextStepsTarget ?? totalStepsTarget)`,
   called from `BadgeInfo_PassportModuleSubController.cs:261` and
   `BadgeDetailCard_PassportFieldView.cs:144`. The crate's `assemble_badge`
   (ports/badges.rs:216-253) yields `totalStepsTarget = 0` AND `nextStepsTarget = null`
   **only** for a `is_tier=true` badge that has an empty `badge_tiers` row set
   (`max().unwrap_or(0)` and the tier-branch `min` over an empty set → `None`). Such an
   orphan tier badge would make the divisor `0` and throw `DivideByZeroException` in the
   passport. This is a DB-integrity / Stage-2 population concern (the seed fixture defines
   tiers for its tier badges), not a wire-shape divergence — flagged so the Stage-2 event
   consumer does not create tier badges with zero tier definitions, or the crate floors
   `totalStepsTarget` to >=1.

2. **Standalone default port collision (cosmetic / deploy-doc).**
   `config.rs:14` defaults `HTTP_SERVER_PORT` to `5147`, which the task's port map assigns
   to the `ab-cdn` bundle; `ROUTES.md` line 7 states port `5141` (the content/lambdas/about
   bundle). Neither matters in practice: badges ships **only** inside the social bundle
   (5145), where the standalone `main.rs` and its default port are unused. No client impact;
   recorded so the standalone default and ROUTES.md are not trusted as deploy truth.

3. **No upstream TS to diff against.** There is no `decentraland/badges` repo in the
   mirrors, so wire parity is asserted solely against the Unity DTOs. The 404 message text
   (`"no tiered badge found with id: ..."`) is a plausible reconstruction but cannot be
   byte-checked against the real service; since the client never parses error bodies, this
   carries no risk.
