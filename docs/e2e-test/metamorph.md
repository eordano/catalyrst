# E2E Test Plan — `metamorph` (catalyrst-media)

Reimplements `metamorph-api.decentraland.org` `/convert`. Crate `catalyrst-media`,
port **5144**, no DB, no Redis/S3. First pass is a stateless passthrough: every
`/convert` is a cache-MISS → `302 Found` to the original URL with **no**
`image/ktx2` Content-Type, so the Unity client takes its uncompressed
(`ExecuteNoCompressionAsync`) decode path. Correct, just not KTX-compressed yet.

Workspace: `<WORKSPACE>`

---

## 1. Unity config — how to repoint the client

The `metamorph` host is **static (NOT `/about`-discovered)**. It is a hard-coded
`RawUrl(...)` template baked into the explorer build and only varies by `{ENV}`
(`org`/`zone`/`today`). It is never read from a realm's `/about` response, so it
**must be changed in Unity** — editing our `/about` does nothing here.

### Where it is defined
File: `<UNITY_EXPLORER>/Explorer/Assets/DCL/NetworkDefinitions/Browser/DecentralandUrlsSource.cs`

Line **217**:
```csharp
DecentralandUrl.MediaConverter => $"https://metamorph-api.decentraland.{ENV}/convert?url={{0}}",
```
- Enum `DecentralandUrl.MediaConverter = 57` is defined in
  `.../DCL/Infrastructure/Utility/DecentralandUrls/DecentralandUrl.cs:91`.
- `{ENV}` (`const string ENV = "{ENV}"`, line 33) is substituted with the
  Decentraland domain (`org`/`zone`/`today`) at runtime via
  `rawUrl.ToString().Replace(ENV, decentralandDomain)` (line 98).
- `{{0}}` is the positional slot for the URL-encoded original asset URL; callers
  do `string.Format(urlsSource.Url(DecentralandUrl.MediaConverter), Uri.EscapeDataString(originalUrl))`.

### Consumers (sanity — confirms it's just this one URL)
- `.../DCL/WebRequests/Texture/GetTextureWebRequest.cs:41` — texture fetch path.
  Note: only used when `useKtx` is true (`textureArguments.UseKtx && ktxEnabled
  && !WebRequestUtils.IsLocalhost(url)`); branches on response `Content-Type ==
  "image/ktx2"` (line 62).
- `.../DCL/Infrastructure/ECS/StreamableLoading/NFTShapes/LoadNFTTypeSystem.cs:41`
  — NFT image shape path, same gating on `ktxEnabled`.
- Also listed in `GatewayUrlsSource.cs:58` (gateway-routed URL set).

### How to repoint to our host
Edit line 217 to point at the local service. For a local node listening on
`127.0.0.1:5144` (keep the `?url={{0}}` slot exactly):
```csharp
DecentralandUrl.MediaConverter => $"http://127.0.0.1:5144/convert?url={{0}}",
```
This drops the `{ENV}` substitution (intentional — local host has no env
domain). If you prefer to keep env-templating against a deployed catalyrst host,
use e.g. `$"https://metamorph.catalyrst.{ENV}/convert?url={{0}}"` and add that
host to the catalyrst gateway. **Caveat:** `GetTextureWebRequest` skips the
converter entirely for localhost asset URLs (`IsLocalhost(url)`), and the whole
path is gated by the `ktxEnabled` feature flag — to exercise `/convert` the
client must have KTX enabled and be loading a non-localhost asset.

**This is a Unity-side edit. It is NOT `/about`-discovered.**

---

## 2. Local service e2e checks

Build + run the service first (from the workspace root):
```bash
cd <WORKSPACE>
cargo run -p catalyrst-media
# listens on 127.0.0.1:5144 (HTTP_SERVER_HOST/HTTP_SERVER_PORT)
```

`curl -i` shows status + headers; `-s -o /dev/null -w` asserts the redirect
target and that NO `image/ktx2` Content-Type / NO `Cache-Control` is set on the
cold-cache redirect (this is what routes the client to its non-KTX path).

### Health
```bash
# Expect: 200, Content-Type text/plain; charset=utf-8, body "OK"
curl -i http://127.0.0.1:5144/health/live
```

### Convert — happy path (whitelisted host, cache MISS → 302 to original)
```bash
# Expect: 302 Found; Location == the original url; NO image/ktx2 CT; NO Cache-Control
curl -i "http://127.0.0.1:5144/convert?url=https%3A%2F%2Fpeer.decentraland.org%2Fcontents%2FQmFoo"
# Assert Location + absence of immutable cache header:
curl -s -o /dev/null -D - "http://127.0.0.1:5144/convert?url=https%3A%2F%2Fpeer.decentraland.org%2Fcontents%2FQmFoo" | grep -iE '^(HTTP/|location:|content-type:|cache-control:)'
```

### Convert — extra params accepted-and-ignored (still 302 to original)
```bash
# imageFormat/videoFormat/wait/forceRefresh accepted, ignored in pass 1 → still 302
curl -i "http://127.0.0.1:5144/convert?url=https%3A%2F%2Fpeer.decentraland.org%2Fa.png&imageFormat=UASTC&videoFormat=MP4&wait=true&forceRefresh=true"
```

### Convert — off-whitelist host (metered+logged but STILL redirected)
```bash
# Expect: 302 Found, Location == original (a redirect is not an SSRF proxy);
# metamorph_convert_off_whitelist_total increments (see /metrics below)
curl -i "http://127.0.0.1:5144/convert?url=https%3A%2F%2Fevil.example.com%2Fx.png"
```

### Convert — missing url (model validation → 400)
```bash
# Expect: 400 Bad Request, body "query parameter 'url' is required"
curl -i "http://127.0.0.1:5144/convert"
```

### Convert — malformed / non-http(s) url (→ 400)
```bash
# Expect: 400; body "query parameter 'url' must be an absolute http(s) URL"
curl -i "http://127.0.0.1:5144/convert?url=not-a-url"
curl -i "http://127.0.0.1:5144/convert?url=ftp%3A%2F%2Fhost%2Ff"
curl -i "http://127.0.0.1:5144/convert?url=javascript%3Aalert(1)"
```

### Convert — HEAD mirrors GET (content-type/size sniffing path)
```bash
# Expect: 302 Found, same Location header, empty body (axum get-route answers HEAD)
curl -I "http://127.0.0.1:5144/convert?url=https%3A%2F%2Fpeer.decentraland.org%2Fa.png"
```

### Metrics — open when WKC_METRICS_BEARER_TOKEN unset
```bash
# Expect: 200, text/plain prometheus exposition with the three counters:
#   metamorph_convert_requests_total / _off_whitelist_total / _bad_request_total
curl -i http://127.0.0.1:5144/metrics
```

### Metrics — bearer guard when token set
```bash
# Start with WKC_METRICS_BEARER_TOKEN=secret in env, then:
# Expect: 401 with no/incorrect token; 200 with correct bearer
curl -i http://127.0.0.1:5144/metrics
curl -i -H "Authorization: Bearer wrong" http://127.0.0.1:5144/metrics
curl -i -H "Authorization: Bearer secret" http://127.0.0.1:5144/metrics
```

### Cache-key parity (unit-level, already covered by `cargo test`)
`cache_key(url) == sha256_hex(url)` — the upstream `RedisKeys.GetS3Key` lookup
key. Verify the hex matches the host tool:
```bash
printf '%s' 'https://peer.decentraland.org/a.png' | sha256sum
# must equal cache_key() in crates/catalyrst-media/src/modules/convert.rs
cargo test -p catalyrst-media   # 5 unit tests (cache_key + whitelist) must pass
```

---

## 3. Real-client smoke (dcl-bevy / dcl-walk)

The Unity refclient (`dcl-walk`) is the right driver because `MediaConverter` is
a Unity-only static URL and the KTX branch lives in Unity's
`GetTextureWebRequest`. bevy-explorer does not consume this enum.

1. Apply the line-217 edit above pointing `MediaConverter` at
   `http://127.0.0.1:5144/convert?url={0}` in a unity-explorer checkout, and
   ensure the KTX feature flag (`ktxEnabled`) is on.
2. Start the service: `cargo run -p catalyrst-media` (port 5144).
3. Launch and drive the client (see the `dcl-explore` skill before non-trivial
   `dcl-walk` use):
   ```bash
   dcl-walk launch
   dcl-walk auth-sign
   # teleport to a parcel with NFT image shapes / textured content so the
   # texture loader exercises the MediaConverter URL on a non-localhost asset
   ```
4. Watch the service logs: each textured asset should produce a
   `convert cache-miss -> redirect to original` debug line with the SHA-256
   `cache_key`, and `metamorph_convert_requests_total` should climb on `/metrics`.
5. **Expected behavior:** textures/NFTs render correctly via the uncompressed
   decode path (because the cold-cache 302 carries no `image/ktx2` Content-Type).
   Confirm with `dcl-walk` screenshot — no broken/missing textures. KTX
   compression is the deferred second pass; pass-1 success = visually correct
   images served through our redirect.

---

## 4. Deferred (second pass — out of scope for pass-1 e2e)
- Real transcode: images→KTX2/UASTC/ASTC via the asset-bundle generator
  (`abgen` encode + `local_store`), videos→MP4/OGV via an ffmpeg background
  worker, keyed by `sha256(url)+format` like upstream `RedisKeys.GetS3Key`.
- Cache-HIT branch: `302` to the converted CDN artifact (CDN base URL from the
  service's environment file) with `image/ktx2`/`video/mp4` Content-Type and
  `Cache-Control: public, max-age=31536000, immutable`. When implemented, add an
  e2e check asserting a warm-cache `/convert` returns that Content-Type +
  immutable header, and that the client then takes its `ExecuteKtxAsync` path.
