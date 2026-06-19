# Parity report — catalyrst-price (service "price")

Lane: `price` · Crate: `crates/catalyrst-price`
Upstream reference: CoinGecko `GET /api/v3/simple/price` (not a Decentraland-owned
service — the Unity explorer calls CoinGecko directly).
Live diff: not-applicable (our content/price server not running by default;
verified statically + against the live `mana_price` archive DB on :5433).

## Per-endpoint summary

| Endpoint | Shape | Efficiency | Severity | Notes |
|---|---|---|---|---|
| `GET /api/v3/simple/price` | match | better | none | Emits `{"decentraland":{"usd":<number>}}` — exactly what `DonationsService.cs` reads as `Dictionary<string,Dictionary<string,decimal>>` then `response["decentraland"]["usd"]`. Served from local 5-min `mana_price` archive instead of live CoinGecko. See caveat: not actually wired into the client. |
| `GET /health` | match | same | none | Util/liveness only; not called by the explorer. Single `SELECT 1`. Standard catalyrst probe convention. |

## Confirmed shape findings

### `GET /api/v3/simple/price` — shape MATCH (verified)

Verified both sides directly:

- **Client read path** (`DonationsService.cs:250-264`,
  `GetCurrentManaConversionAsync`): deserializes
  `Dictionary<string, Dictionary<string, decimal>>` via Newtonsoft and reads
  `response["decentraland"]["usd"]`. No envelope, value must be a JSON number
  coercible to `decimal`.
- **URL constant** (`DecentralandUrlsSource.cs:230`,
  `DecentralandUrl.ManaUsdRateApiUrl`): hardcoded
  `https://api.coingecko.com/api/v3/simple/price?ids=decentraland&vs_currencies=usd`.
  Matches the net-catalog row.
- **Our handler** (`handlers/simple_price.rs`): builds a `serde_json::Map`
  keyed by lowercased `ids` (only `decentraland` is ever emitted) → inner Map
  keyed by lowercased `vs_currencies` (`usd`/`eth`/`btc`) → bare JSON number via
  `Number::from_f64`. Produces `{"decentraland":{"usd":0.06566}}`.

This is an exact structural match: top-level wrapper, no envelope, number (not
quoted string), lowercase `decentraland`/`usd` keys. CONFIRMED.

The two edge cases flagged in the input (NULL `mana_usd` → `{"decentraland":{}}`;
no snapshot at all → `{"decentraland":{}}`) are genuine data-availability gaps,
not serde-shape divergences — the wire grammar is still valid CoinGecko shape,
and live CoinGecko would behave equivalently for missing data (a `decentraland`
key with no `usd` → the same C# `KeyNotFoundException`). Live DB check shows
`mana_usd` is populated and current (latest `coingecko` row 2026-06-09 10:51,
`mana_usd=0.06566`), so neither edge fires in normal operation. Correctly NOT
escalated to a shape issue.

No shape issues to report.

## Confirmed efficiency wins

### `GET /api/v3/simple/price` — EFFICIENCY BETTER (verified, structural)

The "better" verdict is NOT based on language choice. Verified the structural
reason on both sides:

- **Upstream (client)** calls live `api.coingecko.com` on every cache miss.
  CoinGecko's public API is rate-limited (429s); the client mitigates with a
  30-min in-process cache (`MANA_RATE_CACHE_DURATION_MINUTES = 30`,
  `DonationsService.cs:40,252`). Each cold call is exposed to upstream
  rate-limit / availability.
- **Ours** serves from a pre-polled local archive
  (`mana_price.price_snapshots`) via a single
  `SELECT mana_usd::double precision, ... FROM price_snapshots WHERE source='coingecko' ORDER BY taken_at DESC LIMIT 1`
  (`.fetch_optional`, `ports/prices.rs:34-54`). The index
  `idx_price_snapshots_taken_at (taken_at DESC)`
  (`mana-price-archive-schema.sql:23`) drives the ordering; `source` is a
  residual filter.
- The archive is populated every 5 minutes by `mana-price.service`
  (`ExecStart ... run --interval 5`) and the **archive itself** absorbs
  CoinGecko's rate limits — `mana-price-archive.py` has explicit 429 backoff
  (`_fetch_with_retries`, sleeps on 429). So request-serving is structurally
  decoupled from the live external rate-limited API: a per-request single-row
  index lookup against a locally amortized feed.

There is no in-process moka cache here, and none is needed — the upstream
archive already amortizes the CoinGecko fetch across all consumers. The win is
data-source substitution (local pre-aggregated archive vs live rate-limited
external API), which is a real structural difference. CONFIRMED.

**Correctness bonus found during verification** (strengthens the verdict): the
`source = 'coingecko'` filter is an *exact* match, not `LIKE 'coingecko%'`. The
live table also contains `coingecko-historical-daily`, `coingecko-historical-hourly`,
`coingecko-historical-5min`, and `cryptocompare-historical` source tags (stale
backfill rows, max `taken_at` 2026-05-27). An exact match correctly excludes
those; a prefix match would have risked returning a stale historical row as
"latest". The query is correct.

## Caveat (recorded, not a rejection)

The explorer's `ManaUsdRateApiUrl` constant hardcodes
`https://api.coingecko.com/...` and is **not** routed through a configurable
catalyst/realm host. So the default Unity client never hits catalyrst-price —
the service only takes traffic if DNS/proxy redirection points the client at it.
The shape/efficiency parity above is therefore conditional on the request being
routed to us; it is not active in the stock client. This does not change the
shape (match) or efficiency (better-if-routed) verdicts, but the efficiency win
is opt-in rather than live by default.

## Rejected during verification

Nothing was rejected. All input findings survived adversarial re-check:

- Shape `match` for `/api/v3/simple/price` — confirmed against
  `DonationsService.cs` read path and our handler output.
- The two "data-availability, not shape" edge notes — confirmed correct
  (valid grammar; live data is populated so edges don't fire).
- Efficiency `better` — confirmed structural (local archive + archive-side 429
  backoff + 5-min poll), not language-based.
- `/health` match/same — confirmed (no upstream equivalent, trivial `SELECT 1`).
