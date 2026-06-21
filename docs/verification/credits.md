# Verification: catalyrst-credits (service "credits")

Adversarial re-check of the parity findings against the committed tree
(`crates/catalyrst-credits`, branch `feat/service-plane-crates`), the Unity C#
consumer (`Explorer/Assets/DCL/MarketplaceCredits/`), and the net-catalog.

Upstream `credits-server` is **not mirrored** on this machine (only
`credits-squid-core` and `marketplace-credits-landing` exist). Therefore exact
upstream status/body parity is unverifiable from source; shapes are inferred
from the C# DTOs, which our success bodies match. This is stated honestly in
the findings and I confirm it.

All five routed endpoints (`POST /users`, `GET /users/{walletId}/progress`,
`GET /seasons`, `GET /captcha`, `POST /captcha`) appear in the net-catalog as
genuinely called by `MarketplaceCreditsAPIClient.cs`. None are dead.

## Per-endpoint table

| Endpoint | Shape | Client reaction | Severity | Failure-modes-ok | Notes |
|---|---|---|---|---|---|
| GET /users/{walletId}/progress | match (confirmed) | ok (confirmed) | none | mostly — DB-down mislabeled | Body matches `CreditsProgramProgressResponse` C# struct; extra C# fields (lastSeason/currentSeason/currentWeek/nextSeason/user.email/isEmailConfirmed) are filled client-side from `/seasons` + notifications `/subscription`, never expected here. Graceful zero/empty degradation confirmed (users.rs:40-69). |
| GET /seasons | "divergent" claim REJECTED (cosmetic / not server-caused) | degraded → reclassified ok | none (was minor) | mostly — DB-down mislabeled | The empty-`state` vs null divergence does NOT exist as described: JsonUtility never yields null strings, so the client `??=` sentinel cannot fire whether we send `""` or omit/`null`. Intrinsic to JsonUtility, not introduced by our default. |
| GET /captcha | match (confirmed) | ok (confirmed) | none | mostly — DB-down mislabeled | Hand-rolled valid PNG; client reads raw bytes via `ExposeDownloadHandlerAsync` + `Texture2D.LoadImage`. No JSON, no null-crash. |
| POST /captcha | match (confirmed); impl FULL, not 501 | ok, with one real UX divergence | minor | wrong-answer mode mislabeled | `ClaimCreditsResponse{ok, credits_granted (snake), isBlockedForClaiming}` matches C# exactly. Transactional claim confirmed (ports/credits.rs:201-263), 200 not 501 — ROUTES.md "deferred/501" note is stale. BUT wrong-answer returns 400, defeating the client's `ok==false` UX branch. |

## Confirmed (true) parts of the findings

1. **Success-body shapes all match the C# DTOs.** Verified field-by-field
   against `CreditsProgramProgressResponse.cs`, `ClaimCreditsResponse.cs`,
   `ClaimCreditsBody.cs`. `credits_granted` is deliberately snake_case in both
   Rust (dto.rs:99) and C# (ClaimCreditsResponse.cs:9).
2. **POST /captcha is fully implemented, not 501** (`claim_credits`,
   ports/credits.rs:201-263: FOR UPDATE, zero `available`, insert
   `credit_ledger`, commit). Stale "deferred/501" doc note confirmed stale.
3. **Captcha solve gated** on active non-expired challenge (captcha.rs:64-80)
   and `|answer - x| <= 4.0` (captcha.rs:82).
4. **Graceful zero-degradation** of `/progress` for unenrolled users:
   `hasStartedProgram:false`, zeroed credits, empty goals, 200 not 404
   (users.rs:36-77).
5. **Startup panic-free.** config.rs requires only
   `CREDITS_PG_CONNECTION_STRING` (host/port default 127.0.0.1:5150);
   build_state (lib.rs:41-58) returns Err on bad env / unreachable DB / failed
   migration; main.rs propagates and exits cleanly; bundle degrades to
   `credits=down` + 404 on its routes.
6. **Uniform error model** (http.rs:57-73): every ApiError →
   `{"ok":false,"message":...}`; sqlx → 500 "database error" redacted;
   AuthChainError → 401.

## REJECTED finding: GET /seasons "divergent" (state="" vs null)

The finding classes this `degraded/minor`: our empty-string `state` default
allegedly "defeats the client `??=` sentinel", rendering `""` instead of
`NO_DATA`.

**Rejected as not a server-caused divergence.** The client deserializes with
`WRJsonParser.Unity` = `JsonUtility` (MarketplaceCreditsAPIClient.cs:78).
`JsonUtility` **never produces null string fields** — absent OR `null` JSON
strings deserialize to `""`. Therefore:

- `result.lastSeason.state ??= NO_DATA_STATE` (line 80) and
  `result.currentSeason.season.state ??= ...` (line 81) **cannot fire** for a
  JsonUtility struct — the field is already non-null `""`.
- Upstream sending `null` would land identically as `""`.

The predicted mislabel-to-`""` outcome occurs **regardless of our
implementation**; it is intrinsic to JsonUtility, not a parity defect of our
service, and no server-side fix is correct. The finding's own note
("JsonUtility cannot distinguish omitted from \"\"") undercuts its severity.
(`nextSeason.state` is recomputed client-side from `startDate` emptiness at
line 82, so our value there is irrelevant — the finding agrees.)

## CORRECTED finding: "no try/catch → exception aborts the panel" is WRONG

The findings assert `ok:false` for "DB down" on `/progress`, `/seasons`,
`GET /captcha`, `POST /captcha`, reasoning that the client method has no
try/catch so a thrown `UnityWebRequestException` aborts the refresh.

It is true the *methods* in `MarketplaceCreditsAPIClient.cs` have no internal
try/catch and that a non-2xx throws: `WebRequestController.cs:77` `SendRequest`
throws on HTTP error and rethrows at line 113 (SuppressErrors /
IgnoreIrrecoverableErrors not set here); `CreateFromJson` (body parse) only
runs on success, so our error JSON is indeed never read on these paths.

**But every call site wraps the call in `try { } catch (Exception)`:**

- `GetProgramProgressAsync` (→ /progress + /seasons + /subscription): caught at
  `MarketplaceCreditsMenuController.cs:347` (logs "error loading the Credits
  Program") AND `MarketplaceCreditsWelcomeSubController.cs:107`.
- `GenerateCaptchaAsync` (GET /captcha): caught at
  `MarketplaceCreditsGoalsOfTheWeekSubController.cs:175` → `SetCaptchaAsErrorState`.
- `ClaimCreditsAsync` (POST /captcha): caught at
  `MarketplaceCreditsGoalsOfTheWeekSubController.cs:212` → `SetCaptchaAsErrorState`.

So a non-2xx does **not** crash and does **not** abort anything uncaught — it
degrades to a logged error / captcha-error UI state. The `ok:false` "DB down"
labels across all four endpoints are **inaccurate** and should be
`ok:true (degraded, caught)`. **No client null-crash and no uncaught-throw risk
anywhere in this surface.**

## CONFIRMED new divergence: POST /captcha wrong-answer status code

A wrong captcha answer returns **400** (captcha.rs:82-83), which throws and is
caught generically at `GoalsOfTheWeekSubController.cs:212` →
`SetCaptchaAsErrorState(true, isNonSolvedError: false)` + generic "error
claiming the credits" message.

But the client has a dedicated **wrong-but-200** branch
(`GoalsOfTheWeekSubController.cs:203-208`): when `ok == false &&
!isBlockedForClaiming` it sets `isNonSolvedError: true` — the purpose-built
"captcha not solved correctly" state. Our 400 prevents that branch from ever
running, so a wrong answer surfaces as generic "try again" instead of the
intended "captcha incorrect" UX. Strongly implies upstream returns
**200 {ok:false}** for a failed captcha rather than 400. Severity **minor**
(UX-only, no crash). Recommend POST /captcha return 200 {ok:false} on wrong
answer.

## Other observations (out of strict parity scope)

- **Captcha range mismatch (latent functional bug, our own logic):** client
  sends `captchaValue * 100` (0..100) (GoalsOfTheWeekSubController.cs:186), but
  our answer is `seed % 160` (0..159) over a 160px image (captcha.rs:1,4-6).
  When the white column lands in 100..159 the user can never get within
  tolerance 4.0 (max client x=100), so ~37% of challenges are unsolvable. Pure
  invented captcha math (upstream unmirrored), not shape-parity, but it would
  break real claims. Flag for follow-up.
- **/seasons signer unused** (seasons.rs:16): valid auth-chain for any wallet
  passes; no wallet in the path, low concern, matches finding.

## Failure-mode gap summary

- All "DB down → 500, request throws / panel aborts" entries are **overstated**:
  the throw is caught at every call site → logged error or captcha-error UI.
  No abort, no crash.
- POST /captcha wrong-answer returns 400 where the client expects 200
  {ok:false} to drive `isNonSolvedError:true`. Real (minor) UX divergence.
- Auth 401/403 reject paths correct and caught client-side.
