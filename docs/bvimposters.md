# catalyrst-bvimposters — binding contract

Status: AS-BUILT. Implemented and live on saturn: the umbrella dev twin
runs `catalyrst/target/debug/catalyrst-bvimposters` on `127.0.0.1:5154`, serve-only
(`BVIMPOSTERS_BAKE_ENABLED=0`); the rendered unit `umbrella-catalyrst-bvimposters.service`
is linked, prod start human-gated. Store `/home/dcl/one/umbrella/data/bvimposters`:
8,228 zips / 1.9 GB of a 20 GiB budget — ~6,360 seeded from the official-realm cache
(6,475 corpus spec triples; crc0/incomplete skipped), 1,798 CDN read-through fills,
70 locally baked. Campaign (±32 ring around 0,0, levels 0-2, time-boxed 25 min):
3,841 non-empty targets, 2,670 visited, 2,633 served 200 (98.6% of visited; 37
missing, 1,171 unvisited at the time box), 0 errors. Bake-on-miss proven E2E: level-0
tile 0,0 went 404-enqueue → bake → 200 `x-bvi-source: store` in 142 s wall (one impost
run harvested 70 zips); the level-1 plaza tile timed out at 1,500 s and quarantined
correctly. Client hook committed in bevy-explorer (419275c04), inert until the next
wasm deploy sets `imposter_url_base` + `imposter_realm_key`. Read-through
quarantine shipped (section 4b) — 587 upstream-defective #FF00FF tiles listed and
renamed out of `store/`, dev twin restarted on the new binary. The remainder of this
document is the binding contract between the service, the client hook, and ops;
deviations require updating this file first.

Problem: the play client renders octahedral boimp scene imposters beyond ~65m, fetched
as zips from a hardcoded community CDN keyed by realm `about_url` + parcel + level +
content-derived crc32. On our deploy every fetch 404s because the realm key differs,
even though identical-crc bakes exist under the official realm path. This service owns
the imposter supply: crc-keyed, realm-independent, read-through to the community CDN on
miss, bake-on-miss locally so redeploy staleness self-heals.

## 1. Identity

- Crate: `/home/dcl/one/catalyrst/crates/catalyrst-bvimposters`, registered in the
  workspace `members` list of `/home/dcl/one/catalyrst/Cargo.toml`. Modeled on
  `catalyrst-events` (envcfg `handle_standard_args`, tracing default filter
  `catalyrst_bvimposters=info,tower_http=info`, axum router + `TraceLayer`, `/ping`).
  DB-less: no postgres pool, no ts-rs exports, no `ts` feature.
- Bind: `127.0.0.1:5154` (verified free; neighbors metabase :5153, catalyrst-economy
  :5155). Do NOT use 5137 (catalyrst-explorer-api, live), 5140 (retired content-server,
  stale-reference risk), 5171 (reserved by `up-dev.sh` for the editor-scene preview).
- Store root: `/home/dcl/one/umbrella/data/bvimposters` (service creates it).
- Hard house rules: no Rust comments anywhere in the crate; `cargo fmt` gate; build via
  `nix develop` devShell in `~/one/catalyrst` (memory `catalyrst-build-verify.md`).

## 2. Key model

The only key is `(level, x, y, crc)`. Realm never enters the key.

- `level` ∈ 0..=5.
- `(x, y)`: signed parcel coordinates of the level-aligned tile origin. Alignment
  invariant (arithmetic-shift floor, matches the client): `(x >> level) << level == x`
  and same for `y`. Unaligned keys are invalid.
- `crc`: u32, decimal, as computed by the client (CRC_32_CKSUM over active scene entity
  id strings, XOR-rotate cascade for levels ≥ 1 — pointers.rs:92-148). `crc == 0` is
  the empty-tile sentinel; the client never fetches it. The service treats `crc=0` as
  invalid (404, never bake, never read-through).

Filename grammar (identical to upstream CDN objects): `{x},{y}.{crc}.zip`. Parse rule:
strip `.zip`, `rsplit` once on `.` for crc, split remainder on `,` for x,y. Negative
coordinates are common.

Zip payload (byte contract, identical to upstream; `zip::CompressionMethod::Stored`):
three members named `{x},{y}-spec.json`, `{x},{y}.boimp`, `{x},{y}-floor.boimp`. The
spec json is `{"imposters":{"x,y":{scale,region_min,region_max,overhang}},"crc":N}`;
its embedded `crc` MUST equal the filename crc (client validates it on load).

## 3. Routes

All routes GET, all responses body-complete (no chunked bake streaming).

| Route | Behavior |
|---|---|
| `/ping` | catalyrst convention: 200, echoes request path via `OriginalUri` |
| `/status` | 200 JSON: `{store_bytes, store_entries, budget_bytes, bake_enabled, bake_queue: [key...], bake_inflight: key\|null, quarantine: [{key, until, failures}], readthrough_quarantine: {path, keys}}` |
| `/imposters/realms/{realm}/{level}/{x},{y}.{crc}.zip` | the supply route, below |
| `/imposters/realms/{realm}/{level}/{x},{y}.{crc}-spec.json` | debug: extract and return the spec member from the stored zip; 404 if the zip is not in the store (no read-through, no bake) |

The `{realm}` path segment is ACCEPTED AND IGNORED — that is the realm-independence
mechanism. Axum decodes it once, so an unmodified client pointed at us (wire path
`https%253A%252F%252F...`) and our re-keyed client (segment `content`) both land on the
same handler and the same store entry. The handler must never percent-decode, join, or
canonicalize the segment further, and must never use it in store paths or upstream
URLs. Canonical segment for our client hook: literally `content`.

Validation before any store/CDN/bake work: level in range, alignment invariant, crc
parseable and nonzero. Any failure → 404 (not 400: the client treats non-2xx uniformly
as Missing and cache pollution is worse than status pedantry).

Success response headers: `content-type: application/zip`,
`cache-control: public, max-age=31536000, immutable`, `etag: "{crc}"`, plus diagnostic
`x-bvi-source: store | cdn` (load-bearing for the curl matrix).

The service must tolerate being mounted behind a prefix-stripping front proxy (the
serving lane owns nginx; e.g. `/bvimposters/` → strip → `/imposters/...`). No absolute
self-URLs in responses.

## 4. Serve pipeline (per request, after validation)

1. STORE HIT: `{store}/store/{level}/{x},{y}.{crc}.zip` exists → touch mtime, serve.
   If the file fails to open/read → quarantine-rename it to `{store}/evicted/` and fall
   through to miss (self-heal). A store hit serves even for keys on the read-through
   quarantine list (the list gates refills, not serving; quarantining an existing entry
   is the rename in section 4b).
2. MISS → READ-THROUGH QUARANTINE GATE: if the full key `(level, x, y, crc)` is on the
   read-through quarantine list (section 4b), SKIP the CDN entirely and fall through to
   step 3 as if the CDN missed. This is the containment for upstream-defective bakes
   (the baked-in #FF00FF av placeholder, `bevy-explorer/docs/magenta-root-cause.md`):
   without the gate, a quarantine-rename would just re-fetch the identical defective
   bytes from the community CDN on the next request. A local re-bake that lands at the
   same key serves normally again via step 1.
   Otherwise READ-THROUGH (synchronous, inline): fetch
   `{BVIMPOSTERS_CDN_BASE}/imposters/realms/{BVIMPOSTERS_CDN_REALM_SEGMENT}/{level}/{x},{y}.{crc}.zip`
   with timeout `BVIMPOSTERS_READTHROUGH_TIMEOUT_SECS`. The realm segment env value is
   the LITERAL wire segment (double-encoded); build the URL by string concatenation and
   never run any encoder over it — reqwest preserves existing percent-encoding but any
   `urlencoding::encode` pass corrupts the key. On 200: stream to
   `{store}/tmp/{uuid}`, verify it is a readable zip whose spec member crc equals the
   requested crc, fsync, rename into the store, serve the bytes (`x-bvi-source: cdn`).
   Serving inline rather than 404-then-backfill matters: the client records a non-2xx
   as `Missing` for that `(parcel, level, crc)` for the session (manager.rs:199-209)
   with no re-poll.
3. CDN MISS (non-2xx) or CDN error/timeout: if `BVIMPOSTERS_BAKE_ENABLED=1` and the
   key is not quarantined, enqueue a bake (section 5) and return 404. The client
   renders fog for 404 and recovers on re-entry/realm change; that is the accepted UX
   while a bake runs. If bake is disabled, plain 404.

Store writes are always temp-file + fsync + rename within the same filesystem.
Concurrent identical requests during read-through coalesce on a per-key async lock
(single upstream fetch; followers serve the landed file).

## 4b. Read-through quarantine list

Containment for store entries whose upstream bake is defective (the
587 tiles carrying exact-#FF00FF av-placeholder texels, enumerated by the magenta
forensics scan; the PinkPonyClub crc 3577122314 tiles are excluded pending a composite
check).

- List file: `BVIMPOSTERS_QUARANTINE_LIST`, default
  `{BVIMPOSTERS_STORE_ROOT}/readthrough-quarantine.txt` — repo convention: the live list
  lives at `/home/dcl/one/umbrella/data/bvimposters/readthrough-quarantine.txt`
  (uncommitted, `data/` is gitignored; regenerate from the forensics manifest
  `exact-blobs.json`). Missing file = empty list. One store key per line,
  `{level}/{x},{y}.{crc}` with optional `.zip` suffix; blank lines and `#` comments
  skipped; unparseable lines are counted and warned at load. Loaded once at boot —
  restart the service (dev twin: `systemctl --user restart
  umbrella-dev-catalyrst-bvimposters.service`, unsandboxed) to pick up edits. `/status`
  reports `readthrough_quarantine: {path, keys}`.
- Semantics: listed keys never read through to the CDN (serve pipeline step 2). A listed
  key still present under `store/` keeps serving; once renamed out, requests 404 (or
  enqueue a bake when bake is enabled), and a fresh local bake landing at the same key
  serves again.
- Quarantining existing entries is an operator action:
  `catalyrst-bvimposters quarantine [list-file]` (list defaults to the configured path)
  renames every listed key from `store/{level}/` to `{store}/quarantined/{level}/`
  (same-filesystem atomic rename, same pattern as the `evicted/` self-heal rename, but
  persistent: `quarantined/` is never scanned, budgeted, or drained, so the defective
  bytes stay parked for forensics and the rename is reversible). Prints
  `renamed/absent/errors` counts; idempotent. Ordering matters live: restart the service
  with the list BEFORE renaming, or a concurrent request re-fills the defective bytes
  through the read-through window.

Bake requests are keyed by `(level, x, y)` WITHOUT crc: the bake produces whatever crc
our content currently yields, which by construction matches what the client computes
(both derive from catalyst.dcl.one active entities). A request-crc that never gets
baked is simply a stale client view.

- Queue: bounded FIFO of distinct keys, capacity `BVIMPOSTERS_BAKE_QUEUE_DEPTH`
  (default 1, max 2). Duplicate enqueues coalesce; full queue drops the enqueue
  silently (request already got its 404). One worker task; at most one impost
  subprocess alive at any time (keyed-lock is the single worker + coalescing set).
- Invocation for key `(level, x, y)`, tile extent `E = 1 << level` parcels:

```
${BVIMPOSTERS_BAKE_WRAPPER} ${BVIMPOSTERS_IMPOST_BIN} \
  --server ${BVIMPOSTERS_IMPOST_SERVER} \
  [--content-server ${BVIMPOSTERS_IMPOST_CONTENT_SERVER}] \
  --location "{x + E/2},{y + E/2}" \
  --range {max(E/2, 1)} \
  --levels {level} \
  --threads 4 \
  --no-download \
  --zip-output {store}/staging/{job_id}
```

  (level 0: `--location "{x},{y}" --range 1 --levels 0`.) `--content-server` is
  emitted only when `BVIMPOSTERS_IMPOST_CONTENT_SERVER` is non-empty, and the default
  is EMPTY (verified): impost rewrites any override to `{override}/content/`
  and then appends `/entities/active`, producing a double-slash URL that 404s on our
  content core AND the public front; the 404 permanently deadlocks impost's pointer
  fetch (bevy-explorer lifecycle drops failed parcels and only re-arms on focus-parcel
  change, which never happens for a fixed-location baker). With no override impost uses
  the realm about's `https://catalyst.dcl.one/content` and works. `BVIMPOSTERS_BAKE_WRAPPER`
  is an optional command prefix (default empty) that provides the display env.
  VERIFIED: impost runs on a bare Vulkan adapter with NO compositor — the
  committed wrapper `umbrella/scripts/bvimposters-bake-env.sh` (WGPU_BACKEND=vulkan +
  NVIDIA ICD + a known-good nix-store vulkan-loader on LD_LIBRARY_PATH; some store
  copies of the loader fail dlopen with ENOENT, the wrapper pins a verified one) is
  sufficient; the rig sway+wayvnc stack (`/home/dcl/one/rig/lib`) remains the fallback.
  The prebuilt impost also needs the fork fix that disables
  `bevy::post_process::PostProcessPlugin` in `src/bin/impost.rs` — the effect-stack
  pipeline specializes for every camera and rejects the boimp Rg32Uint bake target,
  killing the app ("Quitting the application due to Validation RenderError"). Prerequisite (one-time, ops): impost
  spawns sibling `target/debug/dcl_deno_ipc`; `cargo build -p dcl_deno_ipc` in the
  bevy-explorer devshell must have completed in the same profile or every bake dies at
  startup ("failed to spawn deno binary").
- Harvest: on exit 0, walk `{store}/staging/{job_id}/imposters/realms/*/{lvl}/*.zip`
  for ALL levels ≤ requested (impost emits the whole pyramid inside the extent —
  harvest everything, it is free warm), verify each zip's spec crc equals its filename
  crc, rename into `{store}/store/{lvl}/`, then delete the staging dir. The realm dir
  impost writes under (`enc(about_url)` of `--server`) is discarded — only
  level/tile/crc survive.
- Timeout `BVIMPOSTERS_BAKE_TIMEOUT_SECS` (default 1800): on expiry kill the process
  GROUP (impost spawns dcl_deno_ipc; `killpg`, same trap as abgen npm) and count a
  failure.
- Poison quarantine: per bake key, consecutive-failure counter persisted at
  `{store}/quarantine.json`. After `BVIMPOSTERS_BAKE_MAX_FAILURES` (default 3) the key
  is quarantined for `BVIMPOSTERS_BAKE_QUARANTINE_SECS` (default 86400): requests for
  it 404 without enqueue, `/status` lists it. Success clears the counter. The file is
  loaded at boot so restarts do not reset a poison loop.

## 6. Store + byte-budget LRU

Layout under `BVIMPOSTERS_STORE_ROOT` (default `/home/dcl/one/umbrella/data/bvimposters`):

```
store/{level}/{x},{y}.{crc}.zip     served objects, mtime = last-served
tmp/                                read-through in-flight temp files
staging/{job_id}/                   impost --zip-output roots, deleted after harvest
evicted/                            quarantine-rename target, emptied by the evictor
quarantined/{level}/{...}.zip       read-through-quarantined objects (section 4b), kept
quarantine.json                     bake poison state
readthrough-quarantine.txt          read-through quarantine list (section 4b)
```

Budget `BVIMPOSTERS_STORE_MAX_BYTES` (default 21474836480 = 20 GiB). Evictor runs
after every store insert and every 10 minutes: sum sizes under `store/`; while over
budget, rename oldest-mtime entries into `evicted/` then unlink (rename-first so a
concurrent serve fails cleanly into the miss path instead of reading a torn file).
Entries younger than 1 hour are never evicted (protects just-baked output). Stale-crc
siblings of the same tile are ordinary LRU citizens — no special casing in v1. `tmp/`
and `staging/` older than 24h are swept at boot and by the periodic pass.

## 7. Config, env file, unit, dev twin

Env keys (all `: "${VAR:=default}"`-style overridable, documented in the crate's
`ENV_DOCS` table for `handle_standard_args`):

| Key | Default |
|---|---|
| `HTTP_SERVER_HOST` | `127.0.0.1` |
| `HTTP_SERVER_PORT` | `5154` |
| `BVIMPOSTERS_STORE_ROOT` | `/home/dcl/one/umbrella/data/bvimposters` |
| `BVIMPOSTERS_STORE_MAX_BYTES` | `21474836480` |
| `BVIMPOSTERS_CDN_BASE` | `https://bevy-imposters.dclregenesislabs.xyz` |
| `BVIMPOSTERS_CDN_REALM_SEGMENT` | `https%253A%252F%252Frealm-provider-ea.decentraland.org%252Fmain%252Fabout` |
| `BVIMPOSTERS_READTHROUGH_TIMEOUT_SECS` | `30` |
| `BVIMPOSTERS_QUARANTINE_LIST` | `{store_root}/readthrough-quarantine.txt` |
| `BVIMPOSTERS_BAKE_ENABLED` | `0` |
| `BVIMPOSTERS_BAKE_WRAPPER` | (empty) |
| `BVIMPOSTERS_IMPOST_BIN` | `/home/dcl/one/bevy-explorer/target/debug/impost` |
| `BVIMPOSTERS_IMPOST_SERVER` | `https://catalyst.dcl.one` |
| `BVIMPOSTERS_IMPOST_CONTENT_SERVER` | (empty = omit the flag; any value breaks impost, see §5) |
| `BVIMPOSTERS_BAKE_QUEUE_DEPTH` | `1` |
| `BVIMPOSTERS_BAKE_TIMEOUT_SECS` | `1800` |
| `BVIMPOSTERS_BAKE_MAX_FAILURES` | `3` |
| `BVIMPOSTERS_BAKE_QUARANTINE_SECS` | `86400` |

Files, following the events pattern exactly:

- Committed template `/home/dcl/one/umbrella/env/catalyrst-bvimposters.env.template`;
  live env `/home/dcl/one/umbrella/env/catalyrst-bvimposters.env` (uncommitted). The
  shipped prod env sets `BVIMPOSTERS_BAKE_ENABLED=0` — the prod unit is serve-only;
  baking is enabled deliberately on the dev twin or a manual run, never by default.
- Unit SOURCE template
  `/home/dcl/one/umbrella/systemd/umbrella-catalyrst-bvimposters.service` with the
  standard `@UMBRELLA_DIR@/@BINDIR@` placeholders (Type=exec, EnvironmentFile,
  `ExecStart=@BINDIR@/catalyrst-bvimposters`, Restart=always, RestartSec=30,
  WantedBy=umbrella.target). `scripts/render-units.sh` renders it; `data/systemd/` is
  output, never edited. Unit enable/start is human-gated: `systemctl --user` fails in
  the sandbox — run unit management unsandboxed or drop a request under
  `~/lore/data/restart-requests/`. Never restart prod units, never touch abgen :5147
  or the umbrella nginx.
- Dev twin: free once the unit template + env file + workspace debug binary exist —
  `up-dev.sh` enumerates `umbrella-catalyrst-*.service` files and runs
  `target/debug/catalyrst-bvimposters` with the unit's EnvironmentFiles on the SAME
  port. No extra wiring.

Seed import is a binary mode, not a route:
`catalyrst-bvimposters seed <realm-cache-dir>` (argument handled before the server
path; exits when done, prints counts imported/skipped/crc0/incomplete).

## 8. Client hook contract (bevy-explorer, crates/imposters + minimal config surface)

Scope: `crates/imposters` plus two `AppConfig` fields in
`crates/common/src/structs/config.rs`. Nothing else — ipfs/scene_runner/avatar/visuals
/deploy are owned by concurrent lanes. No wasm rebuild or publish in this workflow;
the gate is commit + native `cargo check -p imposters -p common` green.

1. `AppConfig` gains two `#[serde(default)]` fields (upstream behavior preserved when
   absent from config json):
   - `imposter_url_base: Option<String>` — default `None` ⇒ the current literal
     `https://bevy-imposters.dclregenesislabs.xyz`.
   - `imposter_realm_key: Option<String>` — default `None` ⇒ `CurrentRealm.about_url`
     (current behavior). When `Some(k)`, `k` replaces the realm id EVERYWHERE the
     crate uses it: remote zip URL AND the local cache directory (`file_root`), so the
     browser/native cache also becomes realm-independent and survives realm re-keying.
2. Plumbing route: the plugin reads `Res<AppConfig>` where callers already read
   `Res<CurrentRealm>` (render/manager.rs, render/mod.rs, bake_scene/mips.rs,
   bake_scene/oven.rs) and computes
   `id = config.imposter_realm_key.as_deref().unwrap_or(&current_realm.about_url)`
   at the existing `about_url` call sites. `load_imposter_remote`
   (imposter_spec.rs:169) takes a `base_url: &str` parameter replacing the hardcoded
   host at line 183; the `.replace("%", "%25")` stays exactly as-is (with realm key
   `content` it is a no-op; with default `None` behavior it reproduces upstream
   byte-for-byte).
3. Our deploy config values (set by the deployment's config json, NOT by changed
   defaults): `imposter_realm_key = "content"`, `imposter_url_base` = the front-proxy
   base chosen by the serving lane (local native testing: `http://127.0.0.1:5154`).
   Resulting request: `{base}/imposters/realms/content/{level}/{x},{y}.{crc}.zip` —
   exactly the service route in section 3.
4. crc derivation is untouched: the client keeps computing crcs from OUR catalyst's
   active entities, the service bakes from the same source, so keys agree without any
   realm coupling.
5. Style: match surrounding code, comments only load-bearing why, no upstream-visible
   default changes. Commit with pathspecs only.

## 9. Seed import mapping

Source corpus (verified on disk, ~1.3 GB, 19,311 files, mid-June snapshot):
`/home/dcl/.local/share/bevyexplorer/cache/imposters/realms/https%3A%2F%2Frealm-provider-ea.decentraland.org%2Fmain%2Fabout/{level}/`
holding EXTRACTED triples `{x},{y}-spec.json` + `{x},{y}.boimp` + `{x},{y}-floor.boimp`
(no zips, no crc in filenames).

Rekey rule, per level 0..=5, per spec file:

1. Parse `{x},{y}` from the spec filename; read the json; take its `crc` field.
2. Skip if `crc == 0`, if either sibling (`.boimp`, `-floor.boimp`) is missing, or if
   the store target already exists (idempotent re-runs).
3. Write `store/{level}/{x},{y}.{crc}.zip` (temp + rename) containing the three files
   under their EXACT source basenames, `CompressionMethod::Stored`.

The corpus is read-only input; never modify or move it. Expect a fraction to be stale
against today's crcs — stale entries are harmless dead weight that LRU ages out, and
current-crc requests for those tiles follow the read-through/bake path. Known-good
probe pair for tests: level 0 tile `0,100`, crc `3504527830`, boimp member 260534 B.

## 10. Test matrix

Unit tests (in-crate, no network):

- Key parsing: negatives (`-64,-128.123.zip`), crc bounds, reject crc 0, reject
  unaligned `(x,y)` per level, reject level 6.
- Upstream URL construction: for key (0, 0, 100, 3504527830) the exact string
  `https://bevy-imposters.dclregenesislabs.xyz/imposters/realms/https%253A%252F%252Frealm-provider-ea.decentraland.org%252Fmain%252Fabout/0/0,100.3504527830.zip`.
- LRU: tiny budget, eviction strictly by mtime, touch-on-serve reorders, <1h entries
  protected, eviction is rename-then-unlink.
- Seed rekey: synthetic corpus dir → correct store path, Stored method, member names,
  crc0/incomplete skips, idempotency.
- Quarantine state: failure counting, TTL expiry, boot persistence round-trip.
- Read-through quarantine: list parsing (suffix-optional, comments, invalid lines),
  missing-file-empty, `quarantine` subcommand rename counts, and the handler gate
  (listed store hit serves; listed miss 404s with ZERO upstream fetches against a
  counting mock CDN; unlisted miss still reads through).

Curl matrix (dev twin on :5154; run in order):

1. `curl -s localhost:5154/ping` → 200.
2. Seed then hit: after `seed`, GET
   `/imposters/realms/content/0/0,100.3504527830.zip` → 200, `x-bvi-source: store`,
   `unzip -l` shows the 3 members with `0,100.boimp` = 260534 bytes.
3. Read-through: pick a key present upstream, absent locally (delete it from the
   store): first GET → 200 `x-bvi-source: cdn`, file lands in `store/`, second GET →
   200 `x-bvi-source: store`, byte-identical body.
4. Realm-independence: same key via segment
   `https%253A%252F%252Frealm-provider-ea.decentraland.org%252Fmain%252Fabout` and via
   `content` → identical bytes, same store entry, store entry count unchanged.
5. Bake-on-miss (bake enabled, wrapper configured, dcl_deno_ipc built): GET a
   plausible key with a wrong crc → 404 and `/status` shows the job; after the worker
   finishes, GET the tile with the crc found in the harvested filename → 200.
6. Poison: set `BVIMPOSTERS_IMPOST_BIN=/bin/false`, request a CDN-missing key 3 times
   (waiting out each failed job) → key appears in `/status` quarantine; further GETs
   404 with no new job.
7. `crc=0`, unaligned, level 6 → 404, no store/CDN/bake side effects (verify via
   `/status` counters and logs).
8. LRU live: set `BVIMPOSTERS_STORE_MAX_BYTES` to a few MB on a scratch store, insert
   via read-through until eviction, confirm oldest-first and `/status` bytes ≤ budget.

Campaign acceptance (bounded warm, service-level E2E, no client rebuild):

- With bake enabled, run one warm campaign covering the ±32-parcel square around 0,0
  (either one manual impost run `--location 0,0 --range 32 --levels 5
  --zip-output` into staging + harvest, or request-driven), wall-clock bounded at 60
  minutes.
- PASS: ≥95% of non-empty tiles (levels 0..5 overlapping the square, expected crc ≠ 0
  computed from the content DB per the pointers.rs CKSUM cascade — CRC_32_CKSUM, not
  zlib ISO-HDLC) return 200 from the service with matching filename crc; `/status`
  store bytes under budget; zero quarantined keys from the campaign.

## 11. Known risks (carried from recon, binding on implementers)

- CRC parity with the community CDN holds only while our catalyst's active-entities
  view matches the official realm's; divergence silently degrades read-through hits
  into bakes. Acceptable by design (bake path is the healer), but log CDN-miss rate.
- Never percent-encode or decode the CDN realm segment or the incoming realm path
  segment; the double-encoding contract breaks on any normalization.
- impost's bare-adapter (no compositor) mode is VERIFIED working via
  `umbrella/scripts/bvimposters-bake-env.sh`; the rig sway+wayvnc stack stays the
  fallback. The pinned vulkan-loader store path in that wrapper is GC-vulnerable —
  if bakes start dying at "Unable to find a GPU", re-point `BVI_VULKAN_LOADER_LIB`.
- The shared bevy-explorer `target/` dir may be cargo-locked by other lanes; bake
  prerequisites (`dcl_deno_ipc`) build in the background and waiting is expected.
- Stale-impost trap: a `target/debug/impost` predating the fb31ef18a
  post-process fix dies with the Validation RenderError, OR exits 0 in seconds with
  "all done!" and zero zips. Symptom in the service: instant `bake produced no
  harvestable zips` / exit 139/134. Check the binary mtime against fb31ef18a and
  rebuild in the bevy devShell before debugging anything else.
- Direct level ≥ 1 bakes regressed: impost picks the L1 imposter and
  reports "all done!" ~2 s later without baking (mips ingredients absent under the
  wrapper's fresh per-job XDG_DATA_HOME; the previously verified behavior was a slow bake, not an
  instant no-op). Level-0 bakes work and one L0 run harvests the whole loaded scene.
  Workaround until fixed: bake L0 and accept the level-pyramid gap, or pre-seed the
  job XDG cache with the L0 triples before an L1 run.
- Re-bake campaign finding: exact-#FF00FF is NOT a reliable placeholder
  signature — placeholder-free local re-bakes of the 88-91,34-37 (crc 3682496553) and
  1..4,95..100 (crc 1460665232) scenes reproduce screen-shaped exact-fuchsia blobs,
  i.e. those are scene-authored magenta. Only the floor-member blobs vanished on
  re-bake (genuinely placeholder-borne). Quarantined keys re-baked locally serve again
  by design; treat the remaining quarantine list as containment pending upstream's own
  re-bake, not as proof of visual defect per tile.
