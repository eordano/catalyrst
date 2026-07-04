# draco_decoder

`draco_decoder` is a Rust library for decoding Draco compressed meshes. It provides native and WebAssembly (WASM) support with efficient bindings to the official Draco C++ library.

## Overview

- **Native:**  
  The native part uses [`cxx`](https://cxx.rs/) to create safe and ergonomic FFI bindings that directly connect to Draco's C++ decoding library. This allows efficient and zero-copy mesh decoding in native environments.

- **WASM:**  
  For WebAssembly targets, `draco_decoder` leverages the official Draco Emscripten build. It uses a JavaScript Worker to run the Draco decoder asynchronously, enabling non-blocking mesh decoding in the browser. The JavaScript implementation is available in a separate repository:  
  [https://github.com/jiangheng90/draco_decoder_js.git](https://github.com/jiangheng90/draco_decoder_js.git)

This design provides a unified Rust API while seamlessly switching between native and WASM implementations under the hood.

## Build Guide

- Install essential tools for C++ development (cmake, C++ compiler, etc.)
- `cargo build`

This crate has passed builds on the latest platforms. On Windows, only MSVC is supported.

## Usage

### Async API

```rust
use draco_decoder::decode_mesh_with_config;

// Your Draco-encoded binary mesh data
let data: &[u8] = /* your Draco encoded data here */;

// Decode the mesh data asynchronously
if let Some(result) = decode_mesh_with_config(data).await {
    let decoded_data = result.data;      // Vec<u8> - decoded mesh buffer
    let config = result.config;          // DracoDecodeConfig - mesh metadata
    
    println!("Vertex count: {}", config.vertex_count());
    println!("Index count: {}", config.index_count());
    println!("Buffer size: {}", config.buffer_size());
}
```

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
- On WASM, data transfer between Rust and JS Worker incurs copy overhead. Using SharedArrayBuffer would avoid this but requires cross-origin isolation in browsers.
