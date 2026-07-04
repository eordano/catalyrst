# Asset bundles — unified content/AB server + validation gates

> Status: distilled 2026-07-04; server architecture re-verified 2026-07-03.
> The full compat campaign ledger (fail classes, tolerance proofs, per-vintage
> baselines) lived in `docs/abgen-compat-bag-2026-07-03.md` and the abgen
> campaign docs — recoverable at git ref `ff400cab^` if needed.

## One server, three jobs

`catalyrst-abgen` stands in for `ab-cdn.decentraland.org` behind a single
resolver:

1. **Corpus hit** — serve a pre-built bundle/manifest from the on-disk corpus
   (`ABGEN_OUT_ROOT`).
2. **JIT miss** — convert the entity on demand via the embedded `abgen`
   converter, **write it into the corpus in the offline layout**, then re-serve
   it.
3. **Native passthrough** — non-AB content (`scene.json`, `main.crdt`,
   `bin/*`, raw hashes) streamed from the content store (disk arm
   `ABGEN_CONTENT_DISK` or remote `ABGEN_CATALYST_URL`).

## The transparency invariant

A client must not be able to tell whether an asset was batch-converted or
live-converted: same URL surface, same bytes, same headers
(`application/wasm`, immutable `cache-control`, `ETag`, range, CORS). This
holds **by construction**: a JIT miss writes into the corpus and the request is
re-served through the normal corpus path — the same code that serves
batch-built hits. The manifest comes from one shared function
(`abgen::manifest::write_corpus_manifest`), the bundle bytes from the same
`build_bundle` with the same opts.

The corollary is the sharpest operational rule in this crate: **the offline
corpus and the JIT path must run with matching build settings**
(`ABGEN_VERSION`, `ABGEN_REAL_TEXTURES`, `ABGEN_V38_COMPAT`,
`ABGEN_MANIFEST_CONTENT_SERVER_URL`). A corpus built with different flags than
the live server silently ships two different artifact families under one URL
space.

## Configuration

| Env | Purpose |
|---|---|
| `ABGEN_OUT_ROOT` | corpus root (served + written on JIT miss) |
| `ABGEN_CATALYST_URL` / `ABGEN_CONTENT_DISK` | content source (remote / local disk) |
| `ABGEN_ROOT` | dir holding `template/` — the build template the converter mmaps; **required for JIT** (a corpus miss 500s without it) |
| `ABGEN_VERSION`, `ABGEN_MANIFEST_CONTENT_SERVER_URL` | manifest version + `contentServerUrl`, matched to the corpus build |
| `ABGEN_CACHE_DIR` | converter working cache |

`/health` reports `mode: in-process` and `template_ok`, degrading when the
converter is active but the template is missing.

Known limitation: a corpus miss on the legacy flat `<v>/<hash>_<platform>` URL
404s (the entity isn't derivable from the path); clients use the manifest +
`<v>/<entity>/<file>` form, which is fully covered.

## Why a standing validation loop exists

The hard lesson (2026-07-03): before the loop, no corpus was ever built with
the flags the live server actually serves (`ABGEN_REAL_TEXTURES` +
`ABGEN_V38_COMPAT`), so every live-only code path — real BC7/DXT5 encodes, v38
mesh clustering, shader-reference serialization — shipped unvalidated. Two
whole defect families (JIT empty-render; InternalErrorShader) hid exactly
there.

Three gates, cheapest first — run all three after any change to
`crates/catalyrst-abgen` touching builder/live/texture/animation code:

1. **Fork-parity (byte identity).** `abgen-corpus --from-reference` against
   the reference fork corpus, then `abgen-verify`. PASS = the byte-identical
   ratio does not drop below the proven baseline. The non-identical remainder
   is known-benign payload-encoding churn — the *ratio* is the signal.
2. **Live-mode structural.** `abgen-corpus --live-mode` builds a per-vintage
   sample with the exact flag set `live.rs Proxy::new` applies, then
   `abgen-verify --tolerant` compares structurally against mirrored production
   bundles. Byte identity vs upstream is impossible by design
   (non-deterministic CAB names), so this gate is structural-only. Exit code 3
   = a structural pair exists. Tolerance classes (benign delta families) are
   encoded in `analyze()` in `src/bin/abgen-verify.rs` with unit tests.
3. **Render gate.** Per verification entity, render **three byte modes** —
   fork corpus bytes, the bytes the JIT server actually serves, and the
   upstream mirror — on a real GPU and compare pixels/inventories. The
   three-mode delta is what isolates serving-path defects: fork bytes can be
   pixel-identical to upstream while live bytes of the same entity render
   empty.

Rule when a gate fires: classify as regression (fix before merge) or
newly-discovered upstream/vintage delta (triage → fix, or promote to a
tolerance class **only with a render proof attached**). Never widen a
tolerance to green a gate without a render proof — that is precisely how the
two defect families stayed hidden.

## Registry sibling

`catalyrst-registry` (library crate; consumed by `catalyrst-create` and
`catalyrst-abgen` — the standalone `catalyrst-ab-registry` binary is retired)
serves the asset-bundle-registry surface: `/entities/active`,
`/entities/versions` (nested `versions.assets.{windows,mac,webgl,linux}` each
`{version, buildDate}` — the shape Unity requires), `/profiles`, and admin
handlers.
