# Service-plane bundles

The catalyst content core (`catalyrst-server`/`catalyrst-live`) and the federation
services (`market`, `places`, `events`, `communities`, `comms`, `archipelago`,
`explorer-api`) are joined by a **service plane** that reimplements every other
backend host the Unity explorer resolves (see `DecentralandUrlsSource.cs`).

Each reimplemented service is its own library crate (`crates/catalyrst-<svc>/`,
axum + sqlx against a shared PostgreSQL instance, with a standalone binary). For
deployment they are composed into **bundle binaries** by product surface, so one
process serves many services on one port — the same pattern as the existing
`catalyrst-explorer-api`. A bundle's `main.rs` builds each member's state and
merges each member's `api_router().with_state(..)` (health routes excluded to
avoid duplicate-path panics) under a single listener.

## Bundles

Each bundle listens on a single port assigned by the deployment (`<PORT>`).

| Bundle binary | Member crates |
|---|---|
| `catalyrst-explore` | places, events, archipelago, worlds, map, lists |
| `catalyrst-create` | builder, camera-reel, ab-registry |
| `catalyrst-social` | communities, comms, notifications, badges, media |
| `catalyrst-data` | market, economy, price, credits, **rpc** (merged last — catch-all `/{network}`) |
| `catalyrst-ab-cdn` | *standalone* — catch-all `GET /{*path}` CDN over abgen's on-disk output |
| `catalyrst-social-rpc` | *standalone* — WebSocket dcl-rpc (friends/presence/voice) on `/` |

`ab-cdn` and `social-rpc` stay standalone because a root catch-all and a
WS-on-`/` protocol can't co-host with REST siblings on one port.

## Deliberately kept out of the four bundles

- **`catalyrst-server` (content/lambdas/`/about`)** — this is the catalyst node
  itself (the realm), not a data API; it stays its own service.
- **`catalyrst-explorer-api`** — remains the de-facto **identity/realm** bundle
  (auth-api, feature-flags, blocklist, realm-provider). The 4-surface taxonomy
  has no home for identity/config, so explorer-api keeps it. Its `builder_api`
  and `worlds_content_server` proxy modules are now superseded by the native
  `catalyrst-builder` (create) and `catalyrst-worlds` (explore) crates —
  reconcile/remove the proxies in a later pass.

## Member crate contract

Every member crate exposes, for bundle composition:

- `pub fn api_router() -> axum::Router<AppState>` — real API routes only
  (no `/ping`, `/status`, `/health`, `/ready`, `/metrics`).
- `pub async fn build_state(cfg) -> anyhow::Result<AppState>` — constructs state
  (pools, caches, background tasks, sqlx migrations) exactly as the standalone
  binary does.

Each member keeps its own `main.rs`/binary (re-adding its health routes locally),
so services can still run individually. Members read their own env vars via their
own `Config::from_env()`; a bundle just calls each.

## Status

All crates + bundles compile (`cargo check --workspace`). Not yet wired to a
service manager, not yet run, not yet end-to-end tested.
