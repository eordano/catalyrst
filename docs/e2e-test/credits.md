# E2E Test Plan — catalyrst-credits (credits.decentraland.org)

| | |
|---|---|
| **Key** | `credits` |
| **Crate** | `catalyrst-credits` |
| **Port** | `5143` (`HTTP_SERVER_HOST=127.0.0.1`, `HTTP_SERVER_PORT=5143`) |
| **Workspace** | `<WORKSPACE>` |
| **Upstream host** | `https://credits.decentraland.org` (Marketplace Credits program) |
| **DB** | dedicated `credits` DB on a PostgreSQL instance (`CREDITS_PG_CONNECTION_STRING`) |
| **Auth** | every route is SignedFetch: AuthChain in `x-identity-auth-chain-{n}` + `x-identity-timestamp` + `x-identity-metadata` headers; signer recovered via `catalyrst-crypto::verify_auth_chain` |

Routes (from `crates/catalyrst-credits/src/main.rs`):
`GET /ping`, `POST /users`, `GET /users/{wallet_id}/progress`, `GET /seasons`,
`GET /captcha`, `POST /captcha`.

---

## 1. Unity config — where to repoint the host

**File:** `unity-explorer/Explorer/Assets/DCL/NetworkDefinitions/Browser/DecentralandUrlsSource.cs`
**Enum:** `DecentralandUrl.MarketplaceCredits` (defined in
`Explorer/Assets/DCL/Infrastructure/Utility/DecentralandUrls/DecentralandUrl.cs`).

**Line 218 — exact current mapping:**

```csharp
DecentralandUrl.MarketplaceCredits => $"https://credits.decentraland.{ENV}",
```

(Line 219, `GoShoppingWithMarketplaceCredits`, is just a marketplace-browse deep
link — leave it alone.)

### This is NOT /about-discovered

`MarketplaceCredits` is resolved entirely inside the static `RawUrl(...)` switch.
In `Url(...)` (lines 101–127) the only branch that defers to the realm is
`CacheBehaviour.REALM_DEPENDENT` → returns `<REALM_DEPENDENT>`; `MarketplaceCredits`
has no such caching behavior, so it falls through to the default branch which
just does `urlData.Url.Replace(ENV, decentralandDomain)` where `{ENV}` is the
domain tld (`org` / `today` / `zone`). The credits host therefore comes purely
from this Unity switch — editing our realm `/about` will **not** change it.

### How to repoint to our local host

Hardcode the override in the switch (the URL must be a literal — there is no
`{ENV}` to substitute when you point at a fixed host):

```csharp
DecentralandUrl.MarketplaceCredits => "http://127.0.0.1:5143",
```

Notes:
- No trailing slash and no path — the crate serves routes at host root
  (`/users`, `/seasons`, `/captcha`, ...), matching the upstream layout.
- `GET /seasons` and the credits endpoints are read via `GatewayUrlsSource.cs:60`
  (which references `DecentralandUrl.MarketplaceCredits` for the gateway list) —
  repointing the enum is sufficient; no second edit needed.
- For a runtime/no-rebuild override instead of editing source, subclass /
  decorate `DecentralandUrlsSource` and override `RawUrl`/`Url` for this enum,
  or front the call with a `TransformUrl` that rewrites `credits.decentraland.org`
  → `127.0.0.1:5143` — but the switch edit above is the canonical repoint.

---

## 2. Service-level e2e checks (curl / wscat)

Bring the service up first:

```bash
# provision the dedicated DB + roles (one-time bootstrap)
# run your DB bootstrap script to create the `credits` DB and its roles

# run the binary (loads the service's environment file, applies sqlx migrations on start)
cd <WORKSPACE>
set -a; source <ENV_FILE>; set +a
cargo run -p catalyrst-credits
```

Auth: all non-`/ping` routes require a valid AuthChain. Generate signed headers
with `dcl-walk auth-sign` (refclient identity) and capture the
`x-identity-auth-chain-0/1/2`, `x-identity-timestamp`, `x-identity-metadata`
header set into `$AUTH` for reuse. The 401 cases below are run with NO auth
headers to confirm the guard fires before any business logic.

### C1 — health (no auth)
```bash
curl -sS -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5143/ping
```
Expect: `200`.

### C2 — /seasons without auth → 401
```bash
curl -sS -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5143/seasons
```
Expect: `401`, body `{"ok":false,"message":...}`.

### C3 — /seasons with auth → 200 SeasonsData shape
```bash
curl -sS $AUTH http://127.0.0.1:5143/seasons | jq -e \
  '.lastSeason and .currentSeason.season and .currentSeason.week and .nextSeason
   and (.currentSeason.week.weekNumber|type=="number")
   and (.currentSeason.season.maxMana|type=="string")
   and (.currentSeason.season.timeLeft|type=="number")'
```
Expect: `200`; top-level `lastSeason` / `currentSeason{season,week}` / `nextSeason`
(camelCase), `maxMana` is a string, `secondsRemaining`/`timeLeft` numeric.
(Requires seasons/weeks/goals seed data — see "Prereqs / known gaps".)

### C4 — enroll signer in program → 200 empty
```bash
curl -sS -X POST $AUTH -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5143/users
```
Expect: `200`, empty body. Idempotent: re-running upserts `user_program=started`.
Verify in DB: `SELECT * FROM user_program WHERE signer = <signerAddr>;`.

### C5 — progress for own wallet → 200 user/credits/goals
```bash
curl -sS $AUTH "http://127.0.0.1:5143/users/$WALLET/progress" | jq -e \
  '.user.hasStartedProgram==true
   and (.credits.available|type=="number")
   and (.credits.expiresIn|type=="number")
   and (.credits.isBlockedForClaiming|type=="boolean")
   and (.goals|type=="array")'
```
where `$WALLET` == the lowercased signer address from `$AUTH`.
Expect: `200`; returns ONLY `user`/`credits`/`goals` (client merges season+email).
After C4, `hasStartedProgram` is `true`.

### C6 — progress for a DIFFERENT wallet → 403
```bash
curl -sS $AUTH -o /dev/null -w '%{http_code}\n' \
  http://127.0.0.1:5143/users/0x000000000000000000000000000000000000dead/progress
```
Expect: `403` (handler enforces `signer == {wallet_id}`).

### C7 — mint captcha → 200 image/png
```bash
curl -sS $AUTH -D - -o /tmp/credits-captcha.png http://127.0.0.1:5143/captcha \
  | grep -i '^content-type:'
file /tmp/credits-captcha.png
# confirm a challenge row was persisted
psql "$CREDITS_PG_CONNECTION_STRING" -c \
  "SELECT count(*) FROM captcha_challenges WHERE signer = lower('$WALLET');"
```
Expect: `200`, `Content-Type: image/png`, valid PNG magic bytes (placeholder
visual is acceptable), and one fresh row in `captcha_challenges`.

### C8 — claim credits (POST /captcha) → 501 (deferred), after validation passes
```bash
# first mint a challenge (C7) so an ACTIVE challenge exists for the signer
curl -sS -X POST $AUTH -H 'content-type: application/json' \
  -d '{"x":42.0}' -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5143/captcha
```
Expect: `501`. This is the staged accrual/ledger write path — signer recovery,
active-challenge lookup, and `x` validation all run and pass *before* the 501,
so a `400`/`401` here means validation regressed, not the stub.

### C9 — claim with NO active challenge → 400 (validation, not 501)
```bash
# fresh signer that never called GET /captcha
curl -sS -X POST $AUTH2 -H 'content-type: application/json' \
  -d '{"x":1.0}' -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5143/captcha
```
Expect: `400` (no active challenge) — confirms validation precedes the 501 stub.

### C10 — malformed body on POST /captcha → 400
```bash
curl -sS -X POST $AUTH -H 'content-type: application/json' \
  -d '{"x":"not-a-number"}' -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5143/captcha
```
Expect: `400` (serde rejects non-numeric `x`).

### C11 — DTO casing regression guard
`POST /captcha` once the ledger lands MUST emit `credits_granted` (snake_case)
while every other field stays camelCase. Until then, assert the 501; when
implemented:
```bash
curl -sS -X POST $AUTH ... http://127.0.0.1:5143/captcha | jq -e \
  'has("credits_granted") and has("ok") and has("isBlockedForClaiming")'
```
Expect: `credits_granted` present (NOT `creditsGranted`).

---

## 3. Real-client smoke (dcl-bevy / dcl-walk)

The credits program UI lives in the **Unity** client (the `MarketplaceCredits`
struct/DTOs are unity-explorer's), so the meaningful client smoke is via
`dcl-walk` (upstream Unity refclient). bevy-explorer has no credits surface, so
dcl-bevy is not applicable here.

Steps:
1. Apply the line-218 repoint above in a working checkout of `unity-explorer`,
   then build that client.
2. `dcl-walk launch` then `dcl-walk auth-sign` to obtain a signed-in identity.
3. Use the client's UI driver to open the Marketplace Credits panel
   (Backpack/Marketplace → Credits).
4. Observe outbound requests hit `127.0.0.1:5143`:
   ```bash
   sudo ss -tnp | grep 5143      # client connection to the service
   journalctl --user -u catalyrst-credits -f   # once a systemd unit exists
   # or watch the cargo-run stdout for the tracing "catalyrst-credits listening" + per-request spans
   ```
5. Expected client behavior: the credits panel loads the season banner
   (`/seasons`), shows `hasStartedProgram`/available credits/goals
   (`/users/{wallet}/progress`), renders the captcha image (`GET /captcha`),
   and — until the accrual path lands — the claim action surfaces a failure
   (the `POST /captcha` 501). Confirm the read/enroll/captcha-render path is
   fully green; the claim failure is expected and documented below.

OCR/click driving: use `dcl-walk` OCR-click to open the panel and
`dcl-walk shot` to capture the rendered credits UI for the artifact.

---

## Prereqs / known gaps (must close before this plan fully passes)

These are explicitly **not done** by the implementation and will make checks
C3/C5/C7 return empty/zeroed data or the service fail to start until addressed:

1. **Run the bootstrap** — the DB bootstrap has NOT been run, so the `credits` DB
   and its roles do not yet exist on the target PostgreSQL instance. The
   environment file points at a DB role (`<DB_USER>`); re-run the bootstrap to
   mint these (or fresh creds). Without this the binary cannot connect.
2. **No systemd unit** — service is run by hand for now; add a
   `catalyrst-credits.service` unit for the `journalctl` step.
3. **Seed data** — `credits_seasons` / `credits_weeks` / `credits_goals` are
   empty after migration; C3 (`/seasons` shape) and C5 (`goals` array) need seed
   rows to return meaningful payloads.
4. **POST /captcha accrual** — `credit_ledger` append + grant is stubbed at 501
   (pending the federation write path), so C8 asserts 501 by design; flip C8/C11
   to the success-shape assertions once that lands.
5. **Out of scope** — `PUT /set-email` and `GET /subscription` belong to the
   notifications host (future `catalyrst-notifications`); not served here, no
   check.
6. **Captcha PNG is a placeholder** — dependency-free stored-deflate encoder; C7
   only asserts valid PNG + persisted challenge, not a solvable image.
