# E2E test plan — catalyrst-ab-cdn (ab-cdn.decentraland.org)

| | |
|---|---|
| key | `ab-cdn` |
| crate | `catalyrst-ab-cdn` |
| workspace | `<WORKSPACE>` |
| port | **5143** (`HTTP_SERVER_HOST=127.0.0.1`, `HTTP_SERVER_PORT=5143`) |
| env | `<ENV_FILE>` (`ABGEN_OUT_ROOT`) |
| upstream | `https://ab-cdn.decentraland.org` |
| state | stateless — disk + OS page cache only (no Postgres/Redis/S3) |

The CDN is a thin, content-addressed static file server over abgen's output
tree (`ABGEN_OUT_ROOT`). It auto-detects both abgen layouts per request:

- corpus:   `{root}/{entity}/{hash}_{platform}`
- manifest: `{root}/{entity}/{platform}/{hash}_{platform}` + `{root}/{entity}/{platform}.manifest.json`

---

## 1. Unity config — how to repoint

**This is a hardcoded, static Unity URL — NOT `/about`-discovered.** The realm's
`/about` response does not carry the asset-bundle CDN URL; the explorer builds it
purely from the `DecentralandUrl.AssetBundlesCDN` enum in `DecentralandUrlsSource`.
Editing our `/about` does nothing here — you must edit the Unity source.

Proof it is static (not realm-dependent): `RawUrl(...)` returns a bare `string`
for `AssetBundlesCDN`, which goes through `UrlData`'s implicit
`string -> UrlData` operator that hardcodes `CacheBehaviour.STATIC`
(`DecentralandUrlsSource.cs:274-275`). It is absent from the `RealmDependent(...)`
wrappers and from `ResetRealmDependentUrls`, so realm changes never invalidate it.

### Files

- **Enum:** `Explorer/Assets/DCL/Infrastructure/Utility/DecentralandUrls/DecentralandUrl.cs:67`
  → `AssetBundlesCDN = 41,`
- **URL template (the line to change):**
  `Explorer/Assets/DCL/NetworkDefinitions/Browser/DecentralandUrlsSource.cs:197`

  ```csharp
  DecentralandUrl.AssetBundlesCDN => $"https://ab-cdn.decentraland.{ENV}",
  ```

  Repoint to your host by replacing line 197 with a literal (no `{ENV}`
  substitution, if your host has no per-env suffix):

  ```csharp
  DecentralandUrl.AssetBundlesCDN => "http://127.0.0.1:5143",
  ```

  Use the LAN IP / hostname instead of `127.0.0.1` if the explorer runs on a
  different machine than the CDN. No trailing slash — consumers append their own
  path (`StaticContainer.cs:249` and `GlobalWorldFactory.cs:166` both wrap it in
  `URLDomain.FromString(...)` and append `/{version}/{entity}/{file}`).

### Consumers to be aware of (no edit needed, just for smoke-test reasoning)

- `Explorer/Assets/DCL/Infrastructure/Global/StaticContainer.cs:249` —
  `AssetBundlesPlugin` base URL.
- `Explorer/Assets/DCL/Infrastructure/Global/Dynamic/GlobalWorldFactory.cs:166` —
  `assetBundleCdnUrl`.
- `Explorer/Assets/DCL/NetworkDefinitions/Browser/GatewayUrlsSource.cs:44` —
  routes the URL through a gateway override layer; if a gateway override is
  active it can rewrite the host, so confirm no gateway override masks the repoint.

> Note: the bevy-explorer / godot-explorer clients resolve the AB CDN host from
> their own config, not this Unity enum. For a bevy smoke test see §3.

---

## 2. Local service checks (curl)

### 2.0 Bring the service up

```bash
# load env + run the binary from the workspace
set -a; source <ENV_FILE>; set +a
cargo run -p catalyrst-ab-cdn --manifest-path <WORKSPACE>/Cargo.toml
# expect log: "catalyrst-ab-cdn listening addr=127.0.0.1:5143 root=..."
# (if ABGEN_OUT_ROOT does not exist yet it logs a WARN and every fetch 404s)
```

Seed a fixture tree if abgen output is empty (lets the positive-path checks pass):

```bash
ROOT=$(grep -oP 'ABGEN_OUT_ROOT=\K.*' <ENV_FILE>)
# corpus layout
mkdir -p "$ROOT/bafkreitestentity"
printf 'UnityFS-fake' > "$ROOT/bafkreitestentity/abcdhash_windows"
# manifest layout + native manifest json
mkdir -p "$ROOT/bafkreitestentity/mac"
printf 'UnityFS-fake-mac' > "$ROOT/bafkreitestentity/mac/abcdhash_mac"
printf '{"version":"v41","files":{},"exitCode":0,"date":"2026-06-09"}' \
  > "$ROOT/bafkreitestentity/windows.manifest.json"
```

### Checks (each: command + expected)

1. **Manifest, byte-exact passthrough**
   ```bash
   curl -si http://127.0.0.1:5143/manifest/bafkreitestentity_windows.json
   ```
   Expect `200`, `Content-Type: application/json`, `Cache-Control: public, max-age=600`,
   body byte-equal to the on-disk `windows.manifest.json` (no re-serialization).

2. **Manifest, unknown entity → 404 JSON error**
   ```bash
   curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5143/manifest/doesnotexist_windows.json
   curl -s http://127.0.0.1:5143/manifest/doesnotexist_windows.json
   ```
   Expect `404` and body `{"error":"unknown entity"}`.

3. **Versioned binary, explicit entity (corpus layout)**
   ```bash
   curl -si http://127.0.0.1:5143/v41/bafkreitestentity/abcdhash_windows
   ```
   Expect `200`, `Content-Type: application/octet-stream`,
   `Cache-Control: public, max-age=31536000, immutable`, `ETag: "abcdhash_windows"`,
   `Content-Length` matching file size.

4. **Versioned binary, explicit entity (manifest/nested-platform layout)**
   ```bash
   curl -sI http://127.0.0.1:5143/v41/bafkreitestentity/abcdhash_mac
   ```
   Expect `200` (resolved via `{entity}/mac/abcdhash_mac`), octet-stream, immutable.

5. **Flat fallback — version + filename only (no entity segment)**
   ```bash
   curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5143/v41/abcdhash_windows
   ```
   Expect `200` (resolver scans every entity dir for the filename).

6. **HEAD request — headers only, no body**
   ```bash
   curl -sI http://127.0.0.1:5143/v41/bafkreitestentity/abcdhash_windows
   ```
   Expect `200` with `Content-Length`/`ETag`/`Cache-Control` present and empty body.

7. **LOD route — registered before the generic version route**
   ```bash
   ROOT=$(grep -oP 'ABGEN_OUT_ROOT=\K.*' <ENV_FILE>)
   mkdir -p "$ROOT/lodentity"; printf 'LODFS' > "$ROOT/lodentity/lodhash_windows"
   curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5143/LOD/3/lodhash_windows
   ```
   Expect `200` — `LOD/3` is treated as the version prefix, not an AB version.

8. **Brotli sidecar negotiation**
   ```bash
   ROOT=$(grep -oP 'ABGEN_OUT_ROOT=\K.*' <ENV_FILE>)
   printf 'brotli-bytes' > "$ROOT/bafkreitestentity/abcdhash_windows.br"
   curl -sI -H 'Accept-Encoding: br' http://127.0.0.1:5143/v41/bafkreitestentity/abcdhash_windows
   ```
   Expect `200`, `Content-Encoding: br`, `Vary: Accept-Encoding`,
   `Content-Length` = size of the `.br` sidecar.
   Then without the header — expect NO `Content-Encoding`, identity bytes:
   ```bash
   curl -sI http://127.0.0.1:5143/v41/bafkreitestentity/abcdhash_windows
   ```

9. **Path-traversal rejection**
   ```bash
   curl -s -o /dev/null -w '%{http_code}\n' --path-as-is \
     http://127.0.0.1:5143/v41/bafkreitestentity/..%2f..%2f..%2fetc%2fpasswd
   curl -s -o /dev/null -w '%{http_code}\n' --path-as-is \
     http://127.0.0.1:5143/manifest/..%2f..%2fsecret_windows.json
   ```
   Expect `404` for both — no segment with `..`/`/`/`\`/NUL ever reaches disk.

10. **Missing binary → 404 plain text**
    ```bash
    curl -si http://127.0.0.1:5143/v41/bafkreitestentity/nope_windows
    ```
    Expect `404`, `Content-Type: text/plain`, body `not found`.

11. **CORS preflight / wildcard origin**
    ```bash
    curl -sI -H 'Origin: https://play.decentraland.org' \
      http://127.0.0.1:5143/v41/bafkreitestentity/abcdhash_windows
    ```
    Expect `Access-Control-Allow-Origin: *` on the response.

12. **Parity against upstream (optional, run when ABGEN_OUT_ROOT is populated from real scenes)**
    Pick a known entity id + bundle filename that exists in both, and diff:
    ```bash
    ENT=<real-entity>; F=<real-hash>_windows
    curl -s http://127.0.0.1:5143/v41/$ENT/$F | sha1sum
    curl -s https://ab-cdn.decentraland.org/v41/$ENT/$F | sha1sum
    ```
    Expect identical sha1 (bytes are content-addressed → must match exactly).
    Same for the manifest:
    ```bash
    diff <(curl -s http://127.0.0.1:5143/manifest/${ENT}_windows.json) \
         <(curl -s https://ab-cdn.decentraland.org/manifest/${ENT}_windows.json)
    ```

### Deferred / out-of-scope (should NOT be served by this crate)

- `POST /entities/versions`, `GET /entities/*`, `GET /worlds/*` — these belong to
  the asset-bundle-registry host/crate, not ab-cdn. Confirm they return `404`
  here (no route registered) so we don't accidentally shadow the registry:
  ```bash
  curl -s -o /dev/null -w '%{http_code}\n' -X POST http://127.0.0.1:5143/entities/versions
  ```
  Expect `404`/`405` (no such route).

---

## 3. Real-client smoke test

The goal: a scene loads its asset bundles from OUR CDN instead of upstream, with
no visual regression (correct meshes/textures, no fallback-to-GLTF spam).

### Option A — bevy-explorer (preferred, lighter)

bevy-explorer reads the AB CDN host from its own config, so this validates the
served-bytes contract without touching Unity source.

1. Point the bevy client's asset-bundle / "ab-cdn" base URL override at the
   local CDN (`http://127.0.0.1:5143`); the exact config key is documented by
   the bevy client.
2. Launch the client and teleport to a populated scene whose bundles exist
   under `ABGEN_OUT_ROOT`.
3. Capture a frame; meshes/textures should render from AB, not GLTF.

Pass criteria: scene renders with bundled assets; CDN logs show `200`s for
`/v{N}/{entity}/{file}` and `/manifest/...`; no repeated 404s.

### Option B — upstream Unity client

Requires the source edit in §1, then a rebuild.

1. Edit `DecentralandUrlsSource.cs:197` → `"http://127.0.0.1:5143"` (see §1).
   Confirm no `GatewayUrlsSource` override masks it.
2. Rebuild the Unity client and launch it, then sign in and teleport to a scene
   whose bundles exist under `ABGEN_OUT_ROOT`.
3. Observe: tail the catalyrst-ab-cdn log — expect a burst of `200`s on
   `/{version}/{entity}/{file}` and `/manifest/{entity}_{platform}.json` as the
   scene streams in. Take a screenshot and confirm assets render (no all-GLTF
   fallback, no missing meshes).

Pass criteria: zero AB fetches hit `ab-cdn.decentraland.org` (all go to
`127.0.0.1:5143`); scene visually matches the upstream-CDN baseline.

---

## 4. Notes / known gaps

- `ABGEN_OUT_ROOT` (`<DATA_DIR>/ab-generator/workdir/out`) does not exist
  until abgen runs. Until then every fetch 404s (by design). The §2 fixture seed
  is the way to exercise positive paths before abgen is wired up.
- Manifest `Cache-Control` is `max-age=600` (regenerable), binaries are
  `immutable` (content-addressed) — assert both, they differ on purpose.
- ETag is the content-addressed filename (with any `.br` suffix trimmed), so a
  brotli response and its identity sibling share the same ETag — expected.
