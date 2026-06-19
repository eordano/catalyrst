# Parity Report — catalyrst-credits (upstream: credits-server)

Crate: `crates/catalyrst-credits`
Unity client: `unity-explorer/.../MarketplaceCredits/MarketplaceCreditsAPIService/MarketplaceCreditsAPIClient.cs`
Upstream `credits-server` TS: **NOT mirrored on disk** (only `credits-squid-core` and archived `marketplace-credits-landing` exist; neither contains the captcha/claim REST path). All efficiency verdicts are therefore `unknown` — no structural comparison is possible against an absent source.

## Per-endpoint table

| Endpoint | Shape | Efficiency | Severity | Notes |
|---|---|---|---|---|
| POST /users | match | unknown | none | Client sends empty body, discards response via `.WithNoOpAsync()`. Our handler is one idempotent upsert (`ON CONFLICT (address) DO UPDATE`). No contract to violate. |
| GET /users/{wallet_id}/progress | match | unknown | none | Client overwrites season/email fields from `/seasons` and notifications calls; only keeps `user.hasStartedProgram`, `credits`, `goals`. Goals fetched in a single left-join (no N+1). |
| GET /seasons | match | unknown | none | `SeasonsData` field-for-field match. `maxMana` emitted as string to match `public string maxMana`. No upstream cache to compare; our no-cache recompute is a candidate weakness but unverifiable. |
| GET /captcha | match | unknown | none | Raw `image/png` bytes; client only needs decodable image for `Texture2D.LoadImage`. PNG is a placeholder render but the wire contract holds. |
| **POST /captcha** | **divergent** | unknown | **breaks-client** | **Handler returns 501 and never emits `ClaimCreditsResponse`; client has no try/catch so the claim flow throws.** See below. |
| GET /ping | unknown | unknown | none | Local liveness route only, not in `api_router`, no upstream counterpart, not called by Unity. Nothing to compare. |

## Confirmed shape issues

### POST /captcha — DEFERRED 501, breaks the claim flow (VERIFIED, breaks-client)

Verified end to end:

- **Input shape OK.** `ClaimCreditsBody { x: f64 }` (`dto.rs:104-107`) matches Unity `ClaimCreditsBody { public float x; }` (`ClaimCreditsBody.cs:6-9`).
- **Response struct is shape-correct but unreachable.** `ClaimCreditsResponse { ok, credits_granted (snake_case, no rename), is_blocked_for_claiming (rename → isBlockedForClaiming) }` (`dto.rs:111-117`) matches Unity `ClaimCreditsResponse { bool ok; float credits_granted; bool isBlockedForClaiming; }` (`ClaimCreditsResponse.cs:8-10`) exactly. The snake_case `credits_granted` is a deliberate match for the literal C# field name — confirmed by reading both. This is correct *if* the struct were ever serialized.
- **It is never serialized.** `claim()` (`captcha.rs:67-102`) validates the signer and the active-challenge `x` (±4.0px), then unconditionally returns `ApiError::not_implemented(...)` (`captcha.rs:99-101`). `not_implemented` maps to HTTP 501 with body `{ok:false, message}` (`http.rs:46-47`, `http.rs:64`, `http.rs:72`).
- **Client cannot absorb the 501.** `ClaimCreditsAsync` (`MarketplaceCreditsAPIClient.cs:108-117`) does `SignedFetchPostAsync(...).CreateFromJson<ClaimCreditsResponse>(WRJsonParser.Unity)` with **no try/catch**. A 501 is a non-2xx in the web-request layer and surfaces as `UnityWebRequestException` before any JSON is parsed, so the user-facing credit-claim action fails.

Severity: **breaks-client**. This is the only divergence; it is the entire purpose of the captcha widget (solve → claim credits), so it is not a cosmetic gap.

## Confirmed efficiency wins

None confirmed. Every efficiency verdict is `unknown` because the upstream `credits-server` TS is not on disk, so no claim of "better"/"worse" can be substantiated against a real implementation. Notable structural facts (not wins, just observations):

- GET /users/{wallet_id}/progress fetches goals + per-user progress in **one** left-join (`ports/credits.rs:100-134`) — no N+1 over goals. But with no upstream to compare, this is not a confirmed *win*.
- GET /seasons does up to 4 sequential `LIMIT 1` SELECTs with **no caching** despite near-static season metadata — a potential weakness, not a win.
- POST /captcha currently does only 1 SELECT then short-circuits to 501; the accrual/ledger write transaction does not exist yet (`captcha.rs:96-98` TODO), so there is literally nothing to compare.

## Rejected during verification

Nothing was rejected. The single flagged finding (POST /captcha 501) was independently re-derived from source and holds:

- The 501 mapping is real (`http.rs:64`).
- The handler genuinely never reaches a success branch (`captcha.rs:99-101` is an unconditional return).
- The client genuinely has no error handling around the call (`MarketplaceCreditsAPIClient.cs:113-114`).
- The "upstream not on disk" premise underpinning all `unknown` efficiency verdicts was confirmed: no `credits-server` clone exists, and `credits-squid-core` / `marketplace-credits-landing` contain no captcha/claim path.
