# E2E Test Plan — `price-feed` (catalyrst-price, api.coingecko.com reimpl)

| | |
|---|---|
| Key | `price-feed` |
| Crate | `catalyrst-price` |
| Port | `5144` (loopback, `127.0.0.1`) |
| Upstream emulated | `api.coingecko.com` — `GET /api/v3/simple/price` |
| Data source | a PostgreSQL instance, `mana_price.price_snapshots`, a read-only DB user, latest `source='coingecko'` row |
| Calls CoinGecko live? | No. A separate mana-price archive process polls CoinGecko; this crate only reads the snapshot table. |
| Workspace | `<WORKSPACE>` |

Routes implemented: `GET /api/v3/simple/price?ids=decentraland&vs_currencies=usd`, `GET /health`. No deferred routes.

---

## 1. Unity config — how to repoint the explorer at this host

The explorer resolves this URL through a **single hardcoded literal** in the
`RawUrl(...)` switch. It is NOT realm-discovered — the value contains no `{ENV}`
interpolation and is not read from the realm `/about` response, so it must be
changed **in Unity**, not by editing our `/about`.

- **Enum definition:** `DecentralandUrl.ManaUsdRateApiUrl`
  in `Explorer/Assets/DCL/Infrastructure/Utility/DecentralandUrls/DecentralandUrl.cs`
- **RawUrl line to edit (line 230):**
  `Explorer/Assets/DCL/NetworkDefinitions/Browser/DecentralandUrlsSource.cs`

  ```csharp
  // BEFORE (line 230):
  DecentralandUrl.ManaUsdRateApiUrl => "https://api.coingecko.com/api/v3/simple/price?ids=decentraland&vs_currencies=usd",

  // AFTER — repoint at catalyrst-price (loopback; the crate replays the exact path + querystring):
  DecentralandUrl.ManaUsdRateApiUrl => "http://127.0.0.1:5144/api/v3/simple/price?ids=decentraland&vs_currencies=usd",
  ```

  Because the literal already pins `ids`/`vs_currencies`, no template-arg
  plumbing changes. The crate honors arbitrary order, comma-separated lists, and
  ignores unknown coins/currencies, so this string can be left byte-identical
  apart from the scheme+host+port.

**Discovery flavor:** NOT `/about`-discovered. This is a hardcoded Unity literal;
editing the catalyst `/about` response has no effect on it. Consumed by
`DCL/Donations/DonationsService.cs`, which deserializes
`Dictionary<string,Dictionary<string,decimal>>` and reads
`response["decentraland"]["usd"]` — so the wire shape must stay
`{"decentraland":{"usd":<number>}}` with a bare JSON number (never a quoted
string). The crate casts the NUMERIC columns to `double precision` in SQL to
guarantee this.

> If your unity-explorer checkout is a read-only mirror, make this edit in a
> writable working copy (or in the dev build the smoke test uses), not in the
> mirror.

---

## 2. Service prerequisites (deferred install)

Before any e2e run the service must be installed and started (sample files ship
inside the crate dir):

```bash
# from the crate dir
cp crates/catalyrst-price/env.example             <ENV_FILE>
cp crates/catalyrst-price/catalyrst-price.service <SYSTEMD_UNIT_DIR>/   # + register the unit
systemctl --user daemon-reload
systemctl --user start catalyrst-price.service
systemctl --user status catalyrst-price.service   # expect active (running)
```

For an ad-hoc run without systemd:

```bash
cd <WORKSPACE>
set -x HTTP_SERVER_HOST 127.0.0.1
set -x HTTP_SERVER_PORT 5144
set -x PRICE_PG_COMPONENT_PSQL_CONNECTION_STRING "postgresql:///mana_price?host=<SOCKET_DIR>&port=5433&user=<DB_USER>&password=<DB_PASSWORD>"
cargo run -p catalyrst-price
```

---

## 3. E2E curl checks (against `127.0.0.1:5144`)

Each check states the expected status and shape. `jq` assertions are written so
they exit non-zero on mismatch (suitable for CI).

### C1 — health
```bash
curl -fsS -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5144/health
```
Expect: `200`.

### C2 — canonical explorer call (the exact ManaUsdRateApiUrl querystring)
```bash
curl -fsS 'http://127.0.0.1:5144/api/v3/simple/price?ids=decentraland&vs_currencies=usd' \
  | jq -e '.decentraland.usd | type == "number" and . > 0'
```
Expect: `200`, body `{"decentraland":{"usd":<number>}}`, `usd` is a **bare JSON
number** (not a quoted string), value > 0 (e.g. `0.06554`).

### C3 — multi-currency (usd,eth,btc)
```bash
curl -fsS 'http://127.0.0.1:5144/api/v3/simple/price?ids=decentraland&vs_currencies=usd,btc,eth' \
  | jq -e '(.decentraland.usd|type=="number") and (.decentraland.eth|type=="number") and (.decentraland.btc|type=="number")'
```
Expect: `{"decentraland":{"btc":<n>,"eth":<n>,"usd":<n>}}`, all three bare numbers.

### C4 — default vs_currencies (omitted ⇒ defaults to usd)
```bash
curl -fsS 'http://127.0.0.1:5144/api/v3/simple/price?ids=decentraland' \
  | jq -e 'has("decentraland") and (.decentraland|keys)==["usd"]'
```
Expect: `{"decentraland":{"usd":<number>}}` (handler defaults `vs` to `usd`).

### C5 — unknown coin is silently omitted (CoinGecko semantics)
```bash
curl -fsS 'http://127.0.0.1:5144/api/v3/simple/price?ids=bitcoin&vs_currencies=usd' \
  | jq -e '. == {}'
```
Expect: `200`, body `{}` (unknown `ids` produce no entry, matching CoinGecko).

### C6 — unknown currency is silently dropped
```bash
curl -fsS 'http://127.0.0.1:5144/api/v3/simple/price?ids=decentraland&vs_currencies=usd,jpy' \
  | jq -e '(.decentraland|keys)==["usd"] and (.decentraland.jpy==null)'
```
Expect: `usd` present, `jpy` absent (mapped to no column).

### C7 — empty ids ⇒ empty object
```bash
curl -fsS 'http://127.0.0.1:5144/api/v3/simple/price?vs_currencies=usd' \
  | jq -e '. == {}'
```
Expect: `200`, `{}` (no ids requested).

### C8 — value-quoting regression guard (DonationsService decimal parse)
```bash
# Must NOT contain a quoted numeric like "usd":"0.065" — DonationsService
# deserializes decimal and would throw on a string.
curl -fsS 'http://127.0.0.1:5144/api/v3/simple/price?ids=decentraland&vs_currencies=usd' \
  | grep -Eq '"usd":[0-9]' && echo OK || { echo "FAIL: value is quoted"; exit 1; }
```
Expect: `OK`.

### C9 — parity against the real upstream (informational, network required)
```bash
diff \
  <(curl -fsS 'https://api.coingecko.com/api/v3/simple/price?ids=decentraland&vs_currencies=usd' | jq 'keys, .decentraland|keys') \
  <(curl -fsS 'http://127.0.0.1:5144/api/v3/simple/price?ids=decentraland&vs_currencies=usd' | jq 'keys, .decentraland|keys')
```
Expect: identical key structure. (Numeric value will differ slightly — CoinGecko
is live; ours is the last-polled snapshot. Assert structure, not equality.)

### Data-freshness sanity (optional, psql)
```bash
psql "postgresql:///mana_price?host=<SOCKET_DIR>&port=5433&user=<DB_USER>&password=<DB_PASSWORD>" \
  -c "select mana_usd, mana_eth, mana_btc, taken_at from price_snapshots where source='coingecko' order by taken_at desc limit 1;"
```
Confirms the row the handler serves and that the archive poller is current.

---

## 4. Real-client smoke (dcl-bevy / dcl-walk)

The donations / MANA-to-USD conversion is a **Unity-explorer** feature
(`DonationsService.cs`); bevy-explorer and godot-explorer do not consume
`ManaUsdRateApiUrl`. So the meaningful real-client smoke is on the Unity client:

1. **Apply the repoint** from §1 in a unity-explorer dev build.
2. Start the service (§2) and confirm C1–C2 pass.
3. Launch the Unity client headlessly and drive it (the `dcl-explore` skill drives
   `dcl-walk`):
   ```bash
   dcl-walk launch
   dcl-walk auth-sign
   ```
4. Open the Donations panel (the UI that surfaces the MANA→USD rate via
   `DonationsService`). With OCR clicks, navigate to a donation flow and assert
   the displayed USD equivalent is non-zero and matches
   `mana_amount * <usd from C2>` within rounding.
5. **Network assertion:** while the client runs, confirm it hits this host and not
   CoinGecko:
   ```bash
   ss -tnp 2>/dev/null | grep ':5144'        # expect an ESTABLISHED conn from the client
   ```
   and that no outbound connection to `api.coingecko.com` is made by the explorer
   process (inspect `dcl-walk` logs for the resolved URL).

> bevy/godot smoke is **N/A** for this lane — no code path reads the rate.

---

## 5. Pass criteria

- C1–C8 all green (C9 structural-only).
- Unity smoke: Donations panel shows a sane USD figure, client connects to
  `:5144`, no CoinGecko egress.
- Wire shape stays `{"decentraland":{"usd":<bare number>}}` (C8 guard).
