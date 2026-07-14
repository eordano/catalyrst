# explorer-boot request corpus

Status: FULL capture 2026-08-14 -- complete boot into Genesis City + ~7 min
in-world idle, unity-explorer PR #9728 build (macOS artifact; the Windows run
cross-checks identical shapes with `_windows` asset suffixes).

`corpus-interconnected.json` is the candidate-side capture: the same build on
the Windows host with only `--base-domain interconnected.online` (plus
`--skip-minimum-specs-screen --skip-auth-screen true`), booted in-world against
the box. Every client-composed request lands on `*.interconnected.online` /
`gateway.interconnected.online`; the only decentraland.org touches are
data-carried absolute URLs inside synced content (profile avatar snapshots on
`profile-images.decentraland.org`) -- provenance, not resolution.

The request shapes unity-explorer makes against production during boot and
idle, captured live over the client's own CDP bridge
(`--launch-cdp-monitor-on-start true`, ws on 127.0.0.1:1473). 68 unique
shapes from 2564 requests: the surface a `--base-domain` deployment must
answer identically, and the input for a conformance section walking each
entry against baseline `https://<sub>.decentraland.org<path>` vs candidate
`https://gateway.<base-domain>/<sub><path>` (production already gateway-routes
26 of the shapes -- those are captured in routed form; raw-subdomain and
gateway forms are equivalent by construction on catalyrst deployments).

Capture/regenerate with the rig:

```bash
rig/scripts/explorer-cdp-corpus.sh collect ws://127.0.0.1:1473 events.ndjson  # via ssh -L to the player host
rig/scripts/explorer-cdp-corpus.sh normalize events.ndjson corpus.json
```

Launch the player with `--launch-cdp-monitor-on-start true` and attach the
collector before boot (events are only emitted while a listener is connected).
On a machine without Creator Hub installed, seed
`Strings["CreatorHub.BinPath"]` in `userdata_0.json` (any existing exe) first --
otherwise the bridge's browser-open path blocks boot on a modal file picker.

Contract details the corpus pins:

- `feature-flags.<domain>/explorer.json` is fetched with `referer:
  https://decentraland.org` (Unleash hostname strategy) plus `X-Debug` and
  `X-Address-Hash` -- production serves the full flag set (including
  `explorer-alfa-minimum-requirements`) only when the referer matches, so a
  flags endpoint must serve the full production-shaped set without requiring
  the header.
- Signed-fetch endpoints (comms-gatekeeper, social-api, camera-reel, credits,
  notifications, asset-bundle-registry POSTs) carry `x-identity-timestamp` +
  `x-identity-metadata` headers; a conformance runner needs a real identity
  auth chain for those (see rig auth helpers) or must diff status/shape only.
- `_mac` suffixes in ab-cdn asset names are platform-variant
  (`_windows`/`_mac`); a windows capture yields the same shapes with the other
  suffix.
- Out-of-scope external hosts captured for completeness:
  `media.githubusercontent.com` (satellite tiles), `*.colyseus.cloud`
  (exploration games), `docs.google.com`, S3/CDN one-offs. `peer.dclnodes.io`
  is the realm-picked third-party catalyst -- on a custom deployment the peer
  set comes from `/lambdas/contracts/servers`, so it maps to the deployment's
  own peer.

## Source-derived inventory + reconciliation (2026-08-15)

The boot+idle dynamic capture is not the whole client surface. These fixtures
make the request surface COMPLETE (source-derived) and PROVEN (reconciled
against live production), verified by `tests/source_inventory.rs`
(`cargo test -p catalyrst-conformance --test source_inventory`):

- `source-callsites.jsonl` -- the full static inventory: every HTTP/WebSocket
  call site swept from `unity-explorer` (PR #9728 base-domain worktree) across
  five mechanisms (IWebRequestController, UnityWebRequest, WebSockets,
  DecentralandUrl consumers, scene fetch/signedFetch bridges), one row per
  `file:line`.
- `source-inventory.json` -- those call sites deduplicated into request SHAPES,
  each carrying exactly one `disposition`: `corpus` (a walked corpus.json
  entry) | `probe` (a captured synthetic probe) | `browser-only` | `deferred`
  with one of five allowed reasons (`needs-real-wallet`, `desktop-shell-only`,
  `error-path-only`, `documented-intentional-upstream-dependency`,
  `immutable-content-addressed`). `documented-intentional-upstream-dependency`
  is permitted ONLY for the explicit external `allowedUpstreamHosts`
  (YouTube/Thirdweb/Google-Drive/GitHub tiles/CoinGecko/Segment/OpenSea) plus
  the synthetic scene passthrough -- a first-party base-domain/gateway
  obligation can never be hidden as an upstream leak, and the checker rejects
  it.
- `synthetic-probes.json` -- dynamic probe evidence for shapes not in the boot
  corpus: unauthenticated `curl` (+ signed identity via
  `rig/scripts/signed-fetch.sh`, wallet-1 throwaway) paired baseline vs
  candidate. Signed probes prove authenticated parity for the signed-fetch
  reads whose unauthenticated form shows a 400-vs-401 refusal (see
  `accepted-divergences.json`).
- `route-sweep.json` -- machine-checkable route-existence sweep of every
  first-party `needs-real-wallet` shape against the candidate ORIGIN
  (`--resolve <vhost>:443:<origin-ip>`, Cloudflare-bypassing). Reads probe
  unauthenticated GET; writes probe OPTIONS (never mutate). Each form is judged
  independently; `mapping: dual` services must present BOTH the custom-subdomain
  and gateway-prefix form, `subdomain-only` services only the subdomain.
  Cloudflare/WAF responses and 502/503 are `null` = UNPROVEN, never counted as
  present. `route_sweep_has_no_unresolved_gaps` FAILS on any gap -- gaps are
  exact-path fix items for the conformance lane and may not be waived.

Regenerate: `python3 gen-source-inventory.py source-inventory.json` (offline,
from the committed call-site rows + corpus + probes) and
`python3 gen-route-sweep.py` (live, origin-direct -- gates on candidate warmup).

## Accepted divergences (recorded 2026-08-15)

`accepted-divergences.json` is the ledger the explorer section consults before
reporting a status difference: a row matching an entry's exact
(method, host, path, baseline-status, candidate-status) tuple is printed as an
accepted divergence, counted separately from passes and failures. An observed
pair that does NOT match the recorded pair still reports as a real difference,
so the ledger cannot mask a new regression on the same route.

### Auth-shape family: unauthenticated signed-fetch refusal is 401, upstream 400

Upstream services behind `@dcl/platform-crypto-middleware` answer a missing or
invalid auth chain with `400 {"error":"Invalid Auth Chain","message":"This
endpoint requires a signed fetch request. See ADR-44."}`. The catalyrst ports
answer `401` (RFC 9110: missing/invalid credentials), pinned by
`catalyrst-social-service/tests/contract_gate.rs` (`expect(401)` on
signed-fetch routes and the `/v1/mutes` OpenAPI 401 contract),
`catalyrst-notifications/src/http.rs` (401 envelope test), and
`catalyrst-credits/tests/user_credits_route.rs`.

Signed-flow evidence (2026-08-15, rig throwaway identity
`0xe168f6bccd24d90c1d755449c06eae6494d7cb14`, ADR-44 chain via
`rig/scripts/e2e-signed.mts`) -- authenticated behaviour is identical, the
divergence is confined to the unauthenticated error status:

| request | baseline (decentraland.org) | candidate (interconnected.online) |
|---|---|---|
| GET /v1/mutes?limit=100&offset=0 | 200 `{"data":{"results":[],"total":0,"page":1,"pages":0,"limit":100}}` | 200 identical body |
| GET /v1/community-voice-chats/active | 200 `{"data":{"activeChats":[],"total":0}}` | 200 identical body |
| GET /notifications?onlyUnread=true | 200 (1 real notification for this wallet) | 200 `{"notifications":[]}` (per-deployment data) |
| GET /users/{addr}/credits | 200 `{"credits":[],"totalCredits":0,"totals":{"expiring":0,"nonExpiring":0},"usd":{"balanceCents":0,"credits":0}}` | route ported to `catalyrst-credits` with the identical wire shape + signed E2E test; live answers 404 until the pending deploy lands |

### HEAD content-type on status endpoints

Upstream drops `content-type` on HEAD while its own GET of the same URL
returns `application/json` (verified live 2026-08-15 on
`gateway.decentraland.org/{archipelago-ea-stats,comms-gatekeeper}/status`);
the candidate answers HEAD with the same headers as GET per RFC 9110 S9.3.2.
The runner therefore only diffs `content-type` when the baseline actually
sends one -- a candidate supplying a header the baseline omits is value-add,
not divergence (pinned by `content_type_absent_on_baseline_is_candidate_value_add`).

### Data-carried asset hosts: served, not classified away

Poster URLs (`events-assets-*.decentraland.org`) ride event API response data.
The deployment carries the capability itself: `catalyrst-events` serves
`GET /poster/{filename}` + `/poster-vertical/{filename}` from its own content
store, and its read path rewrites data-carried upstream poster URLs to the
deployment's `events-assets-*.{base domain}` (derived from `HTTP_BASE_URL`).
The `events-assets-*`, `marketing-files` and `marketplace-api` vhosts, and the
optional cache-through fallback to upstream's buckets for blobs uploaded to
upstream (an intentional, documented upstream dependency), are nginx-level and
land via the operator's colmena plan; until then those corpus rows report as
real differences. `exploration-games.decentraland.org` is the upstream-operated
minigames backend (colyseus family, no public source): served only as a
documented pass-through in the same plan.
