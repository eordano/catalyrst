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

Out of scope (notifications.decentraland.org host, future `catalyrst-notifications`):
`PUT /set-email`, `GET /subscription`.

## DTO casing

`ClaimCreditsResponse` uses `credits_granted` (snake_case) per the Unity struct,
while every other field is camelCase. `dto.rs` annotates fields individually.
`GET /users/{walletId}/progress` returns only `user`/`credits`/`goals`; the
client merges season + email fields itself.
