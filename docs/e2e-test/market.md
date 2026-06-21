# E2E test plan — catalyrst-market (key=`market`)

Reimplementation of **market.decentraland.org** (the `marketplace-server` /
`marketplace-api` backend), shipped as the `catalyrst-market` crate.

- Crate: `catalyrst-market`
- Workspace: `<WORKSPACE>`
- Listen port: `5133` (`HTTP_SERVER_PORT`, default in `src/config.rs:22`; host `HTTP_SERVER_HOST`)
- Backing store: a PostgreSQL instance
  - read path: `marketplace_squid` DB, schema `squid_marketplace` (`MARKETPLACE_SQUID_SCHEMA` in `lib.rs`)
  - write/builder path: schema `marketplace`; favorites schema `favorites`
  - config env: `DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING` / `DAPPS_READ_*` / `FAVORITES_*`
  - federation migrations run at boot against the write pool:
    `migrations/0001_federation.sql`, `migrations/0002_trades_schema.sql`
- Build status: `cargo check -p catalyrst-market` passes (dev profile).

---

## 1. Unity config — how to repoint this host

### Finding
There is exactly **one** `DecentralandUrl` enum for this host: `Market = 39`
(`Explorer/Assets/DCL/Infrastructure/Utility/DecentralandUrls/DecentralandUrl.cs:64`).

Its `RawUrl(...)` mapping is at:

```
Explorer/Assets/DCL/NetworkDefinitions/Browser/DecentralandUrlsSource.cs:196
    DecentralandUrl.Market => $"https://market.decentraland.{ENV}",
```

### Important: this is the WEBSITE base, not the API base
`DecentralandUrl.Market` resolves to the marketplace **web dApp** origin and is
only ever used to build browser deep-links opened via `OPEN_URL` /
`URLDomain.FromString(...).AppendDomain`. All four in-client uses are
deep-links, not API calls:

- `Backpack/AvatarSection/AvatarController.cs:78` — opens the marketplace site
- `UI/Sidebar/SidebarController.cs:441` — opens the marketplace site
- `InWorldCamera/PhotoDetail/PhotoDetailUtility.cs:16` — `…/contracts/{0}/items/{1}`
- `Passport/Modules/EquippedItems_PassportModuleController.cs:329` — `…/contracts/{0}/items/{1}`

The unity-explorer client does **not** call the marketplace REST API
(`/v1/catalog`, `/v1/items`, `/v1/orders`, …) through any `DecentralandUrl`
enum — those endpoints are consumed by the marketplace web dApp itself. So
repointing the Unity client only changes which *website* the marketplace
buttons open; it does not exercise catalyrst-market's API surface. API coverage
is validated by the curl checks in section 2.

### How to repoint (edit Unity — NOT /about-discovered)
This value is hard-coded in `RawUrl`; it is **not** discovered from the realm's
`/about` response. To point the client at a local marketplace site, edit line
196 of `DecentralandUrlsSource.cs`:

```csharp
// before
DecentralandUrl.Market => $"https://market.decentraland.{ENV}",
// after (example: local marketplace front-end)
DecentralandUrl.Market => "http://localhost:5133",
```

Note the `{ENV}` token is replaced at runtime with the lowercased environment
name (`org`/`zone`/`today`); a literal localhost URL drops `{ENV}` entirely.
Because the enum is `CacheBehaviour.STATIC` (the implicit `string`->`UrlData`
conversion at line 274-275), the change is picked up on next launch with no
realm-change invalidation needed.

**`/about` is irrelevant for this host.** The realm `/about` document controls
`Lambdas` / `Content` / `EntitiesActive` etc. (the `RealmDependent` entries at
lines 242-253), not `Market`. Do not edit `/about` to move the marketplace.

---

## 2. API e2e checks (curl) against the local service

Set the base once (substitute the port the service is listening on):

```bash
BASE=http://127.0.0.1:5133
```

> If the configured port is already occupied, run a throwaway instance on another
> port using the project's smoke-test script (it accepts a `SMOKE_PORT`
> override) and set `BASE` accordingly.

### 2.0 Healthcheck — `/ping`
`/ping` echoes the request path as `text/plain` (see `handlers/ping.rs`).

```bash
curl -fsS -o /dev/null -w '%{http_code} %{content_type}\n' "$BASE/ping"
# expect: 200 text/plain...
curl -fsS "$BASE/ping"
# expect body: /ping
```

### 2.1 Read path (squid) — catalog / items / collections / contracts
```bash
# contracts — JSON object with "data" array
curl -fsS "$BASE/v1/contracts?first=2" | jq -e '.data | type=="array"'

# collections
curl -fsS "$BASE/v1/collections?first=2" | jq -e '.data | type=="array"'

# v1 catalog (wearables/emotes), paginated
curl -fsS "$BASE/v1/catalog?first=2" | jq -e '.data | type=="array" and (length<=2)'

# v2 catalog
curl -fsS "$BASE/v2/catalog?first=2" | jq -e '.data | type=="array"'

# items
curl -fsS "$BASE/v1/items?first=2" | jq -e 'has("data")'

# nfts
curl -fsS "$BASE/v1/nfts?first=2" | jq -e 'has("data")'
```
Expected: HTTP 200; each body is `{ "data": [...], ... }` (marketplace-server
envelope). `jq -e` exits non-zero on shape mismatch.

### 2.2 Accounts / owners / orders / bids / sales / prices
```bash
curl -fsS -o /dev/null -w '%{http_code}\n' "$BASE/v1/accounts?first=2"   # 200
curl -fsS -o /dev/null -w '%{http_code}\n' "$BASE/v1/owners?first=2"     # 200
curl -fsS "$BASE/v1/orders?first=2"  | jq -e 'has("data")'
curl -fsS "$BASE/v1/bids?first=2"    | jq -e 'has("data")'
curl -fsS "$BASE/v1/sales?first=2"   | jq -e 'has("data")'
curl -fsS "$BASE/v1/prices"          | jq -e 'type=="object"'
```

### 2.3 Per-user profile sub-resources
Use a known address (replace `$ADDR`). Endpoints must return 200 even when empty.
```bash
ADDR=0x0000000000000000000000000000000000000000
for p in wearables wearables/grouped wearables/urn-token \
         emotes emotes/grouped emotes/urn-token \
         names names/names-only ; do
  echo -n "$p -> "
  curl -fsS -o /dev/null -w '%{http_code}\n' "$BASE/v1/users/$ADDR/$p"
done
# expect: 200 on every line
```

### 2.4 Aggregations — trendings / rankings / stats / volume / activity
```bash
curl -fsS "$BASE/v1/trendings"                       | jq -e 'has("data")'
curl -fsS "$BASE/v1/rankings/wearables/7d"           | jq -e 'has("data")'   # {entity}/{timeframe}
curl -fsS "$BASE/v1/stats/wearable/sales"            | jq -e 'type'          # {category}/{stat}
curl -fsS "$BASE/v1/volume/7d"                       | jq -e 'type'          # {timeframe}
curl -fsS "$BASE/v1/activity?first=2"                | jq -e 'type'
```
Expected 200; if upstream squid has no rows for the window, an empty
`{"data":[]}` / `{}` is still a PASS (shape, not content).

### 2.5 Trades (read)
```bash
curl -fsS "$BASE/v1/trades?first=2" | jq -e 'has("data")'                    # 200, list

# detail of a non-existent trade id -> 404 (or 200 with null per upstream)
curl -s -o /dev/null -w '%{http_code}\n' "$BASE/v1/trades/0xdeadbeef"        # expect 404

# accept-by-signature for an unknown hash -> 404 / 400, NOT 500
curl -s -o /dev/null -w '%{http_code}\n' "$BASE/v1/trades/0xdeadbeef/accept" # expect 4xx
```

### 2.6 Federation read endpoints
```bash
curl -fsS "$BASE/v1/federation/bids"    | jq -e 'type'
curl -fsS "$BASE/v1/federation/orders"  | jq -e 'type'
curl -fsS "$BASE/v1/federation/trades"  | jq -e 'type'
```

### 2.7 Federation gossip — snapshot / changes
`snapshot` returns a fixed-key digest object (see `handlers/federation.rs:315`);
`changes` is a since/limit cursor feed.
```bash
# snapshot: must contain all five *_seq keys + log_hash + domain
curl -fsS "$BASE/federation/market/snapshot" | jq -e '
  has("latest_bids_seq") and has("latest_orders_seq") and
  has("latest_trades_seq") and has("latest_cancellations_seq") and
  has("latest_acceptances_seq") and has("log_hash") and
  (.domain=="DecentralandMarket")'

# changes: cursor query, since=0 returns the full local log (or empty)
curl -fsS "$BASE/federation/market/changes?since=0&limit=10" | jq -e 'type'
```

### 2.8 Federation write endpoints (signed-fetch gated)
The six `POST /v1/federation/*` routes (`bid`, `bid/cancel`, `bid/accept`,
`order`, `order/cancel`, `trade`) verify an EIP-712 / signed-fetch auth chain
(`src/auth_chain.rs` + `catalyrst-fed` `Eip712Domain` + `RateLimiter`) and
persist intents via `src/fed/*` with a replay log. Negative-path check that
auth is enforced (no valid auth-chain headers => rejected, not 5xx):
```bash
curl -s -o /dev/null -w '%{http_code}\n' -X POST \
  -H 'content-type: application/json' -d '{}' \
  "$BASE/v1/federation/bid"
# expect: 401 or 400 (auth-chain missing/invalid). MUST NOT be 500 or 200.
```
A full positive-path write test requires a signed auth-chain fixture; defer to
the federation integration lane (it shares the signing contract with the other
catalyrst-* services). For this lane, asserting the reject path is sufficient.

### 2.9 Sanity / regression guards
```bash
# unknown route -> 404, not 500
curl -s -o /dev/null -w '%{http_code}\n' "$BASE/v1/does-not-exist"   # expect 404

# malformed pagination -> 4xx, not 500
curl -s -o /dev/null -w '%{http_code}\n' "$BASE/v1/items?first=abc"  # expect 4xx

# no 5xx anywhere on the GET read surface (loop the read endpoints, fail on >=500)
for u in /ping /v1/contracts /v1/collections /v1/catalog /v2/catalog \
         /v1/nfts /v1/items /v1/orders /v1/bids /v1/sales /v1/prices \
         /v1/trendings /v1/activity /v1/trades \
         /v1/federation/bids /v1/federation/orders /v1/federation/trades \
         /federation/market/snapshot ; do
  code=$(curl -s -o /dev/null -w '%{http_code}' "$BASE$u?first=1")
  [ "$code" -ge 500 ] && echo "FAIL $u -> $code" || echo "ok   $u -> $code"
done
```

---

## 3. Real-client smoke step (dcl-bevy / dcl-walk)

Because the Unity client only uses `DecentralandUrl.Market` to **open the
marketplace website** (not the API), a real-client smoke validates the
deep-link wiring, not the catalyrst-market API. Two options:

**A. dcl-walk (upstream Unity client) — deep-link smoke**
1. Optionally repoint `DecentralandUrl.Market` per section 1 (edit line 196) to
   a local origin so the OPEN_URL target is observable, then rebuild.
2. `dcl-walk launch` then `dcl-walk auth-sign` (see the `dcl-walk` tooling docs
   and the `dcl-explore` skill).
3. Open the Backpack (AvatarController) or click a Passport equipped item; the
   client emits an OPEN_URL to `…/contracts/{contract}/items/{item}`.
4. Confirm the emitted URL host equals the repointed value (screenshot/log via
   `dcl-walk`). This verifies the enum repoint, end to end.

**B. dcl-bevy** — the Bevy explorer does not consume this marketplace API path
either; use it only to confirm marketplace deep-links open, same as above
(`dcl-bevy up`, then trigger a marketplace link). If the goal is API-level
verification, prefer the curl suite in section 2.

> Net: the API contract is proven by section 2; the real-client step proves the
> single Unity touch-point (the website deep-link) still resolves after any
> repoint.

---

## 4. Pre-req: bring the service up

Before running section 2, make sure the service is up:
```bash
systemctl --user start catalyrst-market.service
systemctl --user status catalyrst-market.service   # active, bound on 5133
ss -ltnp | grep 5133                              # confirm listener
```
If `systemctl --user` is unavailable, use the systemd-free runner instead:
```bash
SMOKE_PORT=5139 /path/to/catalyrst-market-smoke.sh
# then set BASE=http://127.0.0.1:5139 for section 2
```

## 5. Out of scope
`credits.decentraland.org` SignedFetch API (`DecentralandUrl.MarketplaceCredits`,
enum 58) has no catalyrst crate yet and is a separate target — not part of this
Market lane.
