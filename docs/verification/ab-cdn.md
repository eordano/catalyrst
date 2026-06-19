# Verification: catalyrst-ab-cdn (service `ab-cdn`, bundle port 5147)

Branch: feat/service-plane-crates. Crate: `crates/catalyrst-ab-cdn`.
Upstream reference: `decentraland/asset-bundle-converter` (the abgen producer; the
"CDN" itself is a static S3 bucket behind CloudFront — there is no upstream app
server, only the uploader at `consumer-server/src/logic/conversion-task.ts`).
Our crate is a DB-free, on-disk file server over abgen's output root.

Verified by reading every crate source file, matching route shapes against the
Unity consumers and the net-catalog (`unity-net-catalog/catalog.db`), reading the
Unity load-system error path, and inspecting the deployed env file + bundle docs.

## Routing model

ONE catch-all plus health/ping:
- `lib.rs:22-29` `api_router()`: `GET /health`, `GET|HEAD /{*path}` -> `handlers::dispatch`
- `main.rs:29` mounts `GET /ping` -> static `"ok"`, then `.merge(api_router())`.

`handlers::dispatch` (handlers.rs:24-79) classifies by leading segment + count:

| Request shape | dispatch branch | resolver | serve fn |
|---|---|---|---|
| `manifest/{name}.json` (2 seg) | handlers.rs:33-42 | `manifest_path` (resolver.rs:27) -> `{root}/{entityId}/{platform}.manifest.json` | `serve_manifest` (serve.rs:66) |
| `LOD/{level}/{filename}` (3 seg) | handlers.rs:44-53 | `lod_path` (resolver.rs:51) -> `{root}/{sceneId}/LOD/{level}/{filename}` | `serve_binary` (serve.rs:92) |
| `{version}/{sceneId}/{filename}` (3 seg) | handlers.rs:55-64 | `binary_path` (resolver.rs:42) -> `{root}/{entity}/{platform}/{filename}` | `serve_binary` |
| `{version}/{filename}` (2 seg, not `manifest`) | handlers.rs:66-76 | `binary_path` (entity = platform-stripped filename) | `serve_binary` |
| anything else | handlers.rs:78 | — | `serve_404` (CORS `*`) |

Platform suffix table (resolver.rs:3-7): `_windows`/`_mac`/`_linux`, else `webgl`.
Matches Unity `PlatformUtils.GetCurrentPlatform()` (`_windows`/`_mac`/`_linux`/empty)
exactly. The deployed env-file comment (catalyrst-ab-cdn.env:8) confirms the same
layout `{ABGEN_OUT_ROOT}/{entityId}/{platform}.manifest.json` and
`{entityId}/{platform}/{filename}`.

## Per-endpoint table

| endpoint | shape | client-reaction | severity | failure-modes-ok | notes |
|---|---|---|---|---|---|
| `GET /ping` | `"ok"` text/plain | none — not client-called | none | YES | liveness only; no upstream counterpart |
| `GET /health` | inline JSON `{status,out_root,out_root_present}`, 200 if out_root is dir else 503 | none — ops probe, not in net-catalog | none | YES — degrades to 503, never panics | out_root currently MISSING on disk ⇒ live `/health`=503 |
| `GET\|HEAD /manifest/{hash}{platform}.json` | byte-passthrough of `{platform}.manifest.json`, `Content-Type: application/json`, `Cache-Control: private,max-age=0,no-cache`, CORS; 404 text on miss | JSON -> `SceneAbDto`; miss/garbage/`int.Parse` throw all caught by LoadSystemBase try/catch ⇒ recoverable scene-load failure, NOT a crash | low | YES — every fs/IO error ⇒ `None` ⇒ 404; no 500/panic | header policy byte-identical to upstream `uploadEntityManifest` (conversion-task.ts:56-69); no Rust DTO |
| `GET\|HEAD /{version}/{sceneId}/{hash}` (3-seg scene AB, v25+) | octet-stream, strong ETag, immutable cache, 304 on INM, 206/416 for non-br, `Content-Encoding: br` for `.br` | bytes -> `UnityWebRequestAssetBundle`; miss/IO ⇒ caught load failure | low | YES — open/seek/read/metadata err ⇒ `None` ⇒ 404; unparseable Range ⇒ full 200; unsatisfiable ⇒ 416 + `Content-Range */size` | matches upstream `uploadDir` policy (immutable, br+plain); no DTO (Unity AB binary) |
| `GET\|HEAD /{version}/{hash}` (2-seg legacy scene AB) | same binary framing | same | low | YES | `hasHashInPath=false` layout; same consumer |
| `GET\|HEAD /LOD/{level}/{sceneId}_{level}{platform}[.br]` | same binary framing | LOD AB bytes; miss ⇒ caught load failure | low | YES | net-catalog confirms literal shape via WebRequestStressTestUtility.cs:13 |
| any other / malformed / traversal | 404 `"not found"` text + CORS | n/a | none | YES — `is_safe_component` rejects `../`,`/`,`\`,NUL,empty ⇒ `None` ⇒ 404 (never 400/500) | mirrors S3/CloudFront 403/404 on unknown keys |

## Upstream / client wiring (corroborating detail)

- `DecentralandUrl.AssetBundlesCDN = https://ab-cdn.decentraland.{ENV}`
  (DecentralandUrlsSource.cs:197); wired in AssetBundlesPlugin
  (StaticContainer.cs:249) and scene/hybrid load (GlobalWorldFactory.cs:166).
- Manifest callers: `LoadAssetBundleManifestSystem.cs:56-63`,
  `LoadHybridSceneSystemLogic.cs:80-86` -> `SceneAbDto`
  (SceneAbDto.cs:10-19: `{version, files, exitCode:int, date}`; ignores
  `contentServerUrl`; `exitCode` non-nullable int, an abgen-producer concern not
  this crate's).
- AB callers: `PrepareAssetBundleLoadingParametersSystemBase.cs:67,91-97` build
  `{version}/{sceneID}/{hash}` and `{version}/{hash}`;
  `GetAssetBundleWebRequest.cs:22-27` feeds `UnityWebRequestAssetBundle` (binary,
  no DTO). LOD: `UpdateSceneLODInfoSystem.cs:89-100`.

## Confirmed issues

1. **Port collision: ab-cdn pinned to :5143, same as the explore bundle.**
   CONFIRMED, live, NEW (not in prior report). `config.rs:16` defaults to 5143
   AND the deployed override `env/catalyrst-ab-cdn.env:5` sets
   `HTTP_SERVER_PORT=5143`. Docs put ab-cdn on 5147 (docs/bundles.md:24,
   docs/deploy/runbook.md:14,153,198) and explore on 5143 (docs/bundles.md:20).
   No `5147` reference exists anywhere in `env` or the deployment units. If
   the explore bundle and ab-cdn both start, the loser of the bind race exits
   non-zero (`main.rs:36` `?` on `TcpListener::bind`). This is a deploy-time
   service-down failure (and the runbook's `curl localhost:5147/...` checks hit
   nothing), NOT a per-request panic. Severity: medium. Fix is one line in the
   env file (set 5147).

2. **Startup is panic-free with no DB / external services.** CONFIRMED.
   `build_state` (lib.rs:18-20) only wraps a PathBuf + in-memory moka cache; no
   fs touch at boot. `Config::from_env` (config.rs:13-20) fails only on an
   unparseable `HTTP_SERVER_PORT`; all else defaults. Missing out_root does NOT
   block startup — verified: `ABGEN_OUT_ROOT` does not currently exist on disk,
   yet the process binds and serves (assets 404, `/health` 503).

3. **No 500/panic path; HTTP-semantics parity not JSON-API parity.** CONFIRMED.
   Every fs/IO failure across serve.rs (resolve 32-50, manifest 72-83, INM 304
   103-120, range 141-173, stream 176-189) and resolver.rs (resolve_with_casing
   69-88) collapses to `Option::None` and surfaces as 404. 416 = empty body +
   `Content-Range bytes */size` (serve.rs:144-149). 503 only from `/health`. A
   transient disk error is indistinguishable from a missing object but never
   panics or 500s — matches S3+CloudFront (404 miss / 206 range / 304
   conditional).

4. **No `.unwrap()` on request-derived data.** CONFIRMED. Every
   `.parse().unwrap()` in serve.rs (16-23,85-88,112-116,128-138,147,166-168,187)
   and handlers.rs:93 is on a static string literal or `to_string()` of a
   `u64`/`format!` — all infallible `HeaderValue::from_str`. Range parsing
   (`parse_range_header`, formatters.rs:41-79) is `.ok()?` throughout: malformed
   ranges -> `None` (ignored, full 200), valid-unsatisfiable -> `Unsatisfiable`
   -> 416. No panic on hostile input.

5. **No response-shape divergence on data endpoints.** CONFIRMED (upholds prior
   report). Manifest and all AB/LOD bodies are raw byte passthrough (no Rust
   DTO), so no field rename/recase/retype risk. Only `/ping` and `/health` have
   Rust-defined shapes and neither is client-called.

## Client-crash risks

NONE. Every Unity consumer of `ab-cdn` runs inside
`LoadSystemBase.FlowInternalAsync`, whose driver wraps the whole flow in
`try {…} catch (Exception e) { result = new StreamableLoadingResult<TAsset>(GetReportCategory(), e); }`
(LoadSystemBase.cs:195-198). A 404, a body that fails Newtonsoft parse into
`SceneAbDto`, or the `int.Parse(version.AsSpan().Slice(1))` throw in
`CheckSceneAbDTO` (LoadAssetBundleManifestSystem.cs:73) all become a caught,
recoverable load failure (scene/asset fails to load, logged) — never a process
crash. `SceneAbDto` is a value-type struct with defaultable fields and is never
dereferenced before the try/catch boundary, so there is no null-deref or
required-field assertion reachable from a CDN response.

## Failure-mode gaps

NONE material. Two recorded nuances, both acceptable parity rather than gaps:
- A transient disk read error (EIO mid-stream, `read_dir` failure in
  `resolve_with_casing` resolver.rs:69-88) is reported as 404, identical to a
  genuinely-absent object, and is sticky for the 60s resolve-cache TTL
  (state.rs:23). S3 also 404s a missing key with no retry signal; never
  escalates to 500/panic.
- Malformed `Range` headers are silently ignored (full 200 instead of 416) —
  RFC-7233-lenient and matches CloudFront.

## Verdict

All crate-level claims hold on the committed tree: panic-free startup with no
external dependencies, no 500/panic path (all fs/IO errors collapse to 404), no
request-derived `.unwrap()`, byte-passthrough bodies with no DTO divergence, and
correct HTTP-semantics parity with the real S3+CloudFront CDN. No client-crash
risk exists. The one real defect is a deployment misconfiguration — both the
config default and `catalyrst-ab-cdn.env` pin port 5143, colliding with the
explore bundle while docs require 5147 — a one-line env fix, not a
code-correctness or parity bug.
