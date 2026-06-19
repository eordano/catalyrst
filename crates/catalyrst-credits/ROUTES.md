# catalyrst-credits routes

Rust port of `credits.decentraland.org` (Marketplace Credits program).
Host root, no path prefix. Listens on the deployment's assigned port (`5143`).
All routes are SignedFetch (AuthChain); the signer is recovered via
`catalyrst-crypto` (no hand-rolled EIP-712). Uses a dedicated `credits` database
on the shared PostgreSQL cluster.

| Method | Path | Handler | Status |
|---|---|---|---|
| POST | `/users` | `users::enroll` (MarkUserAsStartedProgramAsync) — upsert `user_program(signer)=started`, 200 empty | implemented |
| GET | `/users/{walletId}/progress` | `users::progress` (GetProgramProgressAsync) — assemble user/credits/goals; signer must equal `{walletId}` | implemented |
| GET | `/seasons` | `seasons::seasons` (UpdateProgramSeasonsAsync) — last/current(season+week)/next from `credits_seasons`/`credits_weeks` | implemented |
| GET | `/captcha` | `captcha::generate` (GenerateCaptchaAsync) — mint+store a `captcha_challenges` row, return `image/png` bytes | implemented (placeholder PNG) |
| POST | `/captcha` | `captcha::claim` (ClaimCreditsAsync) — validates signer + active challenge + `x`, then **501** | deferred (accrual/ledger pending federation) |

## Admin routes (high-risk financial, bearer-gated)

Spec: `docs/admin-console.md` §4 (catalyrst-credits row). Every route below is
gated by a constant-time bearer compare against `CATALYRST_CREDITS_ADMIN_TOKEN`;
when that env is unset the gate fails closed (403). These are additive and do not
touch the signed-fetch routes above. Every successful mutation is transactional
and writes an `admin_audit` row (migration `0002_admin_audit.sql`). The operator
identity is taken from the trusted `X-Catalyrst-Admin` header (set server-side by
the admin console) and recorded in `admin_audit.actor` for credit/block
mutations (migration `0003_grant_idempotency.sql`). Credit amounts (`amount`,
`maxMana`, `reward`) are decimal **strings**, never JSON numbers, to preserve
MANA-wei precision. Revoke is clamped (never negative).

| Method | Path | Body | Effect |
|---|---|---|---|
| GET | `/admin/seasons` | - | list all seasons |
| POST | `/admin/seasons` | `{name,startDate,endDate,maxMana,amountOfWeeks,state}` | create season (201) |
| PUT | `/admin/seasons/{id}` | same as POST | update season |
| DELETE | `/admin/seasons/{id}` | - | delete season (cascades weeks/goals) (204) |
| GET | `/admin/goals?weekId=` | - | list goals (optionally by week) |
| POST | `/admin/goals` | `{weekId,title,description?,thumbnail?,reward,totalSteps,sortOrder?}` | create goal (201) |
| PUT | `/admin/goals/{id}` | `{title,description?,thumbnail?,reward,totalSteps,sortOrder?}` | update goal |
| DELETE | `/admin/goals/{id}` | - | delete goal (204) |
| POST | `/admin/credits/grant` | `{address,amount,reason?,idempotencyKey?}` | add credits + `grant` ledger row; replayed `idempotencyKey` returns prior result (`replayed:true`) without a 2nd grant |
| POST | `/admin/credits/revoke` | `{address,amount,reason?}` | subtract credits (clamped) + `consume` ledger row |
| POST | `/admin/users/{address}/block` | `{blocked:bool,reason?}` | set `is_blocked_for_claiming` |

Out of scope (notifications.decentraland.org host, future `catalyrst-notifications`):
`PUT /set-email`, `GET /subscription`.

## DTO casing

`ClaimCreditsResponse` uses `credits_granted` (snake_case) per the Unity struct,
while every other field is camelCase. `dto.rs` annotates fields individually.
`GET /users/{walletId}/progress` returns only `user`/`credits`/`goals`; the
client merges season + email fields itself.
