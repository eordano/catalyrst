# abgen-gpu-kernel-ptx - build, verification, interface contract

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

## Contract

`src/core/` is the `no_std` port of abgen's pure block encoders (bc7, mode_tree, bc5, dxt1,
mips): arithmetic-exact with the source in
`crates/catalyrst-abgen/src/{bc7_pure,bc7_mode_tree,bc5_pure,dxt1_pure}.rs` - same operations,
order, literals, integer widths, float types. The project exists to produce byte-identical
output to that oracle; a cleaner expression that changes one bit is a bug. Rules that still
govern any change here:

- `core/` compiles under `#![no_std]` except items gated `#[cfg(feature = "std")]`.
  `crate::sqrtf` instead of `f32::sqrt`; no env, threads, OnceLock, or rayon in core.
- Source statics built via `opt()`/`build_opt_tables()` are explicit `&OptTables` parameters
  threaded through every function that used them.
- Scalar branches only - no AVX2/AVX-512 paths, no rayon, no capture/env machinery.
- CRN (crunch C++ FFI) is excluded; it stays CPU-side in abgen.
- If bit-exactness is at risk (cross-block coupling, lazy LUTs, float intrinsics beyond sqrt),
  stop and report instead of approximating.
- No comments in `.rs` files (repo policy).

Kernel entry points (`src/lib.rs`, stable extern names the host resolves via
`cuModuleGetFunction`): `bc7_encode_groups`, `bc7_encode_groups_perm`, `bc7_encode_groups_desc`,
`bc7_group_sigs`, `bc7_group_sigs_desc`, `blockify_linearize`, `blockify_halve`,
`blockify_quantize_pack`. One thread per 4-block GROUP (lane coupling in `compress_group`).

Host dispatch: `src/gpu/cuda.rs` (dlopen libcuda FFI) and `src/gpu/wgpu*.rs` (WGSL lane), behind
feature `gpu`; harness `abgen-verify gpu <diff|bench|corpus>`. Per-device self-qualification
gates every backend (`ABGEN_GPU_BACKEND`, `ABGEN_GPU_QUALIFY`); GPU output byte-identical to the
oracle on RTX PRO 6000 Blackwell (verification record above).
