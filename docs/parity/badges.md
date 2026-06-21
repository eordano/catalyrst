# Parity verification — catalyrst-badges (key=`badges`)

Adversarial re-check of the flagged parity findings for the Rust port of
`badges.decentraland.org`.

- Crate: `crates/catalyrst-badges`
- Unity client DTOs:
  `decentraland/unity-explorer/Explorer/Assets/DCL/BadgesAPIService/`
- Net-catalog: `the Unity net-catalog` +
  `findings-*.jsonl` (4 badge endpoints, all GET, deserialized via Newtonsoft).
- Live diff: not-applicable.

## Critical context: no upstream service to compare against

There is **no upstream `decentraland/badges` source** anywhere in the mirror
(`github.com-decentraland*/`). Exhaustive grep for the TS implementation
(`latestAchievedBadges`/`notAchieved` handlers, any `badge` service/route file)
returns nothing. The crate's own seed comment
(`migrations/0002_seed_fixture.sql`) and `ROUTES.md` both state the
authoritative definitions live in a **private** `decentraland/badges` repo that
is not available here. The only upstream artifact present is the Unity client's
**response DTOs** (`*Response.cs`), which fix the wire shape but reveal nothing
about the server's query strategy.

Consequence: **shape** verdicts are fully verifiable (DTOs + their consumption
sites are on disk). **Efficiency** verdicts framed as "better/worse than
upstream" are **not** verifiable — there is no upstream code to confirm an N+1
or an uncached scan. What is verifiable is the *internal* structure of our
implementation (caching, query counts, over-fetch), which I confirmed by
reading the source. The comparative framing against a hypothetical naive
implementation is recorded as rejected below; the internal structural facts are
recorded as confirmed.

## Per-endpoint table

| Endpoint | Shape | Efficiency (internal facts, verified) | Severity | Notes |
|---|---|---|---|---|
| GET /categories | match | 1 SQL (`SELECT DISTINCT ... ORDER BY category`) behind moka cache TTL=300s, cap=1, key=`()`. Confirmed. | none | Flat `List<string>`; alpha-sorted; nulls filtered. Client reads `data.categories` directly (BadgesAPIClient.cs:42). |
| GET /users/{address}/preview | match | 4 SQL/call (`load_definitions`, `load_all_tiers`, `load_progress`, `load_achieved_tiers`), uncached, two are FULL-table scans, top-5 applied in-memory. Confirmed over-fetch; no N+1. | none | `{id,name,tierName,image}`; image = `assets.2d.normal`. Client null-guards the list (`?? Array.Empty`, line 52). |
| GET /users/{address}/badges | match | 4 fixed SQL via `user_badges()` regardless of badge count = no N+1. `includeNotAchieved=false` filtered in Rust, not SQL; definitions/tiers full-table loaded. Confirmed. | none | Both `achieved` AND `notAchieved` keys mandatory — client iterates both with no null guard (lines 85, 88). Ours always emits both. |
| GET /badges/{badge_id}/tiers | match | Cache hit: 0 SQL (moka cap=512, TTL=300s, key=badge_id). Miss: 2 SQL (existence/404 + ordered select). Confirmed. | none | `{tierId,tierName,description,assets,criteria{steps}}`. 404 on unknown/non-tiered id is error-path, not shape. Client null-guards `tiers`. |
| GET /ping | unknown | static `&str`, no DB | none | No upstream counterpart; not in `api_router`, excluded from social bundle. Out of scope. |

## Confirmed shape findings — all four endpoints MATCH

Verified field-by-field against the actual C# DTOs and, where it matters, against
the *consumption* sites (not just the DTO declarations):

- **/categories** — `{"data":{"categories":[String]}}` ↔
  `CategoriesResponse.data.categories: List<string>`. Match.
- **/preview** — `{id,name,tierName,image}` ↔ `LatestAchievedBadgeData`. `tierName`
  / `image` are `Option<String>` ↔ C# `string` (reference type, null-safe). Client
  reads them via `string.IsNullOrEmpty` (BadgeOverviewItem:50, Overview ctrl:120).
- **/badges** — full `BadgeData` + `BadgeProgress` + `AchievedTier` tree matches
  `BadgeData`/`BadgeProgressData`/`AchievedTierData` exactly (serde renames
  `isTier`/`completedAt`/`tierId`/etc. line up).
- **/tiers** — `TierData{tierId,tierName,description,assets,criteria{steps}}` ↔
  `TierData`/`BadgeTierCriteria{int steps}`. Match; numbers, not strings.
- **assets** — passthrough `serde_json::Value` from jsonb; seed
  (`0002_seed_fixture.sql`) confirms `{"2d":{normal,hrm,baseColor},"3d":{...}}`,
  matching `BadgeAssetsData` with `[JsonProperty("2d")]`/`[JsonProperty("3d")]`.
  Tier rows hold a `{"2d":{normal}}` subset; Newtonsoft fills the rest as null.

### Strengthened nullability point (confirmed load-bearing, not just cosmetic)

The findings called `nextStepsTarget: Option<i32>` ↔ `int?` merely "correct
alignment". It is more than cosmetic: `BadgesUtils.cs:60` computes

```
stepsDone * 100 / (nextStepsTarget ?? totalStepsTarget)
```

so the client relies on `nextStepsTarget` being JSON `null` (not `0`) to fall
back to `totalStepsTarget`. Our `Option<i32>` with no `skip_serializing_if`
emits explicit `null` — correct. Emitting `0` would corrupt the progress
percentage (or divide-by-zero if `totalStepsTarget` were also 0). Conversely
`totalStepsTarget` is non-nullable C# `int` and is divided by directly
(`BadgeDetailCard:149`); our code always produces it (`max(criteria_steps)` or
`1`, never null). Both confirmed correct.

### Mandatory-key point (confirmed)

`BadgesAPIClient.cs:85,88` iterate `data.achieved` and `data.notAchieved` with
**no null guard** — both keys must always be present, even when empty. Our
handler always emits both (`badges.rs:61-64`); when `includeNotAchieved=false`,
`notAchieved` is present-but-empty. Safe. (By contrast `latestAchievedBadges`
and `tiers` ARE null-guarded on the client — lines 52, 74.) No divergence; the
"empty list is fine" claim holds and is backed by the iteration sites.

## Confirmed efficiency facts (internal, structural — NOT comparative)

These are true of *our* implementation, verified by reading the source. They are
NOT confirmations of "better/worse than upstream" (see rejected section).

- **/categories** — single `DISTINCT` query fronted by a 300s moka cache
  (cap=1, key=unit). Near-static derived list served from memory in steady
  state. Real. (lib.rs:33-37, badges.rs:14-19, ports/badges.rs:50-58)
- **/tiers** — 0 SQL on cache hit (moka cap=512, key=badge_id); 2 SQL on miss
  (existence/404 probe + ordered select). The 404 probe runs only on miss, so
  the hot path skips it. Real. (lib.rs:38-41, badges.rs:73-79,
  ports/badges.rs:105-137)
- **/badges** — exactly 4 SQL via `user_badges()` regardless of badge count;
  all merge/assembly in-memory. No per-badge N+1. Real and structurally sound.
  Caveat (also real): `load_definitions`/`load_all_tiers` load the full global
  tables every request, and `includeNotAchieved=false` filters in Rust, not SQL.
  (ports/badges.rs:191-231)
- **/preview** — same 4 queries, uncached (per-user), with the two global
  full-table loads wasteful for a 5-item preview; the top-5 cap is applied
  in-memory after sort (ports/badges.rs:363-368). Over-fetch is real; no N+1.

## Rejected during verification

1. **All "better than upstream" efficiency verdicts** (/categories, /badges,
   /tiers) — rejected *as comparative claims*. There is no upstream badges
   source on disk, so "structurally superior to an uncached/naive scan" cannot
   be confirmed against the real service; it asserts a baseline that was never
   read. The underlying internal facts (we cache, we use a fixed query count,
   no N+1) are confirmed and retained above, but the "better" verdict relative
   to upstream is unverifiable and should not be reported as a parity win
   against the real service.

2. **The "worse" verdict for /preview** — rejected *as a comparative claim* for
   the same reason: it judges our 4-query over-fetch "worse" than a tighter
   upstream that no one can read here. The over-fetch itself (full
   `badge_definitions` + `badge_tiers` loads for a 5-item result, no SQL LIMIT)
   is a real internal inefficiency and is retained as such — but it is an
   internal optimization opportunity, not a confirmed regression versus
   upstream.

3. **No shape diffs rejected** — all four shape verdicts ("match") survived
   field-by-field re-check against the DTOs and their consumption sites. None
   were cosmetic-only or client-ignored; the one I scrutinized hardest
   (`nextStepsTarget` nullability) turned out to be load-bearing and correct.

## Bottom line

Shapes: all four endpoints are genuine matches; no client-affecting divergence.
Efficiency: our internal structure (caching + fixed query counts + no N+1) is
confirmed real; the over-fetch on /preview and /badges is a real internal
opportunity. The "better/worse vs upstream" verdicts cannot be substantiated
because the upstream `decentraland/badges` service is a private repo absent from
this mirror — only the Unity client DTOs are available.
