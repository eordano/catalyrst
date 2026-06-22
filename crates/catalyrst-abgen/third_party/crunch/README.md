# crunch_ffi — BinomialLLC/crunch (CRN) Rust FFI

Vendored copy of [BinomialLLC/crunch](https://github.com/BinomialLLC/crunch)
v1.04 (public domain — see `license.txt`). Provides a thin Rust
wrapper over `crn_compress` for compressing RGBA mip chains into the
.CRN container format. Used by abgen-rs for the BC5 normal-map
encoder path (`src/bc5_pure.rs::encode_bc5_normal_crn_mip_chain`).

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
├── license.txt     Upstream public-domain license
└── README-upstream.md  Upstream README (kept for reference)
```

## API

```rust
use crunch_ffi::{crn_compress, crn_compress_bc5, CrnFormat};

// BC5 / 3DC / DXN_XY — what abgen-rs uses for normal maps.
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
does not expose a "wrap existing blocks" entry point. So our pipeline
hands it the RGBA source mips and lets crnlib own both the BC5
encoding and the RDO/CRN container step.

This matches how prod's Unity TextureImporter uses crnlib internally
("Compressed (High Quality)" import setting → CRN), so the output
fits exactly inside prod's `m_StreamData.size` envelope (within ~15-30%
on the corpus).

## Building

Plain `cargo build` works on Linux/macOS via `cc-build`. No CMake
dependency. pthreads are linked for `crn_threading_pthreads.cpp`.
Tested on NixOS via an FHS shell. The build is single-translation-unit-
per-cpp (no precompiled headers) and currently takes ~30s cold from
`target/`.
