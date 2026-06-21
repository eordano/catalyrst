# E2E test plan — catalyrst feature-flags (key=`feature-flags`)

Reimplementation of `feature-flags.decentraland.org`.

- Crate: `catalyrst-explorer-api`
- Port: configured via `HTTP_SERVER_PORT` (default in `crates/catalyrst-explorer-api/src/config.rs`). The examples below use `5137` for the deployment's assigned port.
- Handler: `crates/catalyrst-explorer-api/src/modules/feature_flags.rs`
- Snapshot data: `<DATA_DIR>/config/feature-flags.json` (loaded via `FEATURE_FLAGS_CONFIG_PATH`)

## 1. How the explorer reaches this host, and how to repoint it

This URL is a **static enum-driven URL, NOT `/about`-discovered**. It is not part of
the realm `/about` payload, so it is repointed by editing Unity, not our `/about` response.

### Exact source lines (Unity)

- Enum value:
  `Explorer/Assets/DCL/Infrastructure/Utility/DecentralandUrls/DecentralandUrl.cs`
  line 62: `FeatureFlags = 38,`

- `RawUrl(...)` mapping:
  `Explorer/Assets/DCL/NetworkDefinitions/Browser/DecentralandUrlsSource.cs`
  **line 192**:
  ```csharp
  DecentralandUrl.FeatureFlags => $"https://feature-flags.decentraland.{ENV}",
  ```
  `{ENV}` is replaced (in `Probe`/`Url`) by the environment domain (`org`, `zone`, `today`).
  This is `CacheBehaviour.STATIC` (implicit string → `UrlData`), so it does NOT depend on
  realm or feature-flag state and is never reset on realm change.

### How the full request URL is assembled (so you know what to serve)

`Explorer/Assets/DCL/FeatureFlags/FeatureFlagOptions.cs`
- `AppName = "explorer"` (line 26)
- `URL = decentralandUrlsSource.Url(DecentralandUrl.FeatureFlags)` (line 27) → the host above
- `Hostname = decentralandUrlsSource.GetHostnameForFeatureFlag()` (line 29) → sent as `referer`

`Explorer/Assets/DCL/FeatureFlags/HttpFeatureFlagsProvider.cs`
- line 27: appends path `{AppName}.json` → final fetch URL is `<host>/explorer.json`
- lines 31-35: sets request headers `X-Debug`, `referer`, optional `X-Address-Hash`
- line 42: `StripAppNameFromKeys("explorer", response)` strips the `explorer-` key prefix client-side

So the only route the client hits is **`GET <host>/explorer.json`**.

### How to repoint (pick one)

**Option A — edit the enum mapping directly (most precise).**
In `DecentralandUrlsSource.cs` line 192, replace the literal with your host. Because the
client appends `/explorer.json`, point at the bare host root (no trailing slash, no path):
```csharp
DecentralandUrl.FeatureFlags => "http://127.0.0.1:5137",
```
(or whatever host:port fronts the service from inside the client sandbox). Drop the `$"...{ENV}"`
interpolation so all environments resolve to your service. Result: client fetches
`http://127.0.0.1:5137/explorer.json`.

**Option B — front it behind the prod hostname.** Leave Unity untouched and map
`feature-flags.decentraland.org` → `127.0.0.1:5137` at the reverse-proxy/hosts layer used by
your test environment. Note that this hostname→service mapping must be created before relying on
this option.

Either way: do **NOT** touch `/about`. This enum is not realm-discovered.

## 2. Local service e2e checks

Start the service first, pointing it at the captured snapshot:

```bash
cd /path/to/catalyrst
FEATURE_FLAGS_CONFIG_PATH=<DATA_DIR>/config/feature-flags.json \
HTTP_SERVER_PORT=5137 \
cargo run -p catalyrst-explorer-api
```

### Check 1 — primary route returns 200 + correct shape
```bash
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5137/explorer.json
# expect: 200
curl -s http://127.0.0.1:5137/explorer.json | jq 'has("flags") and has("variants")'
# expect: true
```

### Check 2 — flag/variant counts match the captured production snapshot
```bash
curl -s http://127.0.0.1:5137/explorer.json | jq '.flags | length'      # expect: 49
curl -s http://127.0.0.1:5137/explorer.json | jq '.variants | length'   # expect: 11
```

### Check 3 — keys stay app-prefixed (`explorer-...`) on the wire
```bash
curl -s http://127.0.0.1:5137/explorer.json | jq -r '.flags | keys[]' | grep -c '^explorer-'
# expect: 49 (every flag key prefixed; client strips the prefix itself)
```

### Check 4 — served payload is byte/structurally equal to the snapshot file
```bash
diff <(curl -s http://127.0.0.1:5137/explorer.json | jq -S .) \
     <(jq -S . <DATA_DIR>/config/feature-flags.json)
# expect: no output (identical)
```

### Check 5 — variant wire shape (payload optional, `type`/`value` round-trip)
```bash
curl -s http://127.0.0.1:5137/explorer.json \
  | jq '.variants | to_entries[0].value | {name, enabled, has_payload: has("payload")}'
# expect: an object with name(string), enabled(bool); has_payload may be true or false.
# Confirm a payload, when present, has exactly type+value:
curl -s http://127.0.0.1:5137/explorer.json \
  | jq '[.variants[] | select(.payload) | (.payload | keys)] | unique'
# expect: [["type","value"]]
# Confirm the known payload-less variant round-trips with NO payload key:
curl -s http://127.0.0.1:5137/explorer.json \
  | jq '.variants["explorer-alfa-communities"] | has("payload")'
# expect: false
```

### Check 6 — identity headers accepted but ignored (v1 serves a static snapshot)
```bash
curl -s -o /dev/null -w '%{http_code}\n' \
  -H 'X-Debug: true' \
  -H 'referer: https://decentraland.org' \
  -H 'X-Address-Hash: 0xdeadbeef' \
  http://127.0.0.1:5137/explorer.json
# expect: 200, and the body is identical to Check 1 (headers do not change output in v1)
```

### Check 7 — arbitrary appName serves the same snapshot (any `*.json`)
```bash
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5137/anything.json   # expect: 200
diff <(curl -s http://127.0.0.1:5137/anything.json | jq -S .) \
     <(curl -s http://127.0.0.1:5137/explorer.json | jq -S .)
# expect: no output (identical snapshot for any appName)
```

### Check 8 — non-`.json` single segment 404s (catch-all must not shadow siblings)
```bash
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5137/explorer   # expect: 404
```

### Check 9 — literal sibling `/denylist.json` still wins precedence over the catch-all
```bash
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5137/denylist.json  # expect: 200
# and it must NOT return the feature-flags shape:
curl -s http://127.0.0.1:5137/denylist.json | jq 'has("flags")'   # expect: false
```

### Check 10 — debug alias `/flags/{name}` (not used by explorer, smoke only)
```bash
curl -s http://127.0.0.1:5137/flags/explorer-alfa-communities | jq '{name, enabled}'
# expect: { "name": "explorer-alfa-communities", "enabled": true }
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5137/flags/does-not-exist
# expect: 404
```

## 3. Real-client smoke step (dcl-bevy / dcl-walk)

Bevy/Godot do not consume this Unity URL enum, so the meaningful client smoke is via the
upstream Unity client (`dcl-walk` / `dcl-editor`). Steps:

1. Repoint per section 1 Option A (edit `DecentralandUrlsSource.cs` line 192 to
   `http://127.0.0.1:5137`) in the unity-explorer workspace, then build the client.
2. Ensure the local service is running (section 2) and reachable from the client sandbox
   on `5137` (verify with Check 1 from inside the client environment if networking is namespaced).
3. Launch and watch the feature-flags fetch:
   ```bash
   dcl-walk launch
   dcl-walk auth-sign
   # tail logs for the FEATURE_FLAGS report category
   dcl-rig logs | grep -i 'feature.flag\|explorer.json'
   ```
   Expect a successful `GET .../explorer.json` (200) and `FeatureFlagsConfiguration`
   initialized (not `IsEmpty`).
4. Functional confirmation: pick a flag that gates visible UI (e.g.
   `explorer-alfa-communities` → communities entry point; key after client strips the
   `explorer-` prefix is `alfa-communities`). With the flag `true` in the snapshot, the
   gated feature should be present in-client. Capture a screenshot:
   ```bash
   dcl-rig shot /tmp/ff-smoke.png
   ```
5. Negative check (optional): flip a flag to `false` in
   `<DATA_DIR>/config/feature-flags.json`, restart the service, relaunch, confirm the
   gated feature disappears. Restore the snapshot afterward.

## 4. Deferred / out-of-scope for v1

- Per-request Unleash strategy evaluation (hostname / gradualRollout / userId activation).
  v1 ignores `X-Debug`, `referer`, `X-Address-Hash` (Check 6) and serves one static
  snapshot. When this lands, add per-header assertions: same key, different `enabled` for
  different `X-Address-Hash` buckets, and `X-Debug: true` exposing debug-only flags.
- No reverse-proxy/hosts mapping for `feature-flags.decentraland.*` → the service yet; required for
  section 1 Option B.
