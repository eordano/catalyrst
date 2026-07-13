# crunch_ffi — Unity-Technologies/crunch (CRN) Rust FFI

Vendored copy of [Unity-Technologies/crunch](https://github.com/Unity-Technologies/crunch)
branch `unity` @ 8708900 (zlib-style license — see the notice at the end
of `inc/crnlib.h`; upstream `license.txt` retained). Provides a thin Rust
wrapper over `crn_compress` for compressing RGBA mip chains into the
.CRN container format. Used by abgen for the DXT5Crunched texture
path (`src/bc5_pure.rs::encode_dxt5_crn_mip_chain`).

**Why the Unity fork, not classic BinomialLLC v1.04:** Unity >= 2017.3
only decompresses CRN payloads produced by its own crunch fork — the
container header layout is identical, but the fork rewrote the
tables/bitstream encoding (block-linear instead of 2x2 chunk-based).
Classic-crnlib payloads make Unity's `DecompressCrunch` fail with
"encoded with an older version of Crunch (prior to 2017.3)" and then
crash the Metal texture upload. The wrapper also stamps
`m_userdata0 = 1` to match the container fields Unity's TextureImporter
writes (verified against upstream ab-cdn bundles).

Local patches on top of upstream (re-apply if re-vendoring):
* macOS/arm64 portability (`__APPLE__` branches in `crn_mem.cpp`,
  `crn_jpge.cpp`, `crn_threading_pthreads.{cpp,h}`, `crn_platform.h`,
  `inc/crn_decomp.h`) — GCD semaphores, `malloc_size`, no `fopen64`,
  `__builtin_trap`.
* Lazy global init: the namespace-scope `g_crnlib_initializer` static in
  `crnlib/crnlib.cpp` is replaced by `extern "C" crn_ffi_ensure_global_init()`
  called from the FFI wrapper (saves ~67M startup instructions in every
  process that links but never uses crnlib).
* `crn_zeng.cpp/h` dropped (removed upstream in the unity branch).

## Layout

```
third_party/crunch/
├── crnlib/         Upstream C++ encoder sources (verbatim copy)
├── inc/            Upstream public headers (crnlib.h, crn_decomp.h, dds_defs.h)
├── cpp/
│   ├── crn_wrapper.h    C ABI declarations
│   └── crn_wrapper.cc   C ABI implementation calling crn_compress
├── src/lib.rs      Rust shim (`crn_compress_bc5` + smoke test)
├── build.rs        Compiles crnlib + LZMA + wrapper into one static lib
├── Cargo.toml
└── license.txt     Upstream public-domain license
```

## API

```rust
use crunch_ffi::{crn_compress, crn_compress_bc5, CrnFormat};

// BC5 / 3DC / DXN_XY — what abgen uses for normal maps.
let crn_bytes: Vec<u8> = crn_compress_bc5(
    &mip_chain_rgba,    // mip 0 + mip 1 + ... concatenated, R=X G=Y B=* A=*
    &[1024, 512, 256],  // per-mip widths
    &[1024, 512, 256],  // per-mip heights
    255,                // quality level (0..=255; 255 = max quality)
).expect("crnlib refused input");

// First two bytes are the CRN magic 0x48 0x78 ('H' 'x').
assert_eq!(&crn_bytes[..2], b"Hx");
```

## Why not pre-encoded BC5 blocks?

crnlib's public API does its own DXTn block compression internally
(`cCRNDXTCompressorCRN`) before packing into the .CRN container — it
doesn't expose a "wrap existing blocks" entry point. So our pipeline
hands it the RGBA source mips and lets crnlib own both the BC5
encoding and the RDO/CRN container step.

This matches how prod's Unity TextureImporter uses crnlib internally
("Compressed (High Quality)" import setting → CRN), so the output
fits exactly inside prod's `m_StreamData.size` envelope (within ~15-30%
on the corpus).

## Building

Plain `cargo build` works on Linux/macOS via `cc-build`. No CMake dependency;
pthreads linked for `crn_threading_pthreads.cpp`. Tested on NixOS via an FHS
shell. Single-translation-unit-per-cpp (no precompiled headers), ~30s cold
from `target/`.
