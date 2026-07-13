# abgen wasm lab — proof of concept

The abgen converter core (the parent crate, default features off) compiled to
`wasm32-unknown-unknown` and driven from a static web page: drop a
glb/gltf/zip of a wearable, emote or scene, watch the files convert in
parallel on a worker pool (one file per worker; a trap fails only its own
file and the worker is respawned), download real UnityFS bundles + the
manifest. Nothing leaves the browser.

## Run it

```bash
bash wasm-poc/build.sh          # build site/wasm/abgen_poc.wasm (gitignored artifact)
bash wasm-poc/serve.sh          # http://127.0.0.1:5189/wasm/
```

`site/wasm/abgen_poc.wasm` (~4.3 MB) is never committed — `build.sh`
regenerates it from the tree. The module is a plain cargo package at
`wasm-poc/` with its own committed `Cargo.lock`, excluded from the parent
workspace so the default workspace build/check/test never needs a wasm
toolchain. The toolchain is pinned in `wasm-poc/toolchain/flake.nix`
(rust 1.97.0 + wasm32 target); no wasm-bindgen — the module speaks a
hand-rolled C ABI (`poc_init` / `poc_alloc` / `poc_convert`, events stream
back through the imported `env.host_emit`).

## What is real

- Full geometry lane: GLB parse, accessors, RH→LH handedness, winding,
  normals/tangents, skinning, legacy Animation and Mecanim emote clips.
- `KHR_draco_mesh_compression`: the vendored Google draco decoder compiled
  to wasm (decoder-only subset, plain-C bridge in
  `third_party/draco_decoder/cpp/decoder_api_c.cc` replacing the cxx bridge).
- Full texture lane **including the C codecs**: linear-space mips, alpha
  bleed, BC7/DXT1 block compression (pure Rust; the BC7 partition estimator
  runs 4-wide SIMD128 after a startup probe proves it byte-equal to the
  scalar path, which stays the byte truth), IJG libjpeg 9c
  decode (box + fancy), crnlib crunched-DXT5 normal maps — all three C/C++
  libraries are cross-compiled by the nix toolchain (`pkgsCross.wasi32`:
  clang + wasi-libc sysroot + static libc++) and statically linked in.
- Real SerializedFile + UnityFS writer, LZ4HC blocks, deterministic CAB
  names/path ids, embedded type-tree templates (the four `template/*.bundle`
  files are `include_bytes!`ed).
- `validate.rs` (E0–E9) runs on every produced bundle before download.
- The LOD lane: `lodgen` model merge, InitialSceneState placements (an
  uploaded `*_InitialSceneState.json` descriptor drives the same assemble
  instancing as native), parcel crop, square-POT texture atlas, meshopt
  decimation (the same vendored meshoptimizer 0.25 sources the native
  meshopt lane compiles, built here by the wasi toolchain), GLB emit, the
  `LodBuildParams` bundle path (`DCL/Scene_TexArray` binding, parcel
  clipping/root placement from scene.json), the LOD self-gate. Scene
  uploads bake a `LOD/1/{id}_1_{platform}` bundle (plus `.br` and
  `LOD.manifest.json` sidecars) next to the scene bundles. Placements
  acquisition stays native-only (running the scene against the Node
  manifest-builder's game.js does not happen in the browser), as does the
  gltfpack decimation backend (external binary).

## The decoder rule (native default vs the gate)

Native GLB JPEG decode defaults to turbojpeg: measured against the Unity
upstream oracle it is the closer decoder (worst mean channel delta 0.03 vs
2.99 for IJG 9c), so the wasm port does **not** flip the native default.
wasm has no dlopen and always decodes through the vendored libjpeg9c. The
parity gate closes that gap by exporting `ABGEN_JPEG_GLB_9C=1` — the
existing native escape in `src/gltf/scene_build.rs` — on every native
invocation, so both sides decode identically inside the gate while the
production profile (env unset) stays byte-identical to the pre-wasm-port
tree. Transcendental call sites in the byte path (`acos`, `powf`,
`sin/cos`, `log2`) go through `src/detmath.rs` (pure-Rust libm) on both
targets unconditionally — proven byte-neutral vs glibc libm on 60/60 real
bundles.

## Remaining gaps vs the native fleet

- Encode is CPU: SIMD128 in the BC7 partition estimator (runtime
  self-qualified against scalar, worth ~2% wall on a BC7-heavy scene —
  the estimator is a small slice of the wasm encode) on a pool of workers;
  native uses AVX-512 or the bit-exact GPU lane. The `+simd128` build
  flag raises the module floor to engines with wasm SIMD (Chrome 91+,
  Firefox 89+, Safari 16.4+, Node 16.4+ — older engines reject the module
  at validation).
- WebGPU encode is a documented follow-up, deliberately descoped: the
  module is a bindgen-free cdylib with a synchronous `poc_convert`, while
  browser WebGPU buffer readback is async-only, and wgpu's browser backend
  requires wasm-bindgen glue. Doing it honestly means a resumable convert —
  a dedicated texture-encode job phase that awaits GPU readbacks between
  module calls (a natural fit over the worker-pool job protocol), reusing
  the native GPU lane's bit-exact WGSL kernels and its per-device
  self-qualification contract. Until then the native CUDA/wgpu lane is the
  GPU story.

Formerly a gap, now real: libjpeg error recovery uses actual setjmp/longjmp
via wasi-libc's `libsetjmp.a` (the LLVM Wasm-SjLj transform over wasm
exception handling, legacy `try`/`catch` encoding — Chrome 95+, Firefox
100+, Safari 15.2+, Node 17+). A malformed JPEG now fails per-image exactly
like native instead of trapping the module; the EH compile flags are scoped
to the libjpeg9c build (`third_party/libjpeg9c/build.rs`), and the final
link adds `libsetjmp.a` in `wasm-poc/build.rs`. crnlib's bundled `jpgd`
file loader is never called — unchanged.

## wasm C/C++ port notes

- cc-rs compiles the vendored C/C++ with the wasi clang wrapper; the
  devShell pins `CC/CXX/AR_wasm32_unknown_unknown` and appends
  `--target=wasm32-unknown-wasi` via `CFLAGS`/`CXXFLAGS` (env flags land
  last, overriding cc-rs's `--target=wasm32-unknown-unknown`).
- `wasm-poc/build.rs` links wasi-libc's `libc.a` plus `libc++.a`/`libc++abi.a`
  into the final cdylib; the resulting `wasi_snapshot_preview1` imports
  (stdio/env/prestat) are stubbed in `site/wasm/worker.js` — `fd_prestat_get`
  must return EBADF(8) so libc's preopen scan terminates.
- `poc_init` calls `__wasm_call_ctors` explicitly (reactor model); without
  that reference wasm-ld wraps every export in command-model ctor/dtor calls
  and the first export invocation traps in `__funcs_on_exit`.
- Headless verification: `wasm-poc/test/make-fixtures.py` + `node
  wasm-poc/test/headless.mjs <out> windows '' <fixture.glb>` (single
  instance) or `headless-pool.mjs` (the same page pool over a
  worker_threads shim, `--workers=N`); full cross-target gate:
  `bash wasm-poc/test/parity.sh`.

## Cross-target byte gate

`bash wasm-poc/test/parity.sh` rebuilds the wasm module and the native
`abgen`/`abgen-lod` binaries from the same tree, regenerates the fixtures,
converts every one on both sides (native under `ABGEN_JPEG_GLB_9C=1`, per
the decoder rule above), and sha256-compares each produced artifact across
the twelve fixtures: jpeg, crunched-normal, draco, PNG-gAMA,
`KHR_texture_transform` rotation, generated tangents, multi-material, a
two-parcel scene with its LOD1 bake, a dense grid decimated through the
meshopt tri cap, a parcel-overhanging slab through the crop planes, an
InitialSceneState placements assemble, and a good/corrupt JPEG pair.

Coverage boundaries, stated exactly:

- Wearable bundles are byte-compared on all three platforms.
- The scene fixtures byte-compare the scene bundle on all three platforms
  (`scene-lod`), and on windows/mac the baked `LOD/1` bundle plus its
  deterministic sidecars (`<bundle>.br`, `LOD.manifest.json`,
  `LOD.manifest.json.br`); the harness skips the LOD1 bake for webgl on
  both sides. Decimation is byte-gated by `dense-decimate-lod` (wasm
  simplify vs the native `abgen-lod atlas`/`simplify --simplifier meshopt`/
  `bundle` chain), crop by `crop-overhang-lod` (vs `atlas --crop-base
  --crop-parcels`), and ISS placements by `placements-iss-lod` (vs
  `assemble --entity-json --iss` over a prestaged content cache). The
  remaining native-only pieces — the gltfpack backend and the Node
  placements manifest-builder — are not byte-gated by anything.
- `badjpeg-pair` is a failure-parity check: the corrupt member's JPEG hard-
  errors in every decoder, the pipeline degrades that image to the
  deterministic missing-texture placeholder on both sides, and both bundles
  byte-compare; the wasm run must stay trap-free (before setjmp recovery
  the libjpeg longjmp aborted the whole conversion here).
- `manifest.json` is a wasm-only structural check (bundle list + `dcl`); the
  native CLI emits no manifest to compare against.
