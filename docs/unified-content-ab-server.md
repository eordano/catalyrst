# Unified content / asset-bundle server

`catalyrst-ab-cdn` is one server behind a single resolver doing three jobs:

1. **Corpus hit** — serve a pre-built bundle/manifest from the on-disk corpus (`ABGEN_OUT_ROOT`).
2. **JIT miss** — convert the entity on demand via the embedded `abgen` converter, **write it into the corpus** in the offline layout, then re-serve it.
3. **Native passthrough** — non-AB content (`scene.json`, `main.crdt`, `bin/*`, raw hashes) streamed from the content store (`CatalystClient`: disk arm `ABGEN_CONTENT_DISK` or remote `ABGEN_CATALYST_URL`).

## Transparency invariant

A client must not be able to tell whether an asset was batch-converted or live-converted: same URL surface, same response bytes, same headers (`application/wasm`, immutable `cache-control`, `ETag`, range, CORS). This holds **by construction** — a JIT miss writes the bundle into the corpus and the request is re-served through the normal corpus path, so the response is produced by the same code as a batch-built hit.

The manifest is emitted by a single function (`abgen::manifest::write_corpus_manifest`) shared by the offline pipeline and the JIT path; the bundle bytes come from the same `build_bundle` with the same opts. Determinism is the spine of the invariant — the offline corpus and the JIT path must run with matching build settings (`ABGEN_VERSION`, `ABGEN_REAL_TEXTURES`, `ABGEN_V38_COMPAT`, `ABGEN_MANIFEST_CONTENT_SERVER_URL`).

## Configuration

| Env | Purpose |
|---|---|
| `ABGEN_OUT_ROOT` | corpus root (served + written on JIT miss) |
| `ABGEN_CATALYST_URL` / `ABGEN_CONTENT_DISK` | content source (remote / local disk) |
| `ABGEN_ROOT` | dir holding `template/` — the build template the converter mmaps; **required** for JIT (a corpus miss 500s without it) |
| `ABGEN_VERSION`, `ABGEN_MANIFEST_CONTENT_SERVER_URL` | manifest version + `contentServerUrl`, matched to the corpus build |
| `ABGEN_CACHE_DIR` | working cache for the converter |

`/health` reports `mode: in-process`, `template_ok`, and degrades if the converter is active but the template is missing.

## Components

- `resolver` — platform/casing/no-deps resolution (subsumes per-platform aliases and the no-deps hardlink forms via an in-memory bundle index).
- `serve` — corpus static serve (range, ETag, brotli, immutable cache, CORS).
- `abgen::live::Proxy` — in-process converter: resolves an entity (disk-or-remote), builds its bundles, and emits them into the corpus.
- The offline pipeline (`abgen` / `abgen-corpus` / `abgen-verify`) remains the bulk corpus generator, writing the same layout the server writes on a JIT miss.

Known limitation: a corpus miss on the legacy flat `<v>/<hash>_<platform>` URL 404s (the entity isn't derivable from the path); clients use the manifest + `<v>/<entity>/<file>` form, which is fully covered.
