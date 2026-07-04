# Admin console — operator surface

> Status: distilled 2026-07-04 from `crates/catalyrst-server/src/{handlers/console.rs,admin/}`
> and DEPLOYMENT.md. The full design rationale (478 lines, incl. the LATER
> roadmap) lives at git ref `ff400cab^:catalyrst/docs/admin-console.md`.

The console is server-side-rendered HTML on the content core: `GET /`
(landing), `GET /admin`, `GET /admin/{service}`, plus a gated mutation surface
`POST /admin/api/*` (`routes.rs` mounts content flush-cache, denylist
add/remove, snapshot regeneration, sync pause/resume/force, read-only toggle,
telemetry SQL, social user-ban, places moderation, POI CRUD, and proxies to
sibling services).

## Design invariants

- **SSR-first**; JS only progressively enhances. Every view is a shareable URL
  with no hidden client state.
- **Default-safe.** With `ADMIN_ADDRESSES` or `SESSION_SECRET` unset, the
  console is read-only and every mutation returns 403; the UI hides all
  controls. You must opt in to write access.
- **Never on the public edge.** The example nginx configs 404 `/admin`; it is
  reached on the loopback port or a private network, *and* gated by the wallet
  allowlist. The unauthenticated read-only pages are deliberately viewable
  (status isn't sensitive; pages stay shareable) — only controls and
  `POST /admin/api/*` require the session.

## Sibling-service health

`CATALYRST_SERVICE_URLS` (comma-separated `key=baseurl`; keys
`explore,create,social,data,ab-cdn,social-rpc,scene-state,profile-images,explorer-api,telemetry`)
powers short-TTL `/health` probes rendered as service dots on `GET /` and
`/admin`. Unset keys render "not configured", never "down".

## Auth model — one signature, then a stateless cookie

Signed-fetch-per-click is hostile UX for a console, so sign-in is a single
EIP-191 personal-sign over a SIWE-style message, then an HMAC session cookie:

```
GET  /admin/auth/nonce?address=0x…  → { message }
POST /admin/auth/verify             → sets cat_admin cookie ({message, signature})
POST /admin/auth/logout             → clears cookie
GET  /admin/auth/me                 → { address } | 401
```

Statelessness is the point: the message's `Nonce:` is
`HMAC(SESSION_SECRET, host|address|exp)` with a 5-minute expiry — no nonce
store, not replayable against another host or address. `verify` re-checks
host, expiry, nonce HMAC, and recovered signer ∈ `ADMIN_ADDRESSES` before
minting `cat_admin` = `base64url({addr,exp}) . base64url(HMAC)` —
`HttpOnly; SameSite=Strict; Secure` (TTL `ADMIN_SESSION_TTL_SECS`, default
12h). Mutations additionally require same-origin `Origin`/`Referer` when
present.

## Environment

| Env | Meaning | Default |
|---|---|---|
| `ADMIN_ADDRESSES` | comma-separated `0x…` allowlist | unset ⇒ read-only |
| `SESSION_SECRET` | HMAC key for cookie + nonce | unset ⇒ read-only |
| `ADMIN_SESSION_TTL_SECS` | session lifetime | 43200 |
| `ADMIN_COOKIE_INSECURE` | `1` drops the `Secure` flag (plain-HTTP private nets only; localhost is already a secure context) | unset |
| `COMMS_MODERATOR_TOKEN` / `MODERATOR_TOKEN` | bearer forwarded to comms for ban/warn/unban | unset ⇒ social controls hidden |
| `AB_REGISTRY_ADMIN_TOKEN` / `API_ADMIN_TOKEN` | bearer forwarded to the registry for re-ingest / cache flush | unset ⇒ create controls hidden |
| `DEBUGGING_SECRET` | secret injected into scene-state reload | unset ⇒ scene controls hidden |

The console accepts either its own env name or the sibling service's native
name so a single-host deploy needn't set the same value twice. Unsupported
proxy actions return 501 and are audited as "unsupported"
(`admin/api.rs`).

The telemetry `/dash/*` pages carry **no token** — they are loopback-trusted
and MUST stay firewalled to loopback/private network.
