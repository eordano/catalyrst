# draco_decoder

`draco_decoder` is a Rust library for decoding Draco compressed meshes. It provides native and WebAssembly (WASM) support with efficient bindings to the official Draco C++ library.

## Overview

- **Native:**  
  Uses [`cxx`](https://cxx.rs/) to create safe and ergonomic FFI bindings directly to Draco's C++ decoding library — efficient, zero-copy mesh decoding.

- **WASM:**  
  This fork's vendored copy replaces the upstream crate's Emscripten/JS-Worker WASM path with a plain-C bridge (see the Vendoring note below); the WASM build goes through the same `decode_mesh_with_config_sync` entry point as native.

## Build Guide

- Install essential tools for C++ development (cmake, C++ compiler, etc.)
- `cargo build`

This crate has passed builds on the latest platforms. On Windows, only MSVC is supported.

## Usage

### Sync API (Native only)

```rust
use draco_decoder::decode_mesh_with_config_sync;

// Your Draco-encoded binary mesh data
let data: &[u8] = /* your Draco encoded data here */;

// Decode the mesh data synchronously
if let Some(result) = decode_mesh_with_config_sync(data) {
    let decoded_data = result.data;
    let config = result.config;
}
```

### DracoDecodeConfig

The `DracoDecodeConfig` provides metadata about the decoded mesh:

```rust
// Access mesh information
let vertex_count = config.vertex_count();
let index_count = config.index_count();
let buffer_size = config.buffer_size();
let index_length = config.index_length();

// Access attributes
for attr in config.attributes() {
    println!("Attribute - dim: {}, offset: {}, length: {}", 
        attr.dim(), attr.offset(), attr.lenght());
}
```

## How It Works

The decoder uses a caching mechanism within the FFI that splits the decoding process into:

1. **Decode** - Parse the Draco data
2. **Generate Config** - Extract mesh metadata (vertex count, attributes, buffer size)
3. **Allocate & Copy** - Allocate exact memory and copy decoded data

This approach achieves zero-copy data transfer since Rust can allocate the exact required memory based on the decoded metadata.

## Performance

| Environment            | Typical Decoding Time |
| ---------------------- | --------------------- |
| Native (Release Build) | 3 ms – 7 ms           |
| WebAssembly (WASM)     | 30 ms – 50 ms         |

## Warnings

- This crate is work in progress and has not been extensively tested across all platforms.

## Vendoring note

Origin: the `draco_decoder` crate from crates.io, reduced to the decoder-only
subset (the mesh decode path plus the bundled upstream Draco sources under
`third_party/draco/`). Local modification: a plain-C decoder bridge
(`cpp/decoder_api_c.cc` with the `src/ffi_c.rs` FFI) added to replace the `cxx`
bridge on targets where `cxx` is unavailable, producing the same decoded-mesh
layout. Licenses retained: upstream Google Draco is Apache-2.0
(`third_party/draco/LICENSE`), and the crate's own dual license texts
(`LICENSE-APACHE`, `LICENSE-MIT`) are kept in this directory.
