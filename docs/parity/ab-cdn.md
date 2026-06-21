# Parity report: catalyrst-ab-cdn (service `ab-cdn`)

Reference: `decentraland/asset-bundle-converter` (abgen output paths + S3/CloudFront
serving policy) and the explorer fetch expectations (Unity `SceneAbDto`,
`UpdateSceneLODInfoSystem`). Upstream "CDN" is a static S3 bucket behind
CloudFront — there is no app server; the only producer logic is the abgen
uploader (`consumer-server/src/logic/conversion-task.ts`). Our crate is a
DB-free on-disk file server over abgen's output root.

No findings were flagged for adversarial re-check. The full verdict set was
sanity-verified against both the Rust source and the upstream TS / net-catalog;
every cited line was confirmed. Nothing was rejected.

## Per-endpoint table

| Endpoint | Shape | Efficiency | Severity | Notes |
|---|---|---|---|---|
| `GET /ping` | match | same | none | Local health check returning static `"ok"` (text/plain); no upstream counterpart. main.rs:14. |
| `GET/HEAD /manifest/{entityId}{platform}.json` | match | same | none | Byte-passthrough of abgen-written JSON (no serde struct here). Headers byte-identical to upstream. |
| `GET/HEAD /{version}/{sceneId}/{filename}` (v25+ hash-in-path AB) | match | same | none | Opaque octet-stream; immutable + ETag + Range/206 + 304, matching S3/CloudFront. |
| `GET/HEAD /{version}/{filename}` (legacy pre-v25; 2-seg `.br` webgl) | match | same | none | Same serve_binary path; best-effort resolve, clean negative-cached 404 where abgen never wrote a flat layout. |
| `GET/HEAD /LOD/{level}/{sceneId}_{level}{platform}` | match | same | none | Opaque octet-stream LOD bundle; same immutable framing. Confirmed shape in net-catalog. |

## Confirmed shape issues

None. Every shape is `match`.

- The manifest endpoint has **no Rust struct that could rename/recase/retype a
  field**: serve.rs:95 streams the raw file bytes verbatim. Field-level parity
  (`version`, `files`, `exitCode`, `contentServerUrl?`, `date`) is owned by the
  abgen producer, not this crate. Verified: there is no serde model in the crate.
- The binary/LOD endpoints carry opaque `application/octet-stream` bodies — no
  JSON to diff. HTTP framing (Content-Type, Cache-Control, ETag, Range/206,
  304, CORS) was confirmed byte-identical to the upstream S3 object policy:
  - manifest `Cache-Control: private, max-age=0, no-cache` + `application/json`
    matches conversion-task.ts:66-68 (serve.rs:101-102).
  - binary `Cache-Control: public, max-age=31536000, immutable` matches
    conversion-task.ts:178 (serve.rs:155).
  - manifest key convention `manifest/{entityId}_{target}.json` (webgl = bare
    `{entityId}.json`) matches conversion-task.ts:106-110; resolver.rs:57-70
    strips the `_windows/_mac/_linux` suffix (empty = webgl) to recover the
    entity id.
  - net-catalog confirms the client actually reads
    `/manifest/{hash}{platform}.json`, `/manifest/{sceneId}{platform}.json`, and
    `/LOD/1/bafk..._1_mac` — exactly the shapes the crate serves.

### Out-of-scope caveat (abgen producer, not this crate)

Upstream `Manifest` (conversion-task.ts:23) types `exitCode: number | null` and
`contentServerUrl?: string`. Unity `SceneAbDto` reads only `{version, files,
exitCode, date}` with `exitCode` as a non-nullable `int`, and ignores
`contentServerUrl`. If abgen ever emitted `exitCode: null`, Unity's int
deserializer would coerce to `0`. This is an **abgen-rs producer concern,
invisible to the CDN's byte passthrough** — the crate under review cannot cause
or fix it. Noted, not counted as a shape issue for this lane.

## Confirmed efficiency wins (with structural reason)

None rise to a "better than upstream" verdict — all endpoints are `same`. The
structural reasons:

- **DB-free on both sides.** config.rs:6-9 confirms no SQL; the only state is the
  abgen output root. Upstream is a pure S3 bucket behind CloudFront with no app
  server/ORM. There is no N+1 or per-item lookup on either side to win against.
- Our read path adds a moka path-resolution cache (50k cap, 60s TTL, negative
  caching of misses so repeated 404s don't re-scan the dir — state.rs:11-31),
  `spawn_blocking` fs metadata + casing fallback on miss, and a zero-copy
  `ReaderStream` whole-file stream (serve.rs:73-76). For immutable binaries,
  If-None-Match/304 short-circuits **before any fs touch** (serve.rs:126-143),
  matching S3 conditional GET. These are minor origin-side niceties, partly
  negated for manifests by the `no-cache` header, and orthogonal to CloudFront's
  geo-distributed edge cache (a topology advantage, not per-request origin
  efficiency). Net: structurally equivalent at the origin → `same`.

## Rejected during verification

Nothing. The findings list to scrutinize was empty, and the sanity pass over the
full verdict set found no incorrect "match"/"same" verdict and no over-claimed
"better" efficiency win. All cited source lines (serve.rs, resolver.rs,
handlers.rs, state.rs, config.rs; conversion-task.ts:23-24/66-68/106-110/178/266;
net-catalog LOD/manifest shapes) were opened and confirmed real.
