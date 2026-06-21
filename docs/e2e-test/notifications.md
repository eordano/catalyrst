# E2E test plan — catalyrst-notifications (key=`notifications`)

Reimplementation of **notifications.decentraland.org** (the push-notifications +
email-subscription backend the Unity client polls), shipped as the
`catalyrst-notifications` crate.

- Crate: `catalyrst-notifications`
- Workspace: `<WORKSPACE>`
- Listen port: `5143` (`HTTP_SERVER_PORT`; host `HTTP_SERVER_HOST`, see `src/config.rs`)
- Backing store: a PostgreSQL instance, **dedicated `notifications` DB**
  (owns its schema; `sqlx::migrate!` runs `migrations/0001_initial.sql` at boot).
  - tables: `notifications`, `subscriptions`, `subscription_opt_outs`
  - config env: `NOTIFICATIONS_PG_CONNECTION_STRING` (point at your PostgreSQL
    instance, e.g. via a unix socket in `<SOCKET_DIR>` or a TCP `5433`)
  - no Redis / S3 / SQS.
- Auth: every route recovers the EVM address from the signed-fetch auth-chain
  (`src/auth_chain.rs`, copied verbatim from catalyrst-communities → reuses
  `catalyrst-crypto::verify_auth_chain` + `catalyrst-types`). The address is
  **never in the URL** — all queries are scoped to the recovered `signer`.
- Build status: `cargo check -p catalyrst-notifications` passes clean (no
  warnings).

Reader-only in v1: the client lists + marks notifications and edits its
subscription; it never writes new notifications. Notification rows are seeded by
external writers (the ingestion/writer path is deferred). The `/set-email`
mailer is stubbed (stores `unconfirmed_email` + a token, no SMTP/SQS send).

---

## 1. Unity config — how to repoint this host

### Finding
There is exactly **one** `DecentralandUrl` enum for this host:

```
<UNITY_EXPLORER>/Explorer/Assets/DCL/Infrastructure/Utility/DecentralandUrls/DecentralandUrl.cs:95
    Notifications = 60,
```

Its `RawUrl(...)` mapping (the single line to repoint) is at:

```
<UNITY_EXPLORER>/Explorer/Assets/DCL/NetworkDefinitions/Browser/DecentralandUrlsSource.cs:220
    DecentralandUrl.Notifications => $"https://notifications.decentraland.{ENV}",
```

This is the **API base URL** (not a website deep-link). The client appends
`/notifications`, `/notifications/read`, etc. Consumers in-client:

- `Notifications/NotificationsRequestController.cs` — `GET …/notifications?limit=50`
  (initial) and the 5s poll `GET …/notifications?onlyUnread=true&from={ms}`;
  `PUT …/notifications/read`. All **signed** (`WebRequestSignInfo.NewFromUrl`).
- `Communities/CommunitiesDataProvider/CommunitiesDataProvider.cs:41` —
  `subscriptionsBaseUrl` → `/subscription`, `/subscription/opt-outs/...`.
- `MarketplaceCredits/.../MarketplaceCreditsAPIClient.cs:25` —
  `emailSubscriptionsBaseUrl` → `/set-email` (the `isCreditsWorkflow` path).

### How to repoint (edit Unity — NOT /about-discovered)
This value is hard-coded in `RawUrl`; it is **not** discovered from the realm's
`/about` document. (`/about` only drives the `RealmDependent` entries —
`Lambdas`/`Content`/`EntitiesActive`/`EntitiesDeployment` at lines 242-253 of
`DecentralandUrlsSource.cs`. `Notifications` is a `CacheBehaviour.STATIC` entry
via the implicit `string`→`UrlData` conversion at lines 274-275.) **Do not edit
`/about` to move notifications — it has no effect.**

Edit line 220 of `DecentralandUrlsSource.cs`:

```csharp
// before
DecentralandUrl.Notifications => $"https://notifications.decentraland.{ENV}",
// after (example: local catalyrst-notifications)
DecentralandUrl.Notifications => "http://127.0.0.1:5143",
```

`{ENV}` is replaced at runtime with the lowercased env name (`org`/`zone`/`today`);
a literal localhost URL drops `{ENV}` entirely. STATIC ⇒ picked up on next
launch, no realm-change invalidation needed.

### Caveat: the Gateway transform + signed-fetch payload
`DecentralandUrl.Notifications` is in `GatewayUrlsSource.SUPPORTED_URLS`
(`Browser/GatewayUrlsSource.cs:61`, "Notification partially required signed
fetch"). When the `USE_GATEWAY` feature flag is on **and** the env is `Org`/`Zone`,
the URL is rewritten to `https://gateway.decentraland.{env}/notifications/...`.
Two consequences:

1. A literal `http://127.0.0.1:5143` has no `.`-delimited subdomain to hoist, so
   `TransformToGateway` returns it unchanged — fine. But to be safe and avoid the
   gateway path entirely while testing, **disable the `USE_GATEWAY` feature flag**
   (or test in an env not in `SUPPORTED_ENVS`).
2. The client signs the request over `urlsSource.GetOriginalUrl(url)` (the
   *un-gateway-ed* URL) with method `get`/`put`. Our `auth_chain::require_signer`
   builds the payload from the **request method + path only** (`get:/notifications`),
   lowercased — it does not bind the host/query. So a host repoint does not break
   signature verification. The 5-minute timestamp window still applies.

---

## 2. API e2e checks (curl) against the local service

```bash
BASE=http://127.0.0.1:5143
```

> Auth note: 7 of the 9 routes require a valid signed-fetch auth-chain. The curl
> checks below split into (a) **unauthenticated negative-path** checks that need
> no signature (assert the service rejects cleanly, never 5xx), and (b)
> **authenticated** checks that need real `x-identity-auth-chain-{0,1,2}` +
> `x-identity-timestamp` + `x-identity-metadata` headers produced by a signer.
> Generate those with the catalyrst-fed signing helper, or reuse a fixture
> shared with the other catalyrst-* lanes (same auth-chain code).
> The payload signed must be `"{method}:{path}:{ts_ms}:{metadata}"` lowercased,
> with `ts_ms` within 5 minutes of now.

### 2.0 Health — `/ping`
`handlers/ping.rs` returns JSON `{"ok":true}` (note: JSON, **not** text/plain).
No auth.
```bash
curl -fsS -o /dev/null -w '%{http_code} %{content_type}\n' "$BASE/ping"
# expect: 200 application/json
curl -fsS "$BASE/ping" | jq -e '.ok==true'
```

### 2.1 Auth enforcement (negative path, no signature) — all protected routes
Every protected route must reject a missing/invalid auth-chain with a 4xx
(`extract_auth_chain` → `InsufficientLinks`/`MalformedChain`/`MissingTimestamp`)
and **never** 500.
```bash
for m_p in \
  "GET /notifications" \
  "PUT /notifications/read" \
  "GET /subscription" \
  "PUT /subscription" \
  "PUT /set-email" \
  "GET /subscription/opt-outs/community/test-community" \
  "DELETE /subscription/opt-outs/community/test-community" \
  "POST /subscription/opt-outs" ; do
  m=${m_p% *}; p=${m_p#* }
  code=$(curl -s -o /dev/null -w '%{http_code}' -X "$m" \
           -H 'content-type: application/json' -d '{}' "$BASE$p")
  case "$code" in 4*) echo "ok   $m $p -> $code";; *) echo "FAIL $m $p -> $code";; esac
done
# expect: every line "ok ... -> 4xx" (401/400). MUST NOT be 200 or 5xx.
```

### 2.2 `GET /notifications` (authenticated) — list + query params
Provide signed headers for `get:/notifications` (path is bare; query string is
not part of the signed payload). `$AUTH` = the `-H` header bundle.
```bash
# default list: { "notifications": [ ... ] }
curl -fsS $AUTH "$BASE/notifications" | jq -e 'has("notifications") and (.notifications|type=="array")'

# limit cap: limit=500 is clamped to 100 (MAX_LIMIT); never errors
curl -fsS $AUTH "$BASE/notifications?limit=500" | jq -e '.notifications|length<=100'

# unread-only poll shape the client actually sends
curl -fsS $AUTH "$BASE/notifications?onlyUnread=true&from=0" | jq -e 'has("notifications")'

# every item carries id/type/address/timestamp/read/metadata when non-empty
curl -fsS $AUTH "$BASE/notifications?limit=1" \
 | jq -e '.notifications==[] or (.notifications[0]|has("id") and has("type") and has("timestamp") and has("read"))'
```
Empty `{"notifications":[]}` is a PASS (no writer seeded rows yet). To exercise a
non-empty list, seed one row first (section 4) for the test address.

### 2.3 `PUT /notifications/read` (authenticated)
Body `{"notificationIds":["<uuid>", ...]}`; returns `{"updated":n}`. Sign for
`put:/notifications/read`.
```bash
# empty / unknown ids -> updated:0 (idempotent), 200
curl -fsS $AUTH -X PUT -H 'content-type: application/json' \
  -d '{"notificationIds":["00000000-0000-0000-0000-000000000000"]}' \
  "$BASE/notifications/read" | jq -e '.updated==0'

# malformed (non-uuid) id -> 400, not 500
curl -s -o /dev/null -w '%{http_code}\n' $AUTH -X PUT \
  -H 'content-type: application/json' -d '{"notificationIds":["not-a-uuid"]}' \
  "$BASE/notifications/read"
# expect: 400
```

### 2.4 `GET /subscription` (authenticated) — default-on when no row
First call for a fresh signer returns the all-channels-on default
(`SubscriptionDetails::default()`), address = signer, no error.
```bash
curl -fsS $AUTH "$BASE/subscription" \
 | jq -e 'has("address") and has("details") and (.address|ascii_downcase==.address)'
```

### 2.5 `PUT /subscription` (authenticated) — upsert details, then read-back
Body = `SubscriptionDetails` (`ignore_all_email`, `ignore_all_in_app`,
`message_type`). Sign for `put:/subscription`.
```bash
curl -fsS $AUTH -X PUT -H 'content-type: application/json' \
  -d '{"ignore_all_email":true,"ignore_all_in_app":false}' \
  "$BASE/subscription" | jq -e '.details.ignore_all_email==true'

# read-back persists (re-sign for get:/subscription)
curl -fsS $AUTHGET "$BASE/subscription" | jq -e '.details.ignore_all_email==true'
```

### 2.6 `PUT /set-email` (authenticated) — validation + stubbed mailer
Sign for `put:/set-email`. Stores `unconfirmed_email` + token; no SMTP send.
```bash
# valid email -> 200, stored as unconfirmedEmail
curl -fsS $AUTH -X PUT -H 'content-type: application/json' \
  -d '{"email":"player@example.com","isCreditsWorkflow":false}' \
  "$BASE/set-email" | jq -e 'has("address")'

# invalid email (no @) -> 400, not 500
curl -s -o /dev/null -w '%{http_code}\n' $AUTH -X PUT \
  -H 'content-type: application/json' -d '{"email":"nope"}' "$BASE/set-email"
# expect: 400
```

### 2.7 Opt-outs (authenticated) — full community lifecycle
Sign each request for its exact method+path (the community id is in the signed
path for GET/DELETE).
```bash
CID=test-community-123

# create -> 201
curl -s -o /dev/null -w '%{http_code}\n' $AUTH_POST -X POST \
  -H 'content-type: application/json' -d "{\"scope\":\"community\",\"scopeId\":\"$CID\"}" \
  "$BASE/subscription/opt-outs"
# expect: 201

# check opted_out -> { "opted_out": true }
curl -fsS $AUTH_GET "$BASE/subscription/opt-outs/community/$CID" | jq -e '.opted_out==true'

# delete -> 204 No Content
curl -s -o /dev/null -w '%{http_code}\n' $AUTH_DEL -X DELETE \
  "$BASE/subscription/opt-outs/community/$CID"
# expect: 204

# re-check -> { "opted_out": false }
curl -fsS $AUTH_GET "$BASE/subscription/opt-outs/community/$CID" | jq -e '.opted_out==false'

# non-community scope rejected -> 400
curl -s -o /dev/null -w '%{http_code}\n' $AUTH_POST -X POST \
  -H 'content-type: application/json' -d '{"scope":"world","scopeId":"x"}' \
  "$BASE/subscription/opt-outs"
# expect: 400
```

### 2.8 Sanity / regression guards
```bash
# unknown route -> 404, not 500
curl -s -o /dev/null -w '%{http_code}\n' "$BASE/does-not-exist"   # expect 404

# no 5xx anywhere on the unauthenticated surface (auth rejects are 4xx)
for u in /ping /notifications /subscription /set-email \
         /notifications/read /subscription/opt-outs ; do
  code=$(curl -s -o /dev/null -w '%{http_code}' "$BASE$u")
  [ "$code" -ge 500 ] && echo "FAIL $u -> $code" || echo "ok   $u -> $code"
done
```

---

## 3. Real-client smoke step

The Unity client is the real consumer (Bevy/Godot do not poll this API), so drive
the upstream Unity client to exercise the live request flow.

1. Repoint `DecentralandUrl.Notifications` per section 1 (edit line 220 to
   `http://127.0.0.1:5143`) and rebuild the client.
2. Disable the `USE_GATEWAY` feature flag (section 1 caveat) so the client does
   not try to rewrite the host to `gateway.decentraland.*`.
3. Start the local service (section 4) and seed one notification row for the
   wallet you will auth with so the panel is non-empty.
4. Launch the client, then log in with that wallet.
5. Observe the notifications poll: `NotificationsRequestController` fires
   `GET /notifications?limit=50` once, then every 5s
   `GET /notifications?onlyUnread=true&from={ms}`. Confirm via the service log /
   `ss`/access log that requests land on `:5143` and the seeded notification
   renders in the in-client notifications panel.
6. Mark it read in-client → confirm a `PUT /notifications/read` hits the service
   and the row's `read=true`/`read_at` is set (re-query the DB, section 4).
7. (Optional) Open the email-subscription settings UI to exercise
   `GET/PUT /subscription` and `PUT /set-email`.

Pass = the client's signed requests authenticate against the `auth_chain`
(address recovered, 200s), the seeded notification appears, and marking read
updates the row.

---

## 4. Pre-reqs: bring the service up + seed data

These belong to the deployment's integration (NOT in the repo) and must exist
before section 2/3:
- a bootstrap step — mint the service's DB roles, `CREATE DATABASE notifications`,
  GRANT.
- the service's environment file (`<ENV_FILE>`) — `HTTP_SERVER_HOST=127.0.0.1`,
  `HTTP_SERVER_PORT=5143`, `NOTIFICATIONS_PG_CONNECTION_STRING=...` (point at the
  `notifications` DB on your PostgreSQL instance).
- a service unit (`catalyrst-notifications.service`).

Bring up + verify the listener:
```bash
systemctl --user start catalyrst-notifications.service
systemctl --user status catalyrst-notifications.service   # active on 5143
ss -ltnp | grep 5143
```
If `systemctl --user` is unavailable, run the binary directly with the env file
sourced, or set `BASE` to whatever host/port you bound.

Seed a notification (writer path is deferred, so insert directly) — connect to
the `notifications` DB using the connection details from `<ENV_FILE>`:
```sql
INSERT INTO notifications (id, address, type, metadata, timestamp, read, created_at)
VALUES (gen_random_uuid(), '0x<your-test-wallet-lowercase>', 'badge_granted',
        '{"title":"smoke"}'::jsonb, (extract(epoch from now())*1000)::bigint, false, now());
```
Address must be lowercased to match the recovered signer.

---

## 5. Out of scope (deferred in this lane)
- **Notification ingestion/writer path** — explorer is a pure reader/marker; rows
  are seeded externally (section 4 seeds manually for testing).
- **Email confirmation mailer** — `/set-email` stores `unconfirmed_email` + token
  only; no SMTP/SQS send, so there is no confirm-link e2e to run.
- **Gateway HTTP/2 multiplexing** behavior of `gateway.decentraland.*` — out of
  scope; tests disable `USE_GATEWAY`.
- deployment integration artifacts (bootstrap/env/service unit, `@dcl/schemas`
  DTOs) — owned by the deployment, listed in section 4 as pre-reqs.
