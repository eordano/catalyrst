# Asset bundles - unified content/AB server

## One server, three jobs

`catalyrst-abgen` stands in for `ab-cdn.decentraland.org` behind one resolver:

1. Corpus hit - serve a pre-built bundle/manifest from `ABGEN_OUT_ROOT`.
2. JIT miss - convert the entity on demand via the embedded `abgen`
   converter, write it into the corpus in the offline layout, re-serve.
3. Native passthrough - non-AB content (`scene.json`, `main.crdt`, `bin/*`,
   raw hashes) streamed from the content store (disk arm
   `ABGEN_CONTENT_DISK` or remote `ABGEN_CATALYST_URL`).

## Transparency invariant

A client must not be able to tell batch-converted from live-converted: same
URL surface, bytes, headers (`application/wasm`, immutable `cache-control`,
`ETag`, range, CORS). Holds by construction: a JIT miss writes into the
corpus and is re-served through the normal corpus path (manifest via
`abgen::manifest::write_corpus_manifest`, bundle bytes via the same
`build_bundle` with the same opts). Corollary: the offline corpus and the
JIT path must run with matching build settings (`ABGEN_VERSION`,
`ABGEN_REAL_TEXTURES`, `ABGEN_V38_COMPAT`,
`ABGEN_MANIFEST_CONTENT_SERVER_URL`) - mismatched flags silently ship two
artifact families under one URL space.

## Configuration

| Env | Purpose |
|---|---|
| `ABGEN_OUT_ROOT` | corpus root (served + written on JIT miss) |
| `ABGEN_CATALYST_URL` / `ABGEN_CONTENT_DISK` | content source (remote / local disk) |
| `ABGEN_ROOT` | dir holding `template/` - the build template the converter mmaps; **required for JIT** (a corpus miss 500s without it) |
| `ABGEN_VERSION`, `ABGEN_MANIFEST_CONTENT_SERVER_URL` | manifest version + `contentServerUrl`, matched to the corpus build |
| `ABGEN_CACHE_DIR` | converter working cache |

`/health` reports `mode: in-process` and `template_ok`, degrading when the
converter is active but the template is missing. Known limitation: a corpus
miss on the legacy flat `<v>/<hash>_<platform>` URL 404s (the entity is not
derivable from the path); clients use the manifest + `<v>/<entity>/<file>`
form, which is fully covered.

## Registry sibling

`catalyrst-registry` (library crate; consumed by `catalyrst-create` and
`catalyrst-abgen` - the standalone `catalyrst-ab-registry` binary is retired)
serves the asset-bundle-registry surface: `/entities/active`,
`/entities/versions` (nested `versions.assets.{windows,mac,webgl,linux}`
each `{version, buildDate}` - the shape Unity requires), `/profiles`, and
admin handlers.
