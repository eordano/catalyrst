#[cfg(not(target_arch = "wasm32"))]
pub mod assemble;
pub mod atlas;
pub mod crop;
pub mod emit;
pub mod model;
#[cfg(not(target_arch = "wasm32"))]
pub mod placements;
#[cfg(not(target_arch = "wasm32"))]
pub mod simplify;
#[cfg(not(target_arch = "wasm32"))]
pub mod simplify_meshopt;

mod gate;
#[cfg(not(target_arch = "wasm32"))]
mod pipeline;
#[cfg(test)]
mod tests;

pub use gate::{gate_failures, self_gate_bundle, self_gate_bundle_with, GateCheck};
#[cfg(not(target_arch = "wasm32"))]
pub use pipeline::{
    acquire_placements, choose_lane, effective_tri_cap, expected_rel_path, generate,
    normalize_levels, parse_parcel, scene_geometry, staged_glb_name, write_iss_descriptor,
    GenerateOutcome, GenerateParams, LevelBuild, SimplifyLane, TRIS_PER_PARCEL,
};

#[cfg(test)]
use crate::catalyst::CatalystClient;
#[cfg(test)]
use crate::lods;
#[cfg(test)]
use crate::unity::bundle_file::{Bundle, FileContent};
#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::path::PathBuf;
