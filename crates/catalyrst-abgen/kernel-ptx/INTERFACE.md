# abgen-gpu kernel core - binding interface contract

`src/core/` is the `no_std` port of catalyrst-abgen's pure block encoders
(bc7, mode_tree, bc5, dxt1, mips). It compiles natively (CPU oracle diffing,
shared into the host via `#[path]` from `src/gpu/mod.rs`) and to PTX for
NVIDIA GPUs (this crate).

The port is arithmetic-exact: same operations, same order, same literals,
same integer widths, same float types as the source in
`crates/catalyrst-abgen/src/{bc7_pure,bc7_mode_tree,bc5_pure,dxt1_pure}.rs`.
The project exists to produce byte-identical output to that oracle; a cleaner
expression that changes one bit is a bug. Rules that still govern any change
here:

- `core/` compiles under `#![no_std]` except items gated
  `#[cfg(feature = "std")]`. `crate::sqrtf` instead of `f32::sqrt`; no env,
  threads, OnceLock, or rayon in core.
- Source statics built via `opt()`/`build_opt_tables()` are explicit
  `&OptTables` parameters threaded through every function that used them.
- Scalar branches only - no AVX2/AVX-512 paths, no rayon, no capture/env
  machinery.
- CRN (crunch C++ FFI) is excluded; it stays CPU-side in catalyrst-abgen.
- If bit-exactness is at risk (cross-block coupling, lazy LUTs, float
  intrinsics beyond sqrt), stop and report instead of approximating.
- No comments in `.rs` files (repo policy).

Kernel entry points (`src/lib.rs`, stable extern names the host resolves via
`cuModuleGetFunction`): `bc7_encode_groups`, `bc7_encode_groups_perm`,
`bc7_encode_groups_desc`, `bc7_group_sigs`, `bc7_group_sigs_desc`,
`blockify_linearize`, `blockify_halve`, `blockify_quantize_pack`. One thread
per 4-block GROUP (lane coupling in `compress_group`).

Host dispatch: `src/gpu/cuda.rs` (dlopen libcuda FFI) and `src/gpu/wgpu*.rs`
(WGSL lane), behind feature `gpu`; harness `abgen-verify gpu <diff|bench|corpus>`. Per-device self-qualification gates every backend
(`ABGEN_GPU_BACKEND`, `ABGEN_GPU_QUALIFY`); GPU output byte-identical to the
oracle on RTX PRO 6000 Blackwell (see BUILD.md verification record).
