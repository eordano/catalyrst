#!/usr/bin/env bash
# Rebuild the wasm module and refresh the site copy. wasm-poc is excluded
# from the parent workspace, so the build runs from this dir against the
# package's own committed Cargo.lock.
set -euo pipefail
cd "$(dirname "$0")"
nix develop "path:$PWD/toolchain" --command \
  cargo build --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/abgen_wasm_poc.wasm ../site/wasm/abgen_poc.wasm
ls -la ../site/wasm/abgen_poc.wasm
