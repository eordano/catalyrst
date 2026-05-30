# E2E Test Plan — catalyrst-builder (key=`builder`)

Reimplementation of `builder-api.decentraland.org` (read + newsletter subset).

- Crate: `catalyrst-builder`
- Workspace: `<WORKSPACE>`
- Local port: `5143` (HTTP, `127.0.0.1`)
- PostgreSQL: a reachable instance, DB `marketplace_squid`, schema `marketplace`
- Bucket redirect target: `BUILDER_BUCKET_URL=https://builder-api.decentraland.org/v1/storage/contents`

Routes implemented:
- `GET  /v1/collections/{id}/items` — signed-fetch AuthChain verify + collection access (owner/admin/minter/published-manager)
- `GET  /v1/storage/contents/{hash}` — 301 redirect to bucket, immutable cache, `?ts` preserved
- `HEAD /v1/storage/contents/{hash}` — same 301
- `POST /v1/newsletter` — `{email,source}` validated + inserted into `marketplace.builder_newsletter`
- `GET  /ping` — health

---

## 1. Unity client config — how to repoint

The three Builder URLs are **hardcoded templates**, NOT realm/`/about`-discovered. They live in the `RawUrl(...)` switch and are resolved by substituting `{ENV}` (`org`/`zone`/`today`) — there is no `/about` field that carries them, so they MUST be changed in Unity source.

File (in the Unity client checkout): `Explorer/Assets/DCL/NetworkDefinitions/Browser/DecentralandUrlsSource.cs`

Current lines (172-174):

```csharp
DecentralandUrl.BuilderApiDtos       => $"https://builder-api.decentraland.{ENV}/v1/collections/[COL-ID]/items",
DecentralandUrl.BuilderApiContent    => $"https://builder-api.decentraland.{ENV}/v1/storage/contents/",
DecentralandUrl.BuilderApiNewsletter => $"https://builder-api.decentraland.{ENV}/v1/newsletter",
```

Repoint (point at the local service; drop the `{ENV}` interpolation since the host is fixed):

```csharp
DecentralandUrl.BuilderApiDtos       => "http://127.0.0.1:5143/v1/collections/[COL-ID]/items",
DecentralandUrl.BuilderApiContent    => "http://127.0.0.1:5143/v1/storage/contents/",
DecentralandUrl.BuilderApiNewsletter => "http://127.0.0.1:5143/v1/newsletter",
```

Notes:
- The enum `DecentralandUrl` is defined in
  `Explorer/Assets/DCL/Infrastructure/Utility/DecentralandUrls/DecentralandUrl.cs`
  (members `BuilderApiDtos`, `BuilderApiContent`, `BuilderApiNewsletter`). No enum change needed — only the `RawUrl` switch values.
- Keep the `[COL-ID]` token and the trailing `/` on the content URL intact (the client string-replaces / appends to them).
- `{ENV}` is `const string ENV = "{ENV}"` (line 33), replaced later by the resolved domain (`Url(...)` at line 98). When hardcoding a fixed host you may omit `{ENV}` entirely as above; if you want env-switching, leave `{ENV}` and instead front the service behind a `builder-api.decentraland.{org|zone|today}` DNS/proxy entry.
- Production hosting note: storage `GET/HEAD` is a 301 redirect to `BUILDER_BUCKET_URL`. Out of the box that still points at the real `builder-api.decentraland.org` bucket; repoint `BUILDER_BUCKET_URL` in the service's environment file (`<ENV_FILE>`) if you want assets served locally.

**`/about`-discovered?** NO. These three are fixed `RawUrl` switch entries, changed in Unity (`DecentralandUrlsSource.cs` lines 172-174), not via the realm `/about` response.

---

## 2. Concrete e2e checks (curl) against `127.0.0.1:5143`

Run the service first:

```bash
export BUILDER_BUCKET_URL=https://builder-api.decentraland.org/v1/storage/contents
cargo run -p catalyrst-builder   # or via the supervised unit once wired
```

### 2.1 Health
```bash
curl -s -o /dev/null -w "%{http_code}\n" http://127.0.0.1:5143/ping
# expect: 200
```

### 2.2 Newsletter — valid
```bash
curl -s -w "\n%{http_code}\n" -X POST http://127.0.0.1:5143/v1/newsletter \
  -H 'content-type: application/json' \
  -d '{"email":"e2e-builder@example.com","source":"auth"}'
# expect: 200  body {"ok":true}
# verify row: psql -h <SOCKET_DIR> -p 5433 -U <DB_USER> marketplace_squid -c \
#   "select email,source from marketplace.builder_newsletter where email='e2e-builder@example.com';"
# cleanup: delete from marketplace.builder_newsletter where email='e2e-builder@example.com';
```

### 2.3 Newsletter — invalid email
```bash
curl -s -w "\n%{http_code}\n" -X POST http://127.0.0.1:5143/v1/newsletter \
  -H 'content-type: application/json' -d '{"email":"not-an-email"}'
# expect: 400  (looks_like_email rejects: no '@', or domain without '.')
```

### 2.4 Storage content — GET 301 redirect (no follow)
```bash
curl -s -D - -o /dev/null http://127.0.0.1:5143/v1/storage/contents/bafkreiexamplehash
# expect: HTTP/1.1 301 Moved Permanently
#   Location: https://builder-api.decentraland.org/v1/storage/contents/bafkreiexamplehash
#   Cache-Control: public, max-age=31536000, immutable
```

### 2.5 Storage content — `?ts` preserved on redirect
```bash
curl -s -D - -o /dev/null "http://127.0.0.1:5143/v1/storage/contents/bafkreiexamplehash?ts=1700000000"
# expect: 301, Location ends with /bafkreiexamplehash?ts=1700000000
```

### 2.6 Storage content — HEAD 301
```bash
curl -s -I http://127.0.0.1:5143/v1/storage/contents/bafkreiexamplehash
# expect: 301 with same Location + immutable Cache-Control headers
```

### 2.7 Collection items — unsigned request rejected
```bash
curl -s -w "\n%{http_code}\n" \
  http://127.0.0.1:5143/v1/collections/00000000-0000-0000-0000-000000000000/items
# expect: 401  (require_signer fails: no x-identity-auth-chain-* headers)
```

### 2.8 Collection items — malformed collection id
```bash
curl -s -w "\n%{http_code}\n" \
  -H 'x-identity-timestamp: 1' -H 'x-identity-auth-chain-0: {}' \
  http://127.0.0.1:5143/v1/collections/not-a-uuid/items
# expect: 400 ("invalid collection id"); auth runs after uuid parse, so this is the uuid error
```

### 2.9 Collection items — signed but unknown collection
Use a real signed-fetch AuthChain (see helper below) over `GET:/v1/collections/{uuid}/items`.
```bash
# After signing (timestamp within 5 min, lowercased METHOD:PATH:TS:METADATA payload):
curl -s -w "\n%{http_code}\n" \
  -H "x-identity-timestamp: $TS" \
  -H "x-identity-metadata: {}" \
  -H "x-identity-auth-chain-0: $LINK0" \
  -H "x-identity-auth-chain-1: $LINK1" \
  http://127.0.0.1:5143/v1/collections/$RANDOM_UUID/items
# expect: 404 ("collection not found") once signature verifies but no such row.
# If signer is verified AND collection exists AND signer lacks access -> 403.
# If verified + access granted + no imported items -> 200 {"ok":true,"data":[]}.
```

Signing helper (reuse the catalyrst signed-fetch tooling / `dcl-walk auth-sign`, or the
`catalyrst-crypto` test fixtures). The payload format is
`build_payload(method,path,ts,metadata)` = `"{method}:{path}:{ts}:{metadata}"` lowercased,
ts in **milliseconds**, 5-minute expiry window.

### 2.10 Expired / stale signature
Sign with a timestamp older than 5 minutes and replay 2.9.
```bash
# expect: 401 (Expired signature)
```

---

## 3. Real-client smoke (dcl-bevy / dcl-walk)

The Builder API is consumed by the **Unity** client (in-world wearable preview / builder collections + the newsletter prompt). bevy/godot do not exercise these routes, so use `dcl-walk`.

1. Repoint Unity per section 1 (edit lines 172-174, rebuild/relaunch via `dcl-editor build` or `dcl-walk launch`).
2. Start `catalyrst-builder` on `5143` and `tail` its logs (`TraceLayer` logs every request).
3. Newsletter: in-client, trigger the newsletter/subscription opt-in (the marketplace-credits trial-end prompt). Confirm a `POST /v1/newsletter 200` appears in builder logs and a row lands in `marketplace.builder_newsletter`. Clean the row after.
4. Storage content: load a scene/wearable whose contents resolve through `BuilderApiContent`; confirm `GET /v1/storage/contents/{hash}` shows `301` in logs and the client follows the redirect to the bucket and renders the asset (no broken-texture placeholder).
5. Collection items: as a wallet that owns/manages a builder collection, open that collection's item list; confirm `GET /v1/collections/{uuid}/items 200` with a signed AuthChain in builder logs. (With no imported items the body is `data:[]` — expected for v1; verify the request authenticates rather than expecting populated items.)
6. Capture screenshots via `dcl-walk` / `dcl-editor screenshot` for the newsletter prompt and any builder UI exercised.

Pass criteria: all section-2 curl statuses match; Unity routes hit the local service (visible in builder logs) with the documented status codes; no auth regressions (401 on unsigned, 200 on signed).

---

## 4. Known v1 gaps to keep out of scope (do not fail the run on these)
- Write/upload path (populating `builder_items`/`builder_collections`) is deferred — signed collection requests legitimately return `data:[]`.
- Curations, committee membership, third-party collections are intentionally not ported.
- `modules/builder_api.rs` proxy in `catalyrst-explorer-api` is still present; the native crate is not yet under supervision. Once wired into systemd and verified, remove the proxy.
