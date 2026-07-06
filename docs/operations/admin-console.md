# Admin console - operator surface

> Status: distilled from `crates/catalyrst-server/src/{handlers/console.rs,admin/}` and DEPLOYMENT.md. Full design rationale (478 lines): git ref `ff400cab^:catalyrst/docs/admin-console.md`.

Server-side-rendered HTML on the content core: `GET /` (landing), `GET /admin`, `GET /admin/{service}`, plus gated mutations `POST /admin/api/*` (`routes.rs` mounts content flush-cache, denylist add/remove, snapshot regeneration, sync pause/resume/force, read-only toggle, telemetry SQL, social user-ban, places moderation, POI CRUD, proxies to sibling services).

- SSR-first; JS only progressively enhances. Every view is a shareable URL with no hidden client state.
- Default-safe: with `ADMIN_ADDRESSES` or `SESSION_SECRET` unset, the console is read-only, every mutation returns 403, the UI hides all controls.
- Never on the public edge: the example nginx configs 404 `/admin`; reach it on the loopback port or a private network, gated by the wallet allowlist. Unauthenticated read-only pages stay viewable; only controls and `POST /admin/api/*` require the session.

Sibling health: `CATALYRST_SERVICE_URLS` (comma-separated `key=baseurl`; keys `explore,create,social,data,ab-cdn,social-rpc,scene-state,profile-images,explorer-api,telemetry`) powers short-TTL `/health` probes rendered as service dots on `GET /` and `/admin`. Unset keys render "not configured", never "down".

## Auth - one signature, then a stateless cookie

Sign-in is a single EIP-191 personal-sign over a SIWE-style message, then an HMAC session cookie:

```
GET  /admin/auth/nonce?address=0x...  -> { message }
POST /admin/auth/verify               -> sets cat_admin cookie ({message, signature})
POST /admin/auth/logout               -> clears cookie
GET  /admin/auth/me                   -> { address } | 401
```

The message's `Nonce:` is `HMAC(SESSION_SECRET, host|address|exp)` with a 5-minute expiry - no nonce store, not replayable against another host or address. `verify` re-checks host, expiry, nonce HMAC, and recovered signer in `ADMIN_ADDRESSES` before minting `cat_admin` = `base64url({addr,exp}) . base64url(HMAC)` - `HttpOnly; SameSite=Strict; Secure` (TTL `ADMIN_SESSION_TTL_SECS`, default 12h). Mutations additionally require same-origin `Origin`/`Referer` when present.

## Environment

| Env | Meaning | Default |
|---|---|---|
| `ADMIN_ADDRESSES` | comma-separated `0x...` allowlist | unset -> read-only |
| `SESSION_SECRET` | HMAC key for cookie + nonce | unset -> read-only |
| `ADMIN_SESSION_TTL_SECS` | session lifetime | 43200 |
| `ADMIN_COOKIE_INSECURE` | `1` drops the `Secure` flag (plain-HTTP private nets only; localhost is already a secure context) | unset |
| `COMMS_MODERATOR_TOKEN` / `MODERATOR_TOKEN` | bearer forwarded to comms for ban/warn/unban | unset -> social controls hidden |
| `AB_REGISTRY_ADMIN_TOKEN` / `API_ADMIN_TOKEN` | bearer forwarded to the registry for re-ingest / cache flush | unset -> create controls hidden |
| `DEBUGGING_SECRET` | secret injected into scene-state reload | unset -> scene controls hidden |

The console accepts either its own env name or the sibling service's native name. Unsupported proxy actions return 501 and are audited as "unsupported" (`admin/api.rs`). The telemetry `/dash/*` pages carry no token - loopback-trusted; MUST stay firewalled to loopback/private network.
