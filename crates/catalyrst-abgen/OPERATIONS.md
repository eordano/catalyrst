# catalyrst-abgen - operator reference

Env vars, HTTP surfaces, metrics, and latent surfaces. Verified against `src/`;
`grep -rn 'env::var' src/` is the ground truth.

## Binaries
| Binary | Source | What it is |
|---|---|---|
| `catalyrst-abgen` | `src/bin/catalyrst-abgen.rs` | Ab-cdn server: corpus serving + JIT conversion + folded registry routes. |
| `abgen` | `src/bin/abgen.rs` | Single-bundle CLI builder; exported standalone as `abgen-build`. |
| `abgen-corpus` | `src/bin/abgen-corpus.rs` | Batch corpus builder (manifest / `--entity-ids` / `--world <name>[,...]` / `--live` / `--collection` / `--from-ref`). `--world` resolves scene entities via `--worlds-url`, else `ABGEN_WORLDS_URL`, else the public worlds-content-server, fetches entity + content into the store, then converts. |
| `abgen-verify` | `src/bin/abgen-verify.rs` | Parity verifier against reference bundles; `gpu <diff|bench|corpus>` subcommands (GPU bit-exactness harness, `--features gpu` builds only). |
| `abgen-lod` | `src/bin/abgen-lod/` | Scene LOD generator (clean-room `lods-generator` port). |

Debug/decode examples (`cargo build --release --examples`): texdump, texpng, texcmp, texcmp2, matdump, crndump, objdump, jpegprobe, loddump, lodaabb, ressdump, atlasprobe.

## HTTP surface - binds `HTTP_SERVER_HOST:HTTP_SERVER_PORT` (default `127.0.0.1:5147`)
| Route | Method | Purpose |
|---|---|---|
| `/ping`, `/livez` | GET | Static `ok`. |
| `/readyz` | GET | `200 ready` / `503 degraded` (out-root missing, or JIT on with build/required templates missing). |
| `/health` | GET | `/readyz` status plus config/probe fields (`mode`, `out_root*`, `template_ok`, `bundle_index`, `turbojpeg`, `content_db`, `catalyst_url`, `ab_version`, `git_commit`, ...), a `lod_jit` object, and (gpu builds) a `gpu` object whose probe also sets the `abgen_gpu_qualified` gauge. |
| `/metrics` | GET | Prometheus text exposition (503 if the recorder failed to install). With `ABGEN_METRICS_BEARER_TOKEN` set, requires `Authorization: Bearer <token>` (else 401); unset = open. |
| `/entities/versions` | POST | `{"pointers":[...]}` (max 200) -> per-entity AB versions/bundles/status. |
| `/entities/active` | POST | Same input -> full active-entity records + AB status; content DB when connected, content-client fallback otherwise (loses `timestamp`/`deployer`). Optional `?world_name={name}` scopes resolution to that world's scene entities via `ABGEN_WORLDS_CONTENT_URL` (same record shape; pointer filter applies). |
| fallback | GET/HEAD | Serving surface: `/manifest/<entity>_<platform>.json`, `/LOD/<level>/<file>`, `/lods-unity/manifests/<sceneId>_InitialSceneState.json[.br]` (ISS descriptor), `/<version>/<entity>/<bundle>[.br]`, flat `/<version>/<hash>_<platform>[.br]` (pre-v25 clients), shader paths `/<version>[/<sceneId>]/dcl/scene_ignore_<p>`, `/<version>[/<sceneId>]/dcl/universal render pipeline/lit_ignore_<p>`, and `/<version>[/<sceneId>]/dcl/scene_texarray_ignore_<p>` (sceneId stripped - one shared object per canonical name), native content passthrough. On a 404 every lane first probes the S3 space (when configured), then its JIT: manifest/3-seg build in-process (per-`{entity}:{platform}` single-flight; failures negative-cached for `ABGEN_JIT_FAIL_TTL_S`); 3-seg `.br` requests never build (the JIT emits no `.br` sidecar) and 404 fast with `br-not-built`; LOD via the LOD JIT lane (`ABGEN_LOD_JIT`, default OFF, GET only; builds stage in the workdir and only gate-passed output is promoted into the serving root); flat resolves the owning entity for the bare hash (in-memory index -> content DB -> catalyst `/contents/{hash}/active-entities`) then builds and serves through the `{entity}/{platform}` layout; shader materializes the vendored payload (`ABGEN_SHADER_JIT`, default on). ISS misses only read through (the scene LOD JIT produces + writes back ISS descriptors). |
### 404 reasons (`x-abgen-reason` header)
| Reason | Meaning |
|---|---|
| `lod-not-built` | Valid path, no bundle on disk, no build attempted. |
| `lod-jit-disabled:<dep>` | Lane off: `env-off`, or `gltfpack` unresolvable under `ABGEN_SIMPLIFIER=gltfpack` (fail closed; the default meshopt backend is in-crate and needs no binary). |
| `lod-build-failed` | This request's build failed; gate-failed bundles are deleted, never served. |
| `lod-build-failed-cached` | Negative-cached failure (`ABGEN_LOD_JIT_FAIL_TTL_S`). |
| `lod-build-inflight` | Another request holds the build; wait exceeded the timeout. |
| `lod-build-timeout` | Build exceeded `ABGEN_LOD_JIT_TIMEOUT_S`; NOT negative-cached - the build keeps running under the single-flight lock (no overlapping rebuild); later requests find the bundle or the cached failure. |
| `lod-level-unsupported` | Level >= 2 (clients load 0/1). |
| `bad-path` | Malformed LOD path. |
| `hash-unresolved` | Flat `{hash}_{platform}` whose owning entity could not be resolved. Only a definitive not-found is negative-cached (`ABGEN_HASH_RESOLVE_FAIL_TTL_S`, keyed on the exact-case bare hash); resolver errors (content-DB/catalyst failures) return the miss without caching. |
| `br-not-built` | 3-seg `/{version}/{entity}/{bundle}.br` miss; the entity JIT never emits `.br` sidecars, so no build is attempted. Request the non-`.br` name or prime the sidecar into the space. |
| `iss-not-built` | ISS descriptor not on disk and not in the space; run/await the scene's LOD JIT or prime it. |
| `shader-unavailable` | Shader-shaped path whose canonical object is not on disk, not in the space, and not the vendored payload (mac/linux scene shaders + all `lit_ignore_*` need priming into the space with any S3 client until vendored). |

`/readyz` never degrades for LOD: missing LOD deps disable the lane without flipping readiness (LOD 404s are graceful by client contract); check `/health.lod_jit`.
The folded registry routes (profiles, worlds, status, queues, admin) merge in when their config/state build succeeds; otherwise a startup warn `folded registry routes disabled` and the server runs without them - watch for it.

## Metrics (recorder installed at server start; on failure `/metrics` serves 503, counters no-op)
| Metric | Type | Labels | Meaning |
|---|---|---|---|
| `catalyrst_http_requests_total` | counter | `method`, `route`, `status` | Every request (`<unmatched>` for fallback). |
| `catalyrst_http_request_duration_seconds` | histogram | `method`, `route` | Request latency. |
| `abgen_jit_builds_total` / `_build_duration_seconds` | counter / histogram | `outcome` = `ok`/`error`/`panic`/`fail-cached` (counter only) | JIT corpus-miss conversions + wall time (entity + flat lanes, per-`{entity}:{platform}` single-flight). |
| `abgen_jit_coalesced_total` | counter | - | Requests that found the target already materialized after waiting on the single-flight lock. |
| `abgen_index_jit_builds_total` / `_build_duration_seconds` | counter / histogram | `outcome` = `ok`/`error`/`panic` | Eager index-hit conversions (`ABGEN_INDEX_EAGER_BUILD`) + wall time. |
| `abgen_index_jit_skipped_total` | counter | - | Eager builds dropped by the `ABGEN_INDEX_BUILD_MAX_QUEUE` cap. |
| `abgen_gpu_qualified` | gauge | `backend` | 1 when the GPU engine qualified (set on `/health` probes; gpu builds only). |
| `abgen_bundle_index_entries` | gauge | - | Flat bundle-index size at startup. |
| `abgen_lod_jit_builds_total` / `_build_duration_seconds` | counter / histogram | `outcome` = `ok`/`error`/`gate_fail`/`timeout`/`panic` | LOD builds + wall time; a deadline hit records `timeout` plus the terminal outcome when the detached worker finishes. |
| `abgen_lod_jit_negcache_hits_total` | counter | - | Negative-cache short-circuits. |
| `abgen_lod_jit_coalesced_total` | counter | - | Waiters coalesced onto another request's build. |
| `abgen_space_lane_reads_total` | counter | `lane` = `lod`/`iss`/`flat`/`flat-alias`/`shader`, `outcome` | S3 read-through attempts on the non-3-seg lanes (`abgen_space_reads_total` keeps its original manifest/3-seg meaning). |
| `abgen_shader_jit_total` | counter | `outcome` = `ok`/`error` | Vendored-shader serve-time materializations. |

## Logging
- `RUST_LOG` - EnvFilter syntax; default `abgen=info,catalyrst_abgen=info,catalyrst_registry=info,tower_http=info`.
- `ABGEN_LOG_FORMAT` - `json` for JSON lines; anything else is text.
- Levels: `error` = requests will fail, `warn` = degraded-but-serving, `info` = config dump + milestones. The startup `abgen server config` line dumps the resolved config including the git commit.

## Environment variables
### Server config (`src/abcdn/config.rs`, read once at startup)
| Var | Default | Effect / risk |
|---|---|---|
| `HTTP_SERVER_HOST` | `127.0.0.1` | Bind address; "deployed but unreachable" usually means this was never set. |
| `HTTP_SERVER_PORT` | `5147` | Bind port; non-numeric is a hard startup error. |
| `ABGEN_OUT_ROOT` | `./data/ab-generator/out` | Corpus root. Unwritable: server still starts (`out_root_writable=false` in `/health`) but JIT output never persists - rebuilt per request. |
| `ABGEN_CATALYST_URL` | `http://127.0.0.1:5141/content` | Content server for JIT fetches; probed at startup (3s), warns when unreachable; corpus hits still serve, JIT misses fail. |
| `ABGEN_CONTENT_DISK` | unset | Local content-store checked before the remote catalyst; wrong path silently degrades to remote. |
| `ABGEN_CACHE_DIR` | `./abgen-serve-cache` | JIT working cache; blank = unset. |
| `ABGEN_VERSION` | `v41` | AB version segment in served paths and `/health.ab_version`. |
| `ABGEN_MANIFEST_CONTENT_SERVER_URL` | `https://peer.decentraland.org/content` | `contentServerUrl` baked into JIT manifests; wrong value silently poisons them. |
| `ABGEN_WORLDS_CONTENT_URL` | worlds-content-server.decentraland.org | Serve-lane content fallback: entity/content fetches that miss the primary source retry `{url}/contents/{hash}` (world entities are not on catalysts). Also resolves `POST /entities/active?world_name={name}` (scene entities via `/world/{name}/about`, pointer-filtered). `0`/`off`/empty disables both; the `world_name` param then falls back to plain pointer resolution with a warn. |
| `ABGEN_SHADER_JIT` | on | Serve-time materialization of the vendored shared shader bundle (`shader/scene_ignore_windows`, sha-pinned) on shader-path misses; `0`/`false`/`no`/`off` disables. Payload-limited: only the vendored file materializes; other canonicals still 404 `shader-unavailable` until primed. |
| `ABGEN_HASH_RESOLVE_FAIL_TTL_S` | `3600` | Negative-cache TTL for flat `{hash}_{platform}` requests whose owning entity definitively does not exist (exact-case bare-hash key; resolver errors are never cached). Protects the content DB / catalyst from unresolvable-hash stampedes. |
| `ABGEN_JIT_FAIL_TTL_S` | `60` | Negative-cache TTL for failed entity/flat JIT builds (keyed `{entity}:{platform}`); repeat requests inside the window 404 without re-building (`outcome="fail-cached"` on `abgen_jit_builds_total`). |
| `ABGEN_METRICS_BEARER_TOKEN` | unset | Non-empty = `/metrics` requires `Authorization: Bearer <token>`; unset/empty = open. |
| `ABGEN_INDEX_EAGER_BUILD` | on | `env_bool`; eager background conversion of buildable entities returned by `/entities/*` (missing-manifest platforms only). |
| `ABGEN_INDEX_BUILD_PLATFORMS` | `windows,mac` | Comma-separated platforms the eager index lane builds. |
| `ABGEN_INDEX_BUILD_CONCURRENCY` | available parallelism | Semaphore permits for eager index builds. |
| `ABGEN_INDEX_BUILD_DEADLINE_MS` | `20000` | The `/entities/*` handler awaits eager builds up to this before responding; builds keep running past it. |
| `ABGEN_INDEX_BUILD_MAX_QUEUE` | `0` (unbounded) | Pending-build cap; overflow increments `abgen_index_jit_skipped_total`. |
| `ABGEN_FALLBACK_VERSION` | `v41` | Extra version prefix probed on space reads (3-seg + shader lanes) after the primary `ABGEN_VERSION`. |
| `ABGEN_ROOT` | crate dir's parent (compile-time) | Root containing `template/`; a deployed binary without it 500s every JIT miss and `/readyz` degrades. |
### Content DB (folded entity index) + folded registry routes
| Var | Default | Effect / risk |
|---|---|---|
| `CONTENT_PG_CONNECTION_STRING` | unset | Full connection string; wins over `POSTGRES_*`. Configured-but-unreachable: startup warn (`content DB unavailable`) + fallback. |
| `POSTGRES_CONTENT_USER` | unset | Unset = no content DB; `/entities/*` fall back to the content client (no timestamp/deployer); `/health.content_db` = `fallback`. |
| `POSTGRES_HOST` | `./data/run` | Host or unix-socket dir. |
| `POSTGRES_PORT` | `6432` | |
| `POSTGRES_CONTENT_PASSWORD` | empty | |
| `POSTGRES_CONTENT_DB` | `content` | |

Registry routes (read by `catalyrst-registry`): `AB_REGISTRY_PG_CONNECTION_STRING`, `API_ADMIN_TOKEN`, `PROFILE_CDN_BASE_URL` (fallback `PROFILE_IMAGES_URL`), `DENYLIST_MODERATORS`, plus the shared vars above. Failures never stop the server; the routes vanish with a startup warn.
### Build behavior toggles (library; affect bundle bytes)
Presence-checked unless noted (`VAR=0` still enables); snapshotted into `BuildOpts` at each entry point - mid-process changes do nothing.

| Var | Default | Effect / risk |
|---|---|---|
| `ABGEN_COLLECTION_MODE` | off | Wearable/collection naming + emission rules. |
| `ABGEN_REAL_TEXTURES` | off | Full texture encode. The server JIT path forces this and `ABGEN_V38_COMPAT` on; CLI runs must pass `--real-textures`/`--v38-compat` or set the env, else placeholder-texture bundles. |
| `ABGEN_V38_COMPAT` | off | v38-era output compat (metadata TextAsset, timestamped metadata). |
| `ABGEN_V38_TIMESTAMP` | `0` | i64 timestamp in v38 metadata; unparseable becomes 0. |
| `ABGEN_MAGENTA_MISSING` | off | Missing textures render magenta (debug aid); OR'd with the `--magenta-missing` flags. |
| `ABGEN_CONTENT_ROOT` | `./content` | `abgen-corpus` content root when `--content-dir` absent; wrong dir = hard fetch errors. |
| `ABGEN_SHADER_BUNDLE` | `<crate>/shader/scene_ignore_windows` | Shader-bundle path override; sha256-verified, wrong file fails loudly. |
### GPU encode path
| Var | Default | Effect / risk |
|---|---|---|
| `ABGEN_GPU` | unset | Exactly `1` enables the BC7 GPU engine; any failure exits code 2 (no silent fallback). Legacy alias `CATALYRST_ABGEN_GPU` still honored. CLI: `abgen --gpu`, `abgen-corpus --gpu`, `abgen-lod --gpu`. |
| `ABGPUGEN_PTX` | embedded `kernel.ptx` | PTX path override; unreadable is a hard error. |
| `ABGPUGEN_BDIM` | `256` | Encode block dim (1-1024; out-of-range falls back to 256). |
| `ABGPUGEN_BATCH_DEV_BYTES` | `4000000000` | Device arena cap. |
| `ABGPUGEN_BINNING` | on | `0` disables the mode-bin scheduler. |
| `ABGPUGEN_MAXREG` | `128` | PTX JIT max registers; `0` = driver default. |
| `ABGPUGEN_GPU_LOG` | off | Per-batch GPU timing on stderr. |
| `CUDA_CACHE_PATH` / `CUDA_CACHE_MAXSIZE` | auto | Unset/unwritable -> `~/.cache/abgen-gpu-jit`, 1 GiB (avoids the ~29 s per-process JIT tax of a root-only box-wide path). |
### lodgen (`abgen-lod`)
| Var | Default | Effect / risk |
|---|---|---|
| `ABGEN_LOD_MANIFEST_BUILDER` | unset | `scene-lod-entities-manifest-builder` checkout; needed without `--iss`/`--manifest-builder`. Missing: CLI hard error; server lane warns, ISS scenes still build. |
| `ABGEN_SIMPLIFIER` | `meshopt` | Decimation backend (`meshopt`\|`gltfpack`); CLI `--simplifier` overrides. `meshopt` = in-crate meshoptimizer (no subprocess); unrecognized values warn + fall back to the default. |
| `ABGEN_GLTFPACK` | unset | gltfpack path; fallback `--gltfpack` then `PATH`. Only consulted when the effective simplifier is `gltfpack`; then missing everywhere: CLI hard error; server lane fails CLOSED (`lod-jit-disabled:gltfpack`). |
| `ABGEN_LOD_SUBPROC_TIMEOUT_S` | `0` (unlimited) | Deadline for lodgen child processes; the enabled server lane sets it to `ABGEN_LOD_JIT_TIMEOUT_S` unless already set (hung children are killed). |
| `HOME` | - | Default manifest-builder workdir (`~/.cache/abgen-lod/manifest-builder`). |
### LOD JIT lane (server; `src/abcdn/lodjit.rs`) - default OFF, misses get `lod-jit-disabled:env-off`
| Var | Default | Effect / risk |
|---|---|---|
| `ABGEN_LOD_JIT` | unset (OFF) | `env_bool` (`1`/`true`/`yes`/`on` case-insensitive; unrecognized warns + stays OFF); under `ABGEN_SIMPLIFIER=gltfpack` enables only if gltfpack resolves (fail closed, no auto mode); the default meshopt backend has no binary dependency. |
| `ABGEN_LOD_MANIFEST_BUILDER` | unset | As above; unset on an enabled lane = startup warn, non-ISS scenes fail. |
| `ABGEN_GLTFPACK` | unset | As above; probed once at startup (only under `ABGEN_SIMPLIFIER=gltfpack`). |
| `ABGEN_LOD_CACHE_DIR` | `ABGEN_CACHE_DIR` value | Base for `<base>/lod-content` and `<base>/lod-work`; builds serialized by a global semaphore (`ABGEN_LOD_BUILD_CONCURRENCY`, default 1). |
| `ABGEN_LOD_JIT_TIMEOUT_S` | `600` | Per-build wall clock AND max lock/slot wait; timed-out builds are NOT negative-cached - the detached build keeps the lock, so no overlap. |
| `ABGEN_LOD_JIT_FAIL_TTL_S` | `3600` | Negative-cache TTL keyed on `scene:level`. |

One request bakes BOTH levels (0 = undecimated, 1 = tri-capped at 500 x parcels) x windows+mac+linux from a single assemble/crop/atlas pass, plus one `LOD.manifest.json` listing every level/file and the ISS descriptor (served at `/lods-unity/manifests/`); the other five bundles then serve from disk. Production LOD URLs carry the lowercased entity id; an all-lowercase `qm*` sid is case-resolved to the exact CIDv0 through the content DB before the build (content DB absent or unmatched: the build fails on the case-sensitive content fetch; `bafkrei*` ids are already canonical). Builds run in a per-build staging dir under `<base>/lod-work`; only after the self-gate (incl. the `tri-cap` check on capped lanes) passes is the output promoted into the serving root (per-file atomic renames), so gate-failed or over-budget bundles are never servable, not even transiently. All writes are atomic (`*.tmp.<pid>` + rename). `/health.lod_jit` reports `enabled`, `simplifier`, `gltfpack`, `manifest_builder`, `disabled_reasons`, `neg_cache_entries`, `timeout_s`.
### Debug / parity knobs
| Var | Default | Effect / risk |
|---|---|---|
| `TURBOJPEG_LIB` | unset | libturbojpeg path override (nix builds bake a default); `/health.turbojpeg` reports load state. |
| `ABGEN_JPEG_TURBO_BOX` | off | Forces turbojpeg box-filter decode (parity/debug; changes texture bytes). |
| `ABGEN_JPEG_GLB_9C` | off | Forces libjpeg9c fancy upsampling for GLB JPEGs (parity/debug; changes texture bytes). |
| `ABGEN_BC7_SCALAR` | off | Disables AVX2+AVX-512 BC7 (identical bytes, ~10x slower). |
| `ABGEN_BC7_NO512` | off | Disables only AVX-512. |
| `ABGEN_BC7_CAPTURE` | unset | File path for BC7 block captures (mutex-serialized). |
| `ABGEN_BC7_CACHE` | unset | Content-addressed LZ4HC block cache dir (sha1-keyed); grows unboundedly. |
| `ABGEN_LZ4_DUMP` | unset | Dumps every pre-compression block; fills disks fast. |
### Test-only / build-time / deprecated
- `ABGEN_FAST_SERVE` - removed; the fast LZ4HC search (96 chain probes, sufficient-len 64) is now the only compression path. Setting the var does nothing. Output bytes intentionally diverge from the old optimal parse (fork byte-parity is a non-goal; the standard is load/render/behave parity).
- `ABGEN_TEST_CRN_OURS`/`_REF`/`_DUMP` - feed the `dxt5_crn_reference_compare` test in `src/bc5_pure.rs`; no-op when unset.
- `ABGEN_GIT_COMMIT` - baked by `build.rs` (`-dirty` suffix on a dirty tree); surfaces in `/health.git_commit`; setting it at runtime does nothing.
### S3 / Spaces cache - opt-in via `ABGEN_S3_BUCKET` (`src/space.rs`, SigV4 against any S3 endpoint)
- Enable: `ABGEN_S3_BUCKET` non-empty (or `ABGEN_USE_SPACE` truthy per `env_bool`); startup logs `ab-cdn S3 space cache ENABLED (read-through + write-back)`. Vars unset = lane off, ambient `AWS_*` ignored.
- Behavior: read-through on corpus miss (`fallback_version` keys, default `v41`); write-back of every JIT-generated bundle + manifest (`{version}/{cid}/{file}`, `manifest/{cid}_{platform}.json`).
- Lane keys (read-through GET order; write-back key): manifest `manifest/{entity}_{platform}.json`; 3-seg `{version}/{cid}/{file}` then `{fallback_version}/...`; LOD mirrors the URL verbatim `LOD/{level}/{file}` (a successful LOD JIT puts every produced `LOD/{level}/*` file plus the ISS descriptor); ISS mirrors the URL `lods-unity/manifests/{file}`; flat mirrors the URL `{url_version}/{hash}_{platform}[.br]` then the 3-seg alias once the owning entity resolves (a flat JIT also puts the flat alias so replicas hit without re-resolving); shader `{url_version}/{canonical}` then `{version}/...` then `{fallback_version}/...` with the sceneId stripped from the canonical (`dcl/scene_ignore_{p}`, `dcl/universal render pipeline/lit_ignore_{p}` - keys match the upstream ab-cdn layout, so priming = copy `v{N}/dcl/*` from production with any S3 client). Keys are SigV4-signed with RFC-3986 segment encoding (the `lit_ignore_*` keys contain spaces).
- Credential order: `ABGEN_S3_ACCESS_KEY` / `AWS_ACCESS_KEY_ID` + matching secrets, optional `ABGEN_S3_SESSION_TOKEN`/`AWS_SESSION_TOKEN` (signed as `x-amz-security-token`); then the ECS/Fargate container role (`AWS_CONTAINER_CREDENTIALS_{FULL,RELATIVE}_URI`, cached, refreshed 5 min before expiry).
- Endpoint: `ABGEN_S3_ENDPOINT` (default `ab-cdn.ams3.digitaloceanspaces.com`); region `ABGEN_S3_REGION`/`AWS_REGION` (default `ams3`).
- `ABGEN_S3_PATH_STYLE` (`env_bool`): REQUIRED with a bare regional endpoint - only path-style puts the bucket into the request.
- `ABGEN_S3_READ_ONLY` (`env_bool`): read-everywhere / write-nowhere; puts are skipped by the serve lane and logged in the startup `read_only` line.
- The boolean S3 knobs all parse with the shared `env_bool` grammar: `1`/`true`/`yes`/`on` | `0`/`false`/`no`/`off`, case-insensitive; unrecognized values warn and keep the default.
- PUTs send no `x-amz-acl` header (bucket privacy comes from the bucket policy / PublicAccessBlock; ACL-headered PUTs 400 on BucketOwnerEnforced buckets), and every `x-amz-*` header sent is SigV4-signed.
- All space HTTP calls share a 60s-timeout agent; a GET 403 is treated as a miss, warned once per process.
- Metrics: `abgen_space_reads_total{outcome="hit"|"miss"}` (manifest + 3-seg lanes, series shape unchanged), `abgen_space_lane_reads_total{lane="lod"|"iss"|"flat"|"flat-alias"|"shader",outcome}` (new lanes), `abgen_space_request_duration_seconds{op="get"|"put",result}`, `abgen_space_transfer_bytes_total{direction}`, `abgen_space_object_bytes`, `abgen_space_errors_total{op}` (transport/auth failures, distinct from misses); each hit/put also logs one structured line (key, bytes, ms). Alert on hit-ratio trend, error rate, duration p99 - not raw counts.
- No ops CLI: the server self-populates the space via JIT write-back (the former `abgen-space` bin was dropped; `src/space.rs` survives as the server's cache library). The batch converters (`abgen`, `abgen-corpus`) do not touch S3 inline; warm the bucket from CI with any S3 sync client after a batch build.

## wasm lane (browser lab; native services unaffected)
- `wasm-poc/` is a workspace-excluded cdylib that compiles the converter lib to `wasm32-unknown-unknown` for the browser demo under `site/wasm/`. The default catalyrst workspace build/check/test never touches it; no deployed service loads the module.
- Entry points: `bash wasm-poc/build.sh` rebuilds `site/wasm/abgen_poc.wasm` (gitignored; never committed); `bash wasm-poc/serve.sh` serves the demo on `127.0.0.1:5189`; `bash wasm-poc/test/parity.sh` is the cross-target byte gate (8 fixtures x windows/mac/webgl, sha256 per artifact, plus the scene LOD1 bake on windows/mac and its `.br`/`LOD.manifest.json` sidecars).
- Toolchain: pinned by `wasm-poc/toolchain/flake.nix` (rust 1.97.0 + wasm32 target, `pkgsCross.wasi32` clang for the vendored C/C++ codecs). The devShell exports `WASI_LIBC_LIB` and `WASI_LIBCXX_LIB` - the static `libc.a` / `libc++.a` + `libc++abi.a` dirs `wasm-poc/build.rs` links into the cdylib; building outside the devShell fails fast on those two vars.
- Parity contract (the decoder rule): the production native default for GLB JPEG decode stays turbojpeg - measured closest to the Unity upstream oracle. wasm has no dlopen and always decodes via the vendored libjpeg9c, so `parity.sh` exports `ABGEN_JPEG_GLB_9C=1` (the existing native escape hatch) on every native invocation to make both sides byte-identical inside the gate. With the env unset, native output is byte-identical to the pre-wasm-lane tree. Transcendentals in the byte path go through `src/detmath.rs` (pure-Rust libm) on both targets unconditionally.

## Latent / dead surfaces
- `shader::emit` (`src/shader.rs`) - emits the vendored shader bundle into an out dir; no callers in the tree (the serve-time shader lane uses `bundle_bytes_verified` directly; the module's other fns are load-bearing for cross-bundle PPtrs). Latent, intentionally kept.
