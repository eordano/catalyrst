# E2E Test Plan — catalyrst-worlds (`key=worlds`)

Re-implementation of `worlds-content-server.decentraland.org`.

- Crate: `catalyrst-worlds`
- Port: **`5145`** (`HTTP_SERVER_PORT`), bound to `127.0.0.1`
- Base URL under test: `http://127.0.0.1:5145`
- Workspace: `<WORKSPACE>`
- Env: `<ENV_FILE>` (the service's environment file)
- Unit: `catalyrst-worlds.service`
- Database: a PostgreSQL instance with its **own `worlds` DB** (role `<DB_USER>`).

The service is **native-first with upstream proxy fallback**: read endpoints
(`/about`, `/permissions`, `/contents`) only fall back to
`UPSTREAM_WORLDS_URL` (https://worlds-content-server.decentraland.org) when the
world / blob has no local deployment record. Until Phase-B deployment sync
lands and populates `worlds` / `world_scenes` / `content_files`, the proxy path
is what most read checks will actually exercise.

---

## 0. Prerequisites / bring-up

Before first run the maintainer must create the role+DB and apply migration
`0001` (bootstrap snippet at the top of the service's environment file):

```bash
# 1. Create role + DB (as cluster superuser)
psql "host=<SOCKET_DIR> port=5433 dbname=postgres" <<'SQL'
CREATE ROLE <DB_USER> LOGIN PASSWORD '<DB_PASSWORD>';
CREATE DATABASE worlds OWNER <DB_USER>;
\c worlds
CREATE EXTENSION IF NOT EXISTS pg_trgm;
SQL

# 2. Apply schema
psql "host=<SOCKET_DIR> port=5433 dbname=worlds user=<DB_USER> password=<DB_PASSWORD>" \
  -f crates/catalyrst-worlds/migrations/0001_create_worlds_schema.sql

# 3. Build + run
cargo run -p catalyrst-worlds
# or once installed:
systemctl --user start catalyrst-worlds.service
systemctl --user status catalyrst-worlds.service
```

### Seed fixtures (for native-path assertions)

To exercise native (non-proxy) serving, insert a deterministic fixture world
before running the native checks. Example skeleton (adjust columns to match the
applied `0001` schema):

```sql
-- a public, unrestricted world with one scene entity + one content blob
INSERT INTO worlds (name, owner, runtime_metadata, spawn_coordinates, blocked_since)
VALUES ('e2e.dcl.eth',
        '0x1111111111111111111111111111111111111111',
        '{"name":"e2e.dcl.eth","entityIds":["bafkreie2etestentityhash000000000000000000000000000000000000"],"minimapVisible":false}'::jsonb,
        '0,0', NULL);

-- a tiny content blob addressed by hash (inline BYTEA store)
INSERT INTO content_files (hash, content)
VALUES ('bafkreie2etestblob000000000000000000000000000000000000000000', E'\\x68656c6c6f0a'); -- "hello\n"

-- a shared-secret access row to drive the comms 403 / rate-limit checks
INSERT INTO world_access (world_name, access_type, secret)
VALUES ('e2e.dcl.eth', 'shared-secret', 'correct-horse');
```

---

## 1. Native liveness / read checks (curl)

Run each command, compare to the expected status/shape.

```bash
BASE=http://127.0.0.1:5145

# C1 — liveness
curl -sS -o /dev/null -w '%{http_code}\n' "$BASE/ping"
#   EXPECT: 200

# C2 — about (native): healthy doc with comms.adapter pointing back at THIS host
curl -sS "$BASE/world/e2e.dcl.eth/about" | jq '{healthy, acceptingUsers, realm:.configurations.realmName, adapter:.comms.adapter, scenes:.configurations.scenesUrn}'
#   EXPECT: 200; healthy=true; acceptingUsers=true; realm="e2e.dcl.eth";
#           comms.adapter == "fixed-adapter:signed-login:http://127.0.0.1:5145/worlds/e2e.dcl.eth/comms"
#           scenesUrn entries are "urn:decentraland:entity:<id>?=&baseUrl=http://127.0.0.1:5145/contents/"

# C3 — about (proxy fallback): unknown world transparently forwarded upstream
curl -sS -o /dev/null -w '%{http_code}\n' "$BASE/world/does-not-exist-locally.dcl.eth/about"
#   EXPECT: 200 (served from UPSTREAM_WORLDS_URL) — body matches upstream shape

# C4 — permissions (native): secret stripped, owner present
curl -sS "$BASE/world/e2e.dcl.eth/permissions" | jq '{owner, access:.permissions.access.type, secretLeaked:(.permissions.access.secret // "absent")}'
#   EXPECT: 200; owner=="0x1111...1111"; access.type=="shared-secret"; secretLeaked=="absent"
#           (CRITICAL: the shared secret must NOT appear anywhere in the body)

# C5 — contents GET (native blob): full body + immutable cache + ETag
curl -sS -D - -o /tmp/wld_blob.bin "$BASE/contents/bafkreie2etestblob000000000000000000000000000000000000000000"
#   EXPECT: 200; Cache-Control: public,max-age=...,immutable; ETag present;
#           body bytes == "hello\n"

# C6 — contents GET with Range (partial content)
curl -sS -D - -o /dev/null -H 'Range: bytes=0-2' "$BASE/contents/bafkreie2etestblob000000000000000000000000000000000000000000"
#   EXPECT: 206 Partial Content; Content-Range: bytes 0-2/6; Content-Length: 3

# C7 — contents GET with unsatisfiable Range
curl -sS -o /dev/null -w '%{http_code}\n' -H 'Range: bytes=999999-' "$BASE/contents/bafkreie2etestblob000000000000000000000000000000000000000000"
#   EXPECT: 416 Range Not Satisfiable

# C8 — contents HEAD (native): size + ETag headers, no body
curl -sS -I "$BASE/contents/bafkreie2etestblob000000000000000000000000000000000000000000"
#   EXPECT: 200; Content-Length: 6; ETag present; zero-length body

# C9 — contents (proxy fallback): unknown hash forwarded upstream
curl -sS -o /dev/null -w '%{http_code}\n' "$BASE/contents/bafkreiunknownhash00000000000000000000000000000000000000000"
#   EXPECT: 200 (served from UPSTREAM_WORLDS_URL) or upstream's 404 — never a local 500

# C10 — connected-world (native then proxy): wallet presence lookup
curl -sS "$BASE/wallet/0x1111111111111111111111111111111111111111/connected-world" | jq .
#   EXPECT: 200; {"wallet":"0x1111...","world":"<name>"} when a world_presence row exists;
#           otherwise transparently proxied upstream (still 200)

# C11 — blocked world returns 403 (seed blocked_since to test)
#   After: UPDATE worlds SET blocked_since=now() WHERE name='e2e.dcl.eth';
curl -sS -o /dev/null -w '%{http_code}\n' "$BASE/world/e2e.dcl.eth/about"
#   EXPECT: 403; body.error mentions "blocked" + "storage space"
```

## 2. Comms / LiveKit auth checks (curl)

The comms endpoints require a valid signed-fetch AuthChain
(`require_signer`). These checks confirm the **auth gate** and **rate limiter**
without a fully signed chain; the happy path needs a real signed request (see
the bevy/walk smoke step in §4, which produces a genuine AuthChain).

```bash
BASE=http://127.0.0.1:5145

# C12 — comms without AuthChain headers -> 401
curl -sS -o /dev/null -w '%{http_code}\n' -X POST "$BASE/worlds/e2e.dcl.eth/comms"
#   EXPECT: 401; body.error from require_signer (missing/invalid auth chain)

# C13 — per-scene comms without AuthChain -> 401
curl -sS -o /dev/null -w '%{http_code}\n' -X POST "$BASE/worlds/e2e.dcl.eth/scenes/scene-1/comms"
#   EXPECT: 401

# C14 — shared-secret wrong password (requires a valid signed chain in headers;
#         drive via the signed-fetch helper used by catalyrst-comms tests).
#   With a valid AuthChain BUT x-identity-metadata={"secret":"wrong"}:
#   EXPECT: 403; body.error == "Access denied, invalid shared secret."

# C15 — shared-secret rate limiter: 3 failed attempts / 60s window then 429
#   Replay C14 (valid chain, wrong secret) 4x from the same subject:
#   EXPECT: attempts 1-3 -> 403; 4th -> 429 with a Retry-After header (~60).

# C16 — shared-secret correct password -> 200 with fixedAdapter LiveKit URL
#   Valid AuthChain + x-identity-metadata={"secret":"correct-horse"}:
#   EXPECT: 200; body.fixedAdapter == "livekit:wss://<LIVEKIT_HOST>?access_token=<JWT>"
#           JWT room claim == "world-e2e.dcl.eth" (WORLD_ROOM_PREFIX + lowercased name)
#   NOTE: devkey/devsecret mint a parseable but cluster-REJECTED JWT; a real
#         LiveKit join needs LIVEKIT_API_KEY/SECRET shared with catalyrst-comms.
```

The signed happy-path (C14–C16) is best driven through the same EIP-712
signed-fetch helper `catalyrst-comms` uses in its tests, or end-to-end via a
real client (§4) which mints a genuine AuthChain over the request line.

---

## 3. Unity / explorer repointing — config pointer

In a checkout of the `unity-explorer` source, the relevant files are:

URL registry file:
`Explorer/Assets/DCL/NetworkDefinitions/Browser/DecentralandUrlsSource.cs`
Enum file:
`Explorer/Assets/DCL/Infrastructure/Utility/DecentralandUrls/DecentralandUrl.cs`

Six enums map to this host. They are set in the `RawUrl(...)` switch (the
`{ENV}` token is replaced with the environment domain, e.g. `org`). **These are
hard-coded host URLs, NOT `/about`-discovered — repoint them by editing Unity.**
The realm `/about` document only controls the `comms.adapter` value (which our
own `/about` already points back at `:5145`); the registry URLs above are
independent and must be edited here.

Exact lines to change (current → repointed to `http://127.0.0.1:5145`):

| Enum | Current `RawUrl` (line) | Repoint to |
|---|---|---|
| `RemotePeersWorld` | L190 `$"https://worlds-content-server.decentraland.{ENV}/wallet/[USER-ID]/connected-world"` | `"http://127.0.0.1:5145/wallet/[USER-ID]/connected-world"` |
| `WorldPermissions` | L212 `$"https://worlds-content-server.decentraland.{ENV}/world/{{0}}/permissions"` | `"http://127.0.0.1:5145/world/{0}/permissions"` |
| `WorldComms` | L213 `$"https://worlds-content-server.decentraland.{ENV}/worlds/{{0}}/comms"` | `"http://127.0.0.1:5145/worlds/{0}/comms"` |
| `WorldServer` | L214 `$"https://worlds-content-server.decentraland.{ENV}/world"` | `"http://127.0.0.1:5145/world"` |
| `WorldContentServer` | L215 `$"https://worlds-content-server.decentraland.{ENV}/contents/"` | `"http://127.0.0.1:5145/contents/"` |
| `WorldCommsAdapter` | L240 `$"https://worlds-content-server.decentraland.{ENV}/worlds/{{0}}/scenes/{{1}}/comms"` | `"http://127.0.0.1:5145/worlds/{0}/scenes/{1}/comms"` |

Notes:
- Drop the `$"...{ENV}..."` interpolation when hard-coding `127.0.0.1:5145`
  (a plain string literal still implicitly converts to `UrlData`); leave the
  positional `{0}`/`{1}` `string.Format` placeholders intact.
- `WorldServer` (`/world`) is the base used to build `/world/{name}/about`.
  `WorldContentServer` (`/contents/`) is the blob base.
- After editing, rebuild/redeploy the Unity client so the cached `UrlData`
  picks up the new literals.
- Alternatively (no Unity rebuild), repoint at the **explorer-api proxy**: its
  existing `/world*`, `/worlds*`, `/contents*`, `/wallet/*/connected-world`
  forwards can be aimed at `http://127.0.0.1:5145`, and Unity keeps pointing at
  explorer-api.

---

## 4. Real-client smoke (dcl-bevy / dcl-walk)

Use a real client to exercise the signed comms happy-path and end-to-end world
entry, which curl cannot easily reproduce (genuine AuthChain signing).

1. Repoint either Unity (§3) or the explorer-api proxy at `:5145`.
2. Bring up the service (§0) and confirm `C1`–`C10` pass.
3. **dcl-bevy** (fast, native, signs its own AuthChain):
   ```bash
   dcl-bevy up
   # teleport into a world realm that resolves through this host, e.g.:
   #   set the realm to http://127.0.0.1:5145/world/e2e.dcl.eth
   ```
   - EXPECT: client fetches `/world/e2e.dcl.eth/about` from `:5145`, reads
     `comms.adapter` = `fixed-adapter:signed-login:.../worlds/e2e.dcl.eth/comms`,
     POSTs a **signed** request to `/worlds/e2e.dcl.eth/comms`, receives 200 +
     `fixedAdapter` LiveKit URL, and renders the scene from `/contents/<hash>`.
   - Watch `journalctl --user -u catalyrst-worlds -f` (or stdout) for
     the about → comms → contents request sequence and a minted token.
4. **dcl-walk** (Unity refclient) — alternative when validating the
   real Unity URL-registry edits from §3:
   ```bash
   dcl-walk launch
   dcl-walk auth-sign        # produce a real session AuthChain
   # teleport to the world realm served by :5145
   ```
   - EXPECT: same about → comms → contents flow, driven by the production Unity
     client against the repointed enums.
   - For the comms happy-path with `shared-secret`, the client must supply the
     correct secret in `x-identity-metadata` — confirm a 200 + adapter, and that
     a wrong secret yields 403 then 429 after 3 tries (mirrors C14/C15).

### Pass criteria
- All curl checks (C1–C13) return the expected status codes.
- `/permissions` never leaks the shared secret (C4).
- Range semantics: 200 / 206 / 416 correct (C5–C7).
- A real client (bevy or walk) enters the world, mints a LiveKit token via the
  signed `/comms` POST, and loads scene content — all requests hitting `:5145`.
- For prod LiveKit join: `LIVEKIT_API_KEY`/`SECRET`/`HOST` must be real and
  shared with `catalyrst-comms` (devkey/devsecret tokens are rejected by a real
  cluster).
