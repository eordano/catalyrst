# E2E test plan — catalyrst-economy (transactions-api.decentraland.org)

- **key**: `transactions`
- **crate**: `catalyrst-economy`
- **workspace**: `<WORKSPACE>`
- **port**: `5146` (loopback `127.0.0.1`)
- **upstream**: `transactions-server` behind `transactions-api.decentraland.org`
- **route prefix**: `/v1` (nested) + top-level `/ping`

## Routes shipped

| Method | Path | Notes |
|---|---|---|
| POST | `/v1/transactions` | full checkData pipeline + persist; relay broadcast returns **503** until a Gelato/OZ relayer is provisioned |
| GET | `/v1/transactions/:userAddress` | lowercased lookup; parity-only (Unity does not call it) |
| GET | `/v1/contracts/:address` | collection SQL lookup OR `addresses.json` whitelist; 200/404; parity-only |
| GET | `/ping` | returns `pong` |

Error body shape (all errors): `{ "ok": false, "message": "...", "code": "<errcode>" }`,
with `code` ∈ {`invalid_schema`, `sale_price_too_low`, `invalid_contract_address`,
`invalid_transaction`, `quota_reached`, `high_congestion`, `unknown`}. Status mapping:
400 (schema/sale/contract/simulate), 429 (quota), 503 (congestion + relayer-unavailable),
504 (relayer-timeout), 500 (unknown/db).

---

## 1. Unity config — how to repoint the host

The transactions host is **hardcoded** in the Unity URL registry, **not** discovered from
the realm `/about` response. It is the `MetaTransactionServer` enum.

- **Enum**: `MetaTransactionServer = 78`
  in `Explorer/Assets/DCL/Infrastructure/Utility/DecentralandUrls/DecentralandUrl.cs` (line 129)
  of the unity-explorer checkout.
- **RawUrl mapping** (the line to edit):
  `Explorer/Assets/DCL/NetworkDefinitions/Browser/DecentralandUrlsSource.cs` **line 170**:

  ```csharp
  DecentralandUrl.MetaTransactionServer => $"https://transactions-api.decentraland.{ENV}/v1/transactions",
  ```

**Important**: this enum's URL template already includes the **full path** `/v1/transactions`
(unlike most other enums, which are bare hosts). The service nests the route under `/v1`
and exposes `POST /v1/transactions`, so the repointed value must end in `/v1/transactions`
pointing at your host:

```csharp
DecentralandUrl.MetaTransactionServer => "http://127.0.0.1:5146/v1/transactions",
```

(or `http://<lan-host>:5146/v1/transactions` if driving from another machine / through a
reverse proxy fronting the other catalyrst services). Drop the `{ENV}` interpolation —
a local host has no `.org`/`.zone` env suffix. Because the value is a literal here, you
must edit Unity source directly; **this is NOT /about-discovered** and cannot be changed by
editing the realm `/about` response.

> Sanity note for the editor: the Unity client only ever issues the **POST** to this URL.
> The two GET routes are parity-only and unreachable from the client.

---

## 2. Local service e2e checks (curl / wscat)

Assumes the service is up and listening on `127.0.0.1:5146`. There is **no websocket
surface** (pure REST), so wscat is N/A.

### 2.1 Liveness

```bash
curl -fsS http://127.0.0.1:5146/ping
# expect: body "pong", HTTP 200
```

### 2.2 POST /v1/transactions — schema rejection (400 invalid_schema)

`params` must be exactly two strings; here it is one.

```bash
curl -s -o /dev/body -w '%{http_code}\n' -X POST http://127.0.0.1:5146/v1/transactions \
  -H 'content-type: application/json' \
  -d '{"transactionData":{"from":"0x1111111111111111111111111111111111111111","params":["0xdeadbeef"]}}'
# expect: 400; body {"ok":false,"code":"invalid_schema", "message":"...params...exactly 2 strings..."}
```

Missing `from`:

```bash
curl -s -w '\n%{http_code}\n' -X POST http://127.0.0.1:5146/v1/transactions \
  -H 'content-type: application/json' \
  -d '{"transactionData":{"from":"","params":["0xabc","0xdef"]}}'
# expect: 400; code "invalid_schema"
```

### 2.3 POST /v1/transactions — invalid contract address (400 invalid_contract_address)

Well-formed schema, an address that is neither a known collection (in
`squid_marketplace.collection`) nor whitelisted in `addresses.json`. With `RPC_URL` unset
(default), the gas-price + simulate steps are skipped, so the pipeline proceeds
schema -> salePrice(passes, unwatched contract) -> contractAddress (fails here).

```bash
curl -s -w '\n%{http_code}\n' -X POST http://127.0.0.1:5146/v1/transactions \
  -H 'content-type: application/json' \
  -d '{"transactionData":{"from":"0x1111111111111111111111111111111111111111","params":["0x000000000000000000000000000000000000dead","0x"]}}'
# expect: 400; code "invalid_contract_address"; message contains the offending address
```

### 2.4 POST /v1/transactions — valid contract reaches relay stub (503 unknown)

Use a real whitelisted/collection contract address so checkData passes all the way to the
relay step. Pull a known collection address from the squid DB first (point `psql` at your
PostgreSQL instance and supply the relevant credentials):

```bash
PGPASSWORD=<DB_PASSWORD> psql \
  -h 127.0.0.1 -p 5433 -U <DB_USER> -d marketplace_squid -t -A \
  -c "SELECT id FROM squid_marketplace.collection LIMIT 1;"
# -> use that 0x... as params[0]
```

```bash
COL=0x<collection-from-above>
curl -s -w '\n%{http_code}\n' -X POST http://127.0.0.1:5146/v1/transactions \
  -H 'content-type: application/json' \
  -d "{\"transactionData\":{\"from\":\"0x1111111111111111111111111111111111111111\",\"params\":[\"$COL\",\"0x\"]}}"
# expect: 503; code "unknown"; message "No relayer is provisioned. Validation passed; broadcast is unavailable."
# This proves checkData (schema+salePrice+contract+quota) all PASSED and only broadcast is gated.
```

> NB: `params[1]="0x"` is undecodable calldata, so checkSalePrice passes through (price=None).
> To exercise the sale-price floor, supply real `executeMetaTransaction(...)` calldata wrapping
> a `buy`/`executeOrder`/`placeBid` whose price < `MIN_SALE_VALUE_IN_WEI` (1e18) against the
> Polygon CollectionStore / MarketplaceV2 / BidV2 address — expect **400 sale_price_too_low**.

### 2.5 GET /v1/transactions/:userAddress (parity)

```bash
curl -s -w '\n%{http_code}\n' http://127.0.0.1:5146/v1/transactions/0x1111111111111111111111111111111111111111
# expect: 200; JSON array of {id,tx_hash,user_address,created_at} (likely [] until a relay lands a row)
# verify case-insensitivity: the same call with mixed-case address returns the same rows
```

### 2.6 GET /v1/contracts/:address (parity)

```bash
# known collection / whitelisted -> 200 {"ok":true}
curl -s -w '\n%{http_code}\n' http://127.0.0.1:5146/v1/contracts/$COL
# expect: 200; {"ok":true}

# unknown address -> 404 {"ok":false,...}
curl -s -w '\n%{http_code}\n' http://127.0.0.1:5146/v1/contracts/0x000000000000000000000000000000000000dead
# expect: 404; code "unknown"; message "Address is not valid"
```

### 2.7 Quota gate (429 quota_reached) — optional / write-path dependent

Quota faithfully ports the upstream off-by-design `created_at >= NOW()` window (it only counts
rows created at/after query time), and quota runs **after** the relay-broadcast 503. Until a
relayer is provisioned no rows are inserted, so 429 is effectively unreachable in the current
build. To assert the threshold logic directly, seed `MAX_TRANSACTIONS_PER_DAY+1` rows whose
`created_at` is in the future for one `user_address` and confirm the next POST that passes
checkData returns 429 — defer until the relay path lands and inserts rows.

### 2.8 RPC-gated validators (optional)

With `RPC_URL` set (e.g. `https://rpc.decentraland.org/polygon`) and
`MAX_GAS_PRICE_ALLOWED_IN_WEI` set very low, a valid POST should return **503 high_congestion**.
With a deliberately malformed calldata against a real contract and `RPC_URL` set, expect
**400 invalid_transaction** ("Error simulating transaction: ..."). Leave these disabled by
default; enable only when smoke-testing the gas/simulate gates.

---

## 3. Real-client smoke (Unity refclient)

The relay path is the only client-exercised route, and it currently returns 503 (no relayer).
A real-transaction end-to-end (buy / list / bid succeeding on-chain) is therefore **blocked**
until a Gelato or OZ relayer is provisioned (`RELAY_PROVIDER` + `RELAYER_URL`). Until then the
client smoke is limited to confirming the request reaches the host and the validation verdict
surfaces in-client.

`dcl-bevy` and `dcl-godot` do **not** implement the meta-transaction relay flow against this
enum, so they are not useful here. Use the upstream Unity client (`dcl-walk`):

1. Repoint `MetaTransactionServer` per section 1 (`http://127.0.0.1:5146/v1/transactions`),
   rebuild the Unity client.
2. `dcl-walk launch` then `dcl-walk auth-sign` to get an authed session (see the
   `dcl-explore` skill for headless driving).
3. Trigger a marketplace action that relays a meta-tx (e.g. equip/claim/buy flow that POSTs
   to MetaTransactionServer). Capture the network call.
4. **Current expected result**: the POST hits `127.0.0.1:5146`, checkData runs, and the
   client receives **503 `{code:"unknown"}`** (relayer unavailable). Confirm via the
   catalyrst-economy service logs that the request arrived and the pipeline ran to the relay
   step.
5. **Post-relayer (deferred)**: with `RELAYER_URL` set, repeat and expect **200 `{ok:true,
   txHash:"0x..."}`**, a row in `marketplace.transactions`, and an on-chain tx.

### Maintainer follow-ups before a green client run
- Register + start the service via your process/service manager.
- Provision a Gelato/OZ relayer; set `RELAY_PROVIDER` + `RELAYER_URL` to lift the broadcast 503.
- Stage behind a reverse proxy alongside the other catalyrst services if driving the client
  over a hostname rather than `127.0.0.1`.
