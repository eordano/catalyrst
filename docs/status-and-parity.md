# Status and parity — what works, what's stubbed, what deliberately diverges

> Status: rebuilt 2026-07-04. Sources: a fresh code sweep for 501s/mocks/
> fail-modes (this date), plus the 2026-07-03 full audit (per-service dossiers
> at git ref `ff400cab^:catalyrst/docs/{parity,verification,e2e-test}/`).
> Every item carries its provenance date; items from the 07-03 audit that this
> sweep did not re-verify say so. This page is the transparency ledger — when
> code and this page disagree, fix the page.

## 1. Parity philosophy

catalyrst targets behavioral parity with the reference TypeScript services:
same routes, same wire shapes (including upstream's quirks — e.g. the market
crate deliberately replicates upstream SQL oddities like lexicographic price
sort), same auth semantics, same overwrite/snapshot semantics. The Rust
sources carry no comments by policy — the rationale for such
deliberately-odd code lives in these docs, nowhere else. Divergences fall
into three honest buckets:

1. **Additive-only fields** — new fields ride `skip_serializing_if` so the
   non-enriched response is byte-identical to upstream (lease overlay in
   market user-assets; rental fields in map tiles). When the overlay DB is
   absent these components report `is_enabled=false` and pass through.
2. **Deliberate contract changes** — the federation signed-write envelope
   (see [federation.md](./federation.md)) and the admin surface. Community
   writes accept **both** shapes: `writes.rs` dispatches federation envelopes
   to the `fed_*` handlers and plain client bodies to the native client
   handlers (verified 2026-07-04 — the older claim that community writes are
   501-only is obsolete).
3. **Not-yet-implemented** — enumerated below; these return explicit 501s or
   boot-time-disabled behavior, never silent fakes.

## 2. Not-implemented surface (verified 2026-07-04)

| Surface | Behavior | Where |
|---|---|---|
| Cast 2.0 (RTMP/ingress): all `/cast/*` routes + scene-stream-access PUT/DELETE | **501** — needs LiveKit IngressClient | `catalyrst-comms/src/handlers/deferred.rs` |
| `DELETE /events/{id}` | **501** — deletion is federation-path-only, and the mounted federation event routes are GET-only, so no live deletion path exists | `catalyrst-events/src/handlers/events.rs` |
| Registry denylist + queue admin without the `ab_registry` DB | **501** | `catalyrst-registry/src/handlers/{denylist,queues}.rs` |
| Raw (unhashed) file upload in the deployer | rejected as Invalid — supply pre-hashed files | `catalyrst-deployer/src/deployment_service.rs` |
| Meta-tx **broadcast** | `META_TX_BROADCAST_ENABLED` defaults **false**: economy validates transactions but does not broadcast unless enabled with relayer creds | `catalyrst-economy/src/config.rs` |
| Notifications email | silently disabled (`is_enabled=false`) without SendGrid config — subscription writes succeed, mail never sends | `catalyrst-notifications/src/ports/email.rs` |
| EIP-1654 (contract-wallet) signatures on the sync verify path | rejected by design (use the async verifier — [auth.md](./auth.md)); HTTP crates map the not-implemented variant to **501**, except world-storage which maps it to **503** — a known inconsistency | `catalyrst-crypto/src/verify.rs`; `world-storage/src/auth_chain.rs` |

Previously-501 surfaces now implemented (verified 2026-07-04): comms
`/private-messages/token`, private voice chat, community voice chat (all real
handlers in `catalyrst-comms/src/handlers/voice.rs`); communities writes
(dual-dispatch, above); places report/favorites/likes; credits captcha.

## 3. Mock & simulation toggles (all default OFF/honest)

| Toggle | Default | What it fakes when on |
|---|---|---|
| `CREDITS_MOCK_CARD` | off | mock card top-up grants real Credits with no Stripe charge (`/topup/mock-card`, capped 10 000/request); endpoint is 501 when off |
| `CREDITS_MOCK_FULFILLMENT` | off | pack purchase delivers items off-chain via usage-grant, no economy broker call |
| `TRANSLATE_BACKEND=mock` | **mock is the default** | translations answered locally without LibreTranslate — the one mock-by-default surface |
| `PROFILE_IMAGES_BACKEND` | auto: proxy or disabled | `render` is the real local pipeline; `proxy` forwards to an origin |
| `PRICE_POLL_ENABLED` / `GOVERNANCE_POLL_ENABLED` | off | without the poller the service serves whatever snapshots exist; stale price data makes credits checkout **fail-closed** ("MANA/USD oracle is stale", max staleness 300 s) rather than mispriced |

## 4. Money-path posture (credits/economy — verified 2026-07-04)

Fail-closed everywhere:

- `CREDITS_REQUIRE_PURCHASE_INTENT` defaults **on** — unsigned checkouts
  rejected.
- `/checkout` 501s when the usage-grants overlay, economy admin token, or
  escrow address is missing — "refusing to debit Credits for an item that
  would be invisible in the backpack."
- Stripe endpoints 501 without Stripe secrets; MANA top-up 501 without the
  economy token (idempotent replays of already-granted tx hashes still
  answer).
- credits/market/economy refuse to boot on a non-loopback bind without their
  admin bearer token.

## 5. Known divergences & degraded modes (2026-07-03 audit; not re-verified 2026-07-04)

The last full client-against-service audit (per-service dossiers + Unity
net-catalog cross-reference) concluded: **no service panics on a request
path**; all panics are startup-only. Its still-open items, carried forward
with their audit-day severity — re-verify before relying:

- **`GET /about` gates `healthy` on comms** and 503s when the comms sidecar is
  down, where upstream treats comms as optional and returns 200 — breaks realm
  change in the stock client (`about.rs`).
- **Archipelago `/status` not mounted on the explore bundle** (standalone
  binary only) — the stock client's bootstrap health-probe 404s → LiveKit-down
  popup.
- **Archipelago `/ws` speaks JSON-text, upstream speaks binary protobuf** —
  live peer presence/discovery degraded for the stock client.
- Recurring error-contract drift: auth failures 400-vs-401 (communities,
  comms), 503-vs-200 when an optional writer DB is unconfigured (places,
  worlds), 500-instead-of-degrade on per-request DB blips (media, market),
  invalid enum params silently ignored vs upstream 400 (market), plain-text
  axum rejection bodies vs the JSON error envelope.
- Field-completeness gaps degrading (not breaking) UI: places list render
  fields, map rental/estate enrichment, communities list enrichment.
- `catalyrst-rpc` serves a fixed network allowlist; other chains get `-32602`.
- Security flags raised by the audit, unresolved as of 07-03:
  `/get-server-scene-adapter` missing the authoritative-server identity gate;
  communities moderation-list readable by any signed wallet; managed-communities
  route unauthenticated. The registry's admin-bearer bypass of signed-fetch is
  by design, not one of these.

## 6. Direction-of-failure exceptions worth knowing

Fail-closed is the house rule (auth, third-party roots, sync hash checks,
money paths, worlds NAME-ownership publish authz when squid is down). Two
deliberate exceptions:

- **worlds wallet-denylist fetch failure is fail-OPEN** (empty set, access
  allowed) — availability chosen over enforcement
  (`catalyrst-worlds/src/ports/denylist.rs`).
- **social-rpc profile enrichment failure serves placeholder profiles**
  instead of erroring — degraded content over hard failure
  (`catalyrst-social-rpc/src/profiles.rs`).

## 7. How to re-verify any claim here

```bash
# side-by-side vs a reference host (bootstraps inputs from the baseline)
cargo run -p catalyrst-conformance -- \
  --baseline https://peer.decentraland.org --candidate http://localhost:5141

# offline replay of recorded fixtures (CI-friendly)
cargo run -p catalyrst-conformance --bin catalyrst-conformance-replay

# oracle vectors from a live DB
cargo run -p catalyrst-oracle-tests --bin extract && \
  cargo test -p catalyrst-oracle-tests -- --ignored
```

State-dependent endpoints (`/content/snapshots`, `/content/failed-deployments`)
legitimately differ between peers; `volatility.toml` masks them. Client-shape
questions resolve against the Unity DTOs/converters, not against the TS
server — the client is the contract that crashes.
