# E2E test plan — `catalyrst-badges` (key=`badges`)

Rust reimplementation of `badges.decentraland.org`. Read-only profile-badge state.

- Crate: `catalyrst-badges` (workspace `<WORKSPACE>`)
- Port: `5141` (host `127.0.0.1`, env `HTTP_SERVER_HOST` / `HTTP_SERVER_PORT`)
- DB: own `badges` DB on a shared PostgreSQL instance (unix socket `<SOCKET_DIR>`), env `BADGES_PG_CONNECTION_STRING`
- Success envelope: bare `{"data": ...}` (NOT `{ok,data}`). All routes `auth:none`. `{address}` lowercased before lookup.
- Build: `cargo check -p catalyrst-badges` passes clean.

---

## 1. Unity config to repoint this host

The Badges base URL is **NOT** discovered from the realm `/about`. It is a static
template keyed off `DecentralandUrl.Badges` and the `ENV` (`org`/`zone`/`today`)
suffix. Repointing requires a Unity-side edit.

### Canonical edit — `DecentralandUrlsSource.cs`

File: `unity-explorer/Explorer/Assets/DCL/NetworkDefinitions/Browser/DecentralandUrlsSource.cs`

Line **202** (inside the `RawUrl(DecentralandUrl url)` switch):

```csharp
DecentralandUrl.Badges => $"https://badges.decentraland.{ENV}",
```

Repoint to the local service (no trailing slash — `BadgesAPIClient` appends
`/categories`, `/users/{wallet}/preview`, etc.):

```csharp
DecentralandUrl.Badges => "http://127.0.0.1:5141",
```

The client consumes this verbatim:
`unity-explorer/Explorer/Assets/DCL/BadgesAPIService/BadgesAPIClient.cs`
line 21 `badgesBaseUrl => decentralandUrlsSource.Url(DecentralandUrl.Badges)`, then
`{badgesBaseUrl}/categories` (l.37), `/users/{walletId}/preview` (l.47),
`/users/{walletId}/badges?includeNotAchieved={true|false}` (l.59),
`/badges/{badgeId}/tiers` (l.69). These exactly match our four implemented routes.

### Gotcha — GatewayUrlsSource override

`unity-explorer/Explorer/Assets/DCL/NetworkDefinitions/Browser/GatewayUrlsSource.cs`
line **48** lists `DecentralandUrl.Badges` in `SUPPORTED_URLS`. When the gateway
source is active (only for `Org`/`Zone` envs, per `SUPPORTED_ENVS`) it rewrites the
host to a `gateway.` subdomain. For local testing this is bypassed by either
(a) running a non-gateway env, or (b) the hardcoded `http://127.0.0.1:5141` above,
which has no `https://` host segment for the gateway rewrite to target. No edit to
GatewayUrlsSource is required for a localhost repoint; just be aware it can shadow
the base URL on `.org`/`.zone`.

### Enum reference

`DecentralandUrl.Badges` is defined in
`unity-explorer/Explorer/Assets/DCL/NetworkDefinitions/Browser/DecentralandUrl.cs`.
No enum change needed — only the RawUrl mapping value.

**Summary: this host is static-mapped in Unity, NOT /about-discovered. Edit
`DecentralandUrlsSource.cs:202`.**

---

## 2. Bring the service up

```bash
# one-time: mint roles, create DB, grant SELECT, run migrations (incl. seed fixture), write env
bash <WORKSPACE>/scripts/bootstrap-catalyrst-badges.sh

# run (embedded sqlx::migrate! runs 0001_initial + 0002_seed_fixture on startup)
cd <WORKSPACE>
cargo run -p catalyrst-badges

# confirm it is the only new listener on the configured port
ss -ltnp | grep 5141
```

Seed fixture defines badges: `open_for_business`, `decentraland_citizen` (non-tier),
`walkabout`, `emotionista` (tiered). Per-user progress tables
(`user_badge_progress`, `user_achieved_tiers`) are empty until written out-of-band
(Stage-2 ingest worker is deferred), so user routes return empty-ish payloads on a
fresh DB unless you seed a test address.

---

## 3. Concrete e2e curl checks

Use a real seeded badge id and any test address. `0x0000…0001` will exercise the
empty-progress path; substitute a manually-seeded address to exercise populated rows.

```bash
BASE=http://127.0.0.1:5141
ADDR=0x0000000000000000000000000000000000000001

# C1 health — 200, body exactly "pong"
curl -fsS -w '\n%{http_code}\n' "$BASE/ping"

# C2 categories — 200, {"data":{"categories":[...]}} ; should include the seeded categories
curl -fsS -w '\n%{http_code}\n' "$BASE/categories" | tee /dev/stderr | head

# C3 user preview — 200, {"data":{"latestAchievedBadges":[...]}} (empty array on fresh DB)
curl -fsS -w '\n%{http_code}\n' "$BASE/users/$ADDR/preview"

# C4 badges, achieved only — 200, {"data":{"achieved":[...],"notAchieved":[...]}}; notAchieved omitted/empty when false
curl -fsS -w '\n%{http_code}\n' "$BASE/users/$ADDR/badges?includeNotAchieved=false"

# C5 badges, include not-achieved — 200; notAchieved[] populated from seeded definitions
curl -fsS -w '\n%{http_code}\n' "$BASE/users/$ADDR/badges?includeNotAchieved=true"

# C6 tiers for a tiered badge — 200, {"data":{"tiers":[{tierId,tierName,description,assets,criteria{steps}}]}}
curl -fsS -w '\n%{http_code}\n' "$BASE/badges/walkabout/tiers"

# C7 tiers for a non-tiered badge — 200 with empty tiers[] (NOT 404)
curl -fsS -w '\n%{http_code}\n' "$BASE/badges/decentraland_citizen/tiers"

# C8 address case-insensitivity — uppercase address must equal lowercase result (handler lowercases)
curl -fsS "$BASE/users/0x0000000000000000000000000000000000000001/preview" > /tmp/lc.json
curl -fsS "$BASE/users/0X0000000000000000000000000000000000000001/preview" > /tmp/uc.json
diff /tmp/lc.json /tmp/uc.json && echo "case-insensitive OK"

# C9 unknown badge tiers — define expected: empty tiers[] 200 (or 404 per handler) — assert it does not 500
curl -sS -o /dev/null -w '%{http_code}\n' "$BASE/badges/does_not_exist_xyz/tiers"

# C10 envelope shape — top-level key is exactly "data" (no "ok"), assert with jq
curl -fsS "$BASE/categories" | jq -e 'has("data") and (has("ok")|not)' && echo "envelope OK"

# C11 DTO field-name fidelity — assets keys must be literal "2d"/"3d", progress fields camelCase
curl -fsS "$BASE/users/$ADDR/badges?includeNotAchieved=true" \
  | jq -e '.data.notAchieved[0] | (.assets|has("2d") and has("3d")) and (.progress|has("stepsDone") and has("totalStepsTarget") and has("achievedTiers"))' \
  && echo "DTO fields OK"
```

Expected shapes (must match Unity `DCL.BadgesAPIService.BadgeData`):
- `BadgeData` = `{id,name,description,category,isTier,completedAt,assets{2d{normal,hrm,baseColor},3d{...}},progress{stepsDone,nextStepsTarget(nullable),totalStepsTarget,lastCompletedTierAt,lastCompletedTierName,lastCompletedTierImage,achievedTiers[{tierId,completedAt}]}}`
- Timestamps are ISO-8601 with millis.
- `tiers[]` entry = `{tierId,tierName,description,assets,criteria{steps}}`.

No websockets on this service — `wscat` is N/A (all four client calls are plain GET).

---

## 4. Real-client smoke (Unity passport)

Badges render only in the Unity passport UI (`BadgesAPIService` is wired through
`PassportPlugin` / `PassportController`); bevy/godot do not consume this host, so the
real-client step is Unity-only.

1. Apply the `DecentralandUrlsSource.cs:202` repoint above in a working copy of
   `unity-explorer`.
2. Launch the Unity client and authenticate.
3. Open your own passport (own-profile path hits `includeNotAchieved=true`), then a
   second profile (other-profile hits `false`). Confirm the Badges overview tab
   populates from the seeded definitions and does not error.
4. Capture proof: take a screenshot of the passport Badges tab, and check the
   client logs to confirm requests hit `127.0.0.1:5141` with no 4xx/5xx.
5. Negative check: confirm no requests still leave for `badges.decentraland.org`
   (grep logs for the upstream host).

If passport badge data populates from the local fixture with no console errors and
all C1–C11 curl checks pass, the repoint is verified end to end.

---

## 5. Notes / gaps

- Stage-2 event ingest worker (writes `user_badge_progress` / `user_achieved_tiers`)
  is deferred; until then user-specific routes reflect only out-of-band seeded rows.
  To make C3–C5/C11 return non-empty `achieved`/`progress`, INSERT a fixture address
  into those tables before testing.
- Caching: categories + per-badge tiers are moka TTL-cached 300s — after editing
  `badge_definitions`/`badge_tiers`, wait out the TTL or restart before re-asserting
  C2/C6.
- Port note: the crate's port is standardized across `config.rs`, `ROUTES.md`, and
  the bootstrap script. Confirm the configured port is free
  (`ss -ltnp | grep 5141`) before launch.
