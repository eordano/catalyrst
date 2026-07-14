# Asset bundles - served by upstream abgen

The asset-bundle converter, ab-cdn JIT server, LOD generator, and AB-registry
all live upstream in **[decentraland/abgen](https://github.com/decentraland/abgen)**.
Catalyrst carries no asset-bundle code; it consumes the upstream flake:

- `flake.nix` input `abgen` (see the pin comment there for the current ref)
  re-exposes `packages.abgen` (the server binary) and `packages.abgen-compare`
  (the parity/inspection pipeline).
- One `abgen` binary serves everything on :5147: pre-built corpus bundles,
  JIT conversion on miss, LODs, ISS descriptors, `/entities/active|versions`,
  profiles, and - when `CONTENT_PG_CONNECTION_STRING` (URL form) is set - the
  signed registry surface (`/entities/status`, `/queues/*`, `/denylist*`,
  `/registry`, `/flush-cache`).
- Env contract: `ABGEN_ROOT`, `ABGEN_SHADER_BUNDLE`, `ABGEN_OUT_ROOT`,
  `ABGEN_CACHE_DIR`, `ABGEN_CATALYST_URL`, plus the LOD-JIT lane vars - see
  `umbrella/env/catalyrst-abgen.env` for the deployed set.

To change or gate asset-bundle behavior, work in the upstream repo (fork-PRs
via `eordano/abgen`; never push upstream directly) and bump the flake input
here.
