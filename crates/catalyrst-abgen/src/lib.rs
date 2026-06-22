#![allow(
    clippy::type_complexity,
    clippy::inherent_to_string,
    clippy::too_many_arguments,
    clippy::needless_range_loop
)]

#[global_allocator]
static GLOBAL_ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[macro_use]
pub mod value;
pub mod scene;

pub mod alpha_bleed;
pub mod animation;
pub mod animation_mecanim;
pub mod cabname;
pub mod catalyst;
pub mod draco;
pub mod glbscan;
pub mod gltf;
pub mod hashes;
pub mod live;
pub mod local_store;
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
pub mod space;
pub mod tangents;
pub mod texprofile;

pub mod bc5_pure;
pub mod bc7_mode_tree;
pub mod bc7_pure;
pub mod dxt1_pure;
pub mod dxt_unity;
pub mod ffi;

pub mod unity;

pub mod builder;
pub mod bundle;

pub mod compress;
pub mod lods;
pub mod regen;
pub mod shader;
pub mod validate;
pub mod wearables;

pub use anyhow::{anyhow, bail, Context, Result};
