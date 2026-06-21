# E2E Test Plan — `config-denylist`

Catalyrst reimplementation of `config.decentraland.org/denylist.json`.

| | |
|---|---|
| Key | `config-denylist` |
| Crate | `catalyrst-explorer-api` |
| Port | **`5137`** (the deployment's assigned port) |
| Route | `GET /denylist.json` (mounted at root in `src/main.rs` via `modules::blocklist::routes()`) |
| Backing store | flat file at `cfg.blocklist_path` (default `<DATA_DIR>/config/denylist.json`), hot-read per request. No DB, no auth, no migrations. |
| Seed file | `<DATA_DIR>/config/denylist.json` → `{"users":[]}` |

## Contract

Unity `BlocklistCheckStartupOperation` → `ApplicationBlocklistGuard.IsUserBlocklistedAsync`
(`Explorer/Assets/DCL/ApplicationsGuards/ApplicationBlocklistGuard/ApplicationBlocklistGuard.cs`):

- GETs the URL, then `JsonUtility.FromJson<BlocklistData>(response.body)`.
- Bans the local identity if any `users[].wallet` equals the own address (case-insensitive).
- On any HTTP/parse failure it logs and returns `isBanned = false` (fail-open). Our handler mirrors
  this: read error or parse error → `200` with `{"users":[]}`.
- Note: the guard is only reached on this path when the `REPORT_USER` feature flag is **disabled**;
  when enabled the client uses the moderation RPC provider instead and never hits `/denylist.json`.

Required JSON shape (JsonUtility-compatible, field name `wallet` is load-bearing):

```json
{ "users": [ { "wallet": "0x..." } ] }
```

---

## 1. Unity config — how to repoint

**This URL is NOT `/about`-discovered.** It is hardcoded in the `RawUrl` switch and only the `{ENV}`
token (org/zone/today) is substituted at runtime (`decentralandDomain = environment.ToString().ToLower()`,
see `Probe`/`Url` in `DecentralandUrlsSource.cs`). The host segment `config.decentraland` is fixed in
source, so it can only be changed by editing Unity — there is no realm-driven override.

File: `Explorer/Assets/DCL/NetworkDefinitions/Browser/DecentralandUrlsSource.cs`
Enum: `DecentralandUrl.Blocklist = 54` (`Explorer/Assets/DCL/Infrastructure/Utility/DecentralandUrls/DecentralandUrl.cs:85`)

Current line (`DecentralandUrlsSource.cs:207`):

```csharp
DecentralandUrl.Blocklist => $"https://config.decentraland.{ENV}/denylist.json",
```

Repoint to local service — replace the whole RHS (the `{ENV}` token must be dropped, since the local host
has no env-suffixed domain):

```csharp
DecentralandUrl.Blocklist => $"http://127.0.0.1:5137/denylist.json",
```

(No reverse-proxy assumed. If catalyrst is fronted by a host alias, use that full URL instead; just keep
the `/denylist.json` path. Do not leave the `{ENV}` token in — there is nothing to substitute.)

bevy-explorer and godot-explorer have **no** blocklist/denylist code path, so no repoint is needed there;
the only real-client target is the Unity explorer.

---

## 2. Service-level e2e checks (curl)

Bring the service up first (it is not currently listening on `5137`):

```bash
# from the catalyrst workspace
cd <WORKSPACE>
HTTP_SERVER_HOST=127.0.0.1 HTTP_SERVER_PORT=5137 cargo run -p catalyrst-explorer-api
# (or once installed as a service: systemctl --user start catalyrst-explorer-api.service)
```

### Check A — happy path, empty seed list

```bash
curl -sS -i http://127.0.0.1:5137/denylist.json
```
Expect: `200 OK`, header `content-type: application/json`, body byte-identical to the seed file
(`{"users":[]}` modulo whitespace). The body is served verbatim from disk after a parse-validation pass.

### Check B — shape is JsonUtility-compatible

```bash
curl -sS http://127.0.0.1:5137/denylist.json | jq -e 'has("users") and (.users | type == "array")'
```
Expect: prints `true`, exit 0. (`users` present, array-typed.)

### Check C — populated list round-trips the `wallet` field

```bash
# temporarily seed a banned wallet
cp <DATA_DIR>/config/denylist.json /tmp/denylist.bak
printf '{"users":[{"wallet":"0xDEADBEEF00000000000000000000000000000001"}]}' > <DATA_DIR>/config/denylist.json
curl -sS http://127.0.0.1:5137/denylist.json | jq -e '.users[0].wallet == "0xDEADBEEF00000000000000000000000000000001"'
# hot-reload check: no service restart needed, the handler re-reads per request
cp /tmp/denylist.bak <DATA_DIR>/config/denylist.json
curl -sS http://127.0.0.1:5137/denylist.json | jq -e '.users | length == 0'
```
Expect: first `jq -e` prints `true` (wallet field name preserved verbatim, exit 0); after restoring the
seed, second `jq -e` prints `true` — proving hot-reload with no restart.

### Check D — fail-open on malformed file

```bash
cp <DATA_DIR>/config/denylist.json /tmp/denylist.bak
printf 'this is not json {' > <DATA_DIR>/config/denylist.json
curl -sS -o /tmp/dl.json -w '%{http_code}\n' http://127.0.0.1:5137/denylist.json
jq -e '.users == []' /tmp/dl.json
cp /tmp/denylist.bak <DATA_DIR>/config/denylist.json
```
Expect: HTTP `200` (NOT 4xx/5xx), body `{"users":[]}` → `jq -e` prints `true`. Matches Unity's
catch-and-fail-open. A `tracing::warn!` "denylist parse failed; serving empty" should appear in logs.

### Check E — fail-open on missing file

```bash
BLOCKLIST_PATH=/tmp/does-not-exist.json \
  HTTP_SERVER_PORT=5147 cargo run -p catalyrst-explorer-api &  # alt port to avoid clashing
sleep 2
curl -sS -o /tmp/dl2.json -w '%{http_code}\n' http://127.0.0.1:5147/denylist.json
jq -e '.users == []' /tmp/dl2.json
```
Expect: HTTP `200`, body `{"users":[]}`. Confirms read-error fail-open and that `BLOCKLIST_PATH` env
override is honored.

### Check F — legacy extra fields tolerated

```bash
cp <DATA_DIR>/config/denylist.json /tmp/denylist.bak
printf '{"users":[],"names":[],"contents":[],"scenes":[]}' > <DATA_DIR>/config/denylist.json
curl -sS -w '\n%{http_code}\n' http://127.0.0.1:5137/denylist.json
cp /tmp/denylist.bak <DATA_DIR>/config/denylist.json
```
Expect: HTTP `200`; parses cleanly (legacy `names`/`contents`/`scenes` ignored), body served verbatim.

---

## 3. Real-client smoke (Unity)

Only the Unity explorer consumes the blocklist, so the live smoke targets it.

1. Apply the repoint from section 1 to a checkout of `unity-explorer`, editing `DecentralandUrlsSource.cs:207`.
2. Ensure the feature flag `REPORT_USER` is **off** so the file path is exercised (otherwise the moderation
   RPC is used and `/denylist.json` is never fetched).
3. Start the local service (section 2) with the seed file `{"users":[]}`.
4. Launch and authenticate the Unity client through your normal client driver.
   - **Not-banned path:** with the empty seed, startup proceeds past `BlocklistCheckStartupOperation` into
     the world with no "blocked" screen. Confirm via screenshot/logs that the loading flow completes.
5. **Banned path (negative):** stop the client, seed `denylist.json` with the exact authenticated wallet
   address used to sign in:
   ```bash
   printf '{"users":[{"wallet":"<YOUR_AUTH_WALLET>"}]}' > <DATA_DIR>/config/denylist.json
   ```
   Relaunch + auth. Expect the `BlockedScreenController` "banned" screen to appear (the client should not
   reach the world). Verify via screenshot.
6. Restore the seed: `printf '{"users":[]}' > <DATA_DIR>/config/denylist.json`.

Pass criteria: empty list → normal entry; matching wallet → blocked screen; service unreachable → client
fails open and still enters (no false ban).
