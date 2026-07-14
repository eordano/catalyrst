#![allow(
    clippy::type_complexity,
    clippy::inherent_to_string,
    clippy::too_many_arguments,
    clippy::needless_range_loop
)]

#[cfg(all(not(target_os = "windows"), not(target_arch = "wasm32")))]
#[global_allocator]
static GLOBAL_ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[macro_use]
pub mod value;
#[cfg(feature = "gpu")]
pub mod gpu;
#[cfg(feature = "gpu")]
pub mod gpu_dispatch;
#[cfg(feature = "gpu")]
pub mod gpuhost;
pub mod scene;

pub mod alpha_bleed;
pub mod animation;
pub mod animation_mecanim;
pub mod cabname;
#[cfg(not(target_arch = "wasm32"))]
pub mod catalyst;
pub mod clihelp;
pub mod dates;
pub mod detmath;
pub mod draco;
#[cfg(not(target_arch = "wasm32"))]
pub mod glbscan;
pub mod gltf;
pub mod hashes;
#[cfg(not(target_arch = "wasm32"))]
pub mod live;
#[cfg(not(target_arch = "wasm32"))]
pub mod local_store;
pub mod lodgen;
pub mod lz4;
pub mod manifest;
pub mod materials;
pub mod mesh_layout;
pub mod naming;
pub mod normals;
pub mod pathids;
pub mod placeholder;
pub mod png;
pub mod resize;
pub mod ress;
pub mod sbp_order;
pub mod skeleton;
#[cfg(not(target_arch = "wasm32"))]
pub mod space;
pub mod tangents;
pub mod texprofile;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod tmppath;
#[cfg(not(target_arch = "wasm32"))]
pub mod worlds;

pub mod bc5_pure;
pub mod bc7_mode_tree;
pub mod bc7_pure;
pub mod dxt1_pure;
pub mod dxt_unity;
#[cfg(not(target_arch = "wasm32"))]
pub mod ffi;
#[cfg(target_arch = "wasm32")]
#[path = "ffi_wasm.rs"]
pub mod ffi;

pub mod unity;

pub mod builder;
pub mod bundle;

pub mod compress;
pub mod lods;
#[cfg(not(target_arch = "wasm32"))]
pub mod regen;
pub mod shader;
pub mod validate;
#[cfg(not(target_arch = "wasm32"))]
pub mod wearables;

#[cfg(not(target_arch = "wasm32"))]
pub mod abcdn;

pub use anyhow::{anyhow, bail, Context, Result};

#[cfg(feature = "gpu")]
pub fn enable_gpu() -> std::result::Result<(), String> {
    gpu_dispatch::enable()
}

#[cfg(not(feature = "gpu"))]
pub fn enable_gpu() -> std::result::Result<(), String> {
    Err("this binary was built without the gpu feature (rebuild with --features gpu)".to_string())
}

#[cfg(feature = "gpu")]
pub fn gpu_status() -> Option<(&'static str, bool, Option<String>)> {
    gpu::gpu_status()
}

#[cfg(not(feature = "gpu"))]
pub fn gpu_status() -> Option<(&'static str, bool, Option<String>)> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
pub fn maybe_enable_gpu_from_env() {
    if clihelp::env_bool("ABGEN_GPU", false) || clihelp::env_bool("CATALYRST_ABGEN_GPU", false) {
        if let Err(e) = enable_gpu() {
            eprintln!("error: ABGEN_GPU: {e}");
            std::process::exit(2);
        }
    }
}
