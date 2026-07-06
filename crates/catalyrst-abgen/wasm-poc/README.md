# abgen wasm lab — proof of concept

The abgen converter core (the parent crate, default features off) compiled to
`wasm32-unknown-unknown` and driven from a static web page: drop a
glb/gltf/zip of a wearable, emote or scene, watch it convert in a worker,
download real UnityFS bundles + the manifest. Nothing leaves the browser.

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

- The full geometry lane: GLB parse, accessors, RH→LH handedness, winding,
  normals/tangents, skinning, legacy Animation and Mecanim emote clips.
- `KHR_draco_mesh_compression`: the vendored Google draco decoder compiled
  to wasm (decoder-only source subset, plain-C bridge in
  `third_party/draco_decoder/cpp/decoder_api_c.cc` replacing the cxx
  bridge).
- The full texture lane **including the C codecs**: linear-space mips, alpha
  bleed, BC7/DXT1 block compression (pure Rust, scalar), IJG libjpeg 9c
  decode (box + fancy), and crnlib crunched-DXT5 normal maps — all three
  C/C++ libraries are cross-compiled by the nix toolchain
  (`pkgsCross.wasi32`: clang + wasi-libc sysroot + static libc++) and
  statically linked into the module.
- The real SerializedFile + UnityFS writer, LZ4HC blocks, deterministic CAB
  names/path ids, embedded type-tree templates (the four `template/*.bundle`
  files are `include_bytes!`ed).
- `validate.rs` (E0–E9) runs on every produced bundle before download.
- The LOD lane's pure core: `lodgen` model merge, square-POT texture atlas,
  GLB emit, the `LodBuildParams` bundle path (`DCL/Scene_TexArray` binding,
  parcel clipping/root placement from scene.json) and the LOD self-gate.
  Scene uploads bake a `LOD/1/{id}_1_{platform}` bundle (plus the `.br` and
  `LOD.manifest.json` sidecars) next to the scene bundles. Decimation
  (gltfpack/meshopt), scene placements (the Node manifest-builder runs
  game.js) and parcel crop stay native-only.

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

- No setjmp on wasm: a malformed JPEG longjmp becomes a trap that aborts the
  whole conversion (reported through the panic hook) instead of recovering
  per-image. wasi-libc ships `libsetjmp.a` (wasm-EH based) if this ever needs
  to be real. Same for crnlib's bundled `jpgd` file loader (never called).
- Single-threaded scalar CPU; native uses AVX-512 or the bit-exact GPU lane.

## wasm C/C++ port notes

- cc-rs compiles the vendored C/C++ with the wasi clang wrapper; the
  devShell pins `CC/CXX/AR_wasm32_unknown_unknown` and appends
  `--target=wasm32-unknown-wasi` via `CFLAGS`/`CXXFLAGS` (env flags land
  last, overriding cc-rs's `--target=wasm32-unknown-unknown`).
- `wasm-poc/build.rs` links wasi-libc's `libc.a` plus `libc++.a`/`libc++abi.a`
  into the final cdylib; the resulting `wasi_snapshot_preview1` imports
  (stdio/env/prestat) are stubbed in `site/wasm/worker.js` — `fd_prestat_get`
  must return EBADF(8) so libc's preopen scan terminates.
- `poc_init` calls `__wasm_call_ctors` explicitly (reactor model). Without
  that reference wasm-ld wraps every export in command-model ctor/dtor calls
  and the first export invocation traps in `__funcs_on_exit`.
- Headless verification: `wasm-poc/test/make-fixtures.py` + `node
  wasm-poc/test/headless.mjs <out> windows '' <fixture.glb>`; the full
  cross-target gate is `bash wasm-poc/test/parity.sh`.

## Cross-target byte gate

`bash wasm-poc/test/parity.sh` rebuilds the wasm module and the native
`abgen`/`abgen-lod` binaries from the same tree, regenerates the fixtures,
converts every one on both sides for windows/mac/webgl (native under
`ABGEN_JPEG_GLB_9C=1`, per the decoder rule above), and sha256-compares
each produced artifact across the eight fixtures: jpeg, crunched-normal,
draco, PNG-gAMA, `KHR_texture_transform` rotation, generated tangents,
multi-material, and a two-parcel scene with its LOD1 bake.

Coverage boundaries, stated exactly:

- Wearable bundles are byte-compared on all three platforms.
- `scene-lod` byte-compares the scene bundle on all three platforms, and on
  windows/mac the baked `LOD/1` bundle plus its deterministic sidecars
  (`<bundle>.br`, `LOD.manifest.json`, `LOD.manifest.json.br`); the harness
  skips the LOD1 bake for webgl on both sides. The native-only LOD pieces —
  gltfpack/meshopt decimation, the Node placements manifest-builder, parcel
  crop — have no wasm counterpart and are therefore not byte-gated by
  anything.
- `manifest.json` is a wasm-only structural check (bundle list + `dcl`); the
  native CLI emits no manifest to compare against.
