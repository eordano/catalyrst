# abgen-gpu-kernel-ptx - build + verification

Compiles the shared BC7 encoder core (`src/core/`, also consumed by the host
via `#[path]` in `src/gpu/mod.rs`) to PTX with rustc's built-in
`nvptx64-nvidia-cuda` target. No CUDA toolkit, no rustc_codegen_nvvm. The host
dlopens `libcuda.so.1` at runtime, so default host builds have zero CUDA
dependencies.

## Build

```
cd crates/catalyrst-abgen/kernel-ptx
nix develop path:. --command cargo build --release
```

The flake pins nightly-2026-04-15 with rust-src + llvm-tools +
llvm-bitcode-linker. `.cargo/config.toml` sets the nvptx target,
`-Z build-std=core`, `-C target-cpu=sm_86`. Artifact:
`target/nvptx64-nvidia-cuda/release/abgen_gpu_kernel_ptx.ptx`. The frozen copy
embedded in the host lives at `../src/gpu/kernel.ptx` (`include_bytes!`);
`ABGEN_GPU_PTX` overrides it at runtime.

## Run

```
nix develop .#gpu -c \
cargo run --release -p catalyrst-abgen --features gpu --bin abgen-verify -- gpu diff --gpu
```

The `gpu` devShell pins the 64-bit vulkan-loader by derivation and sets the
driver/ICD paths; never assemble LD_LIBRARY_PATH from /nix/store globs (an
ELFCLASS32 loader match reports "no wgpu adapter" with no other symptom).
The default shell and bare shells lack the driver paths, and the failure
mode is deceptive - GPU configs report MISMATCH ("loading the CUDA driver
library failed" / "no wgpu adapter") while CPU configs still PASS, so check
the exit code, never grep for PASS.

## Verification record

- diff --gpu byte-identical vs oracle, all 4 BC7 configs
  (Slow/Basic x perceptual) at 512/seed3, 8192/seed1, 16384/seed42,
  32768/seed7 - exit 0 every run. Bench: 15.6-18.8 M blocks/s incl. transfers
  vs 62k blocks/s CPU single-thread.
- No FMA-contraction divergence at these settings; if a toolchain bump breaks
  byte-identity, try `-C llvm-args=--nvptx-fma-level=0` first.
- One thread per 4-block GROUP (GROUP_WIDTH), not per block - required for
  byte-identity because `compress_group` couples lanes.
- `Params`/`EndpointErr`/`OptTables` are `#[repr(C)]` (host->device raw-byte
  boundary); keep them repr(C).
- crate/module renamed from the retired abgpugen name; PTX
  regenerated (internal mangled symbols only; the eight `.visible .entry`
  names are unchanged) and re-verified with gpu diff at the four recorded
  seed/size combos; the harness then moved from a dedicated bin into
  `abgen-verify gpu <diff|bench|corpus>`.
