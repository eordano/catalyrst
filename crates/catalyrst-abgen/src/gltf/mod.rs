mod accessors;
mod load;
mod scene_build;
mod transform;

pub use load::load_gltf_inputs;

use crate::mesh_layout;
use crate::scene::{Primitive, Scene};
use crate::value::Value;
use anyhow::Result;
use scene_build::parse_impl;
use serde_json::Value as J;

pub type Resolve<'a> = Option<&'a (dyn Fn(&str) -> Option<Vec<u8>> + Sync)>;

pub fn parse(
    glb_bytes: &[u8],
    ext: &str,
    resolve: Resolve,
    magenta_missing: bool,
    normalized_attribute_scaling: bool,
) -> Result<Scene> {
    let (gltf, buffers) = load_gltf_inputs(glb_bytes, ext, resolve)?;
    parse_with_inputs(
        &gltf,
        &buffers,
        resolve,
        magenta_missing,
        normalized_attribute_scaling,
    )
}

pub fn parse_with_inputs(
    gltf: &J,
    buffers: &[Vec<u8>],
    resolve: Resolve,
    magenta_missing: bool,
    normalized_attribute_scaling: bool,
) -> Result<Scene> {
    parse_impl(
        gltf,
        buffers,
        resolve,
        magenta_missing,
        normalized_attribute_scaling,
        false,
    )
}

pub fn parse_classify(glb_bytes: &[u8], ext: &str, resolve: Resolve) -> Result<Scene> {
    let (gltf, buffers) = load_gltf_inputs(glb_bytes, ext, resolve)?;
    parse_impl(&gltf, &buffers, resolve, false, false, true)
}

pub fn vertex_buffer(prim: &Primitive) -> (Vec<u8>, Vec<Value>) {
    let single: Vec<Vec<[f64; 2]>>;
    let uv_sets: &[Vec<[f64; 2]>] = if !prim.uv_sets.is_empty() {
        &prim.uv_sets
    } else if let Some(uvs) = &prim.uvs {
        single = vec![uvs.clone()];
        &single
    } else {
        &[]
    };
    let attrs = mesh_layout::MeshAttributes {
        positions: &prim.positions,
        normals: Some(&prim.normals),
        tangents: prim.tangents.as_deref(),
        colors: prim.colors.as_deref(),
        uv_sets,
        weights: prim.weights.as_deref(),
        joints: prim.joints.as_deref(),

        color_unorm16: prim.from_draco && prim.colors.is_some(),
    };
    mesh_layout::vertex_buffer(&attrs)
}

pub fn aabb(
    positions: &[[f64; 3]],
    decl_min: Option<[f64; 3]>,
    decl_max: Option<[f64; 3]>,
) -> (Value, Value) {
    let (min, max) = match (decl_min, decl_max) {
        (Some(mn), Some(mx)) => (mn, mx),
        _ => {
            let mut mn = [f64::INFINITY; 3];
            let mut mx = [f64::NEG_INFINITY; 3];
            for p in positions {
                for i in 0..3 {
                    if p[i] < mn[i] {
                        mn[i] = p[i];
                    }
                    if p[i] > mx[i] {
                        mx[i] = p[i];
                    }
                }
            }
            (mn, mx)
        }
    };

    let ce = |mn: f64, mx: f64| -> (f64, f64) {
        let mnf = mn as f32;
        let mxf = mx as f32;
        let e = (mxf - mnf) * 0.5f32;
        ((mnf + e) as f64, e as f64)
    };
    let (cx, ex) = ce(min[0], max[0]);
    let (cy, ey) = ce(min[1], max[1]);
    let (cz, ez) = ce(min[2], max[2]);
    let center = map! { "x" => cx, "y" => cy, "z" => cz };
    let extent = map! { "x" => ex, "y" => ey, "z" => ez };
    (center, extent)
}

#[cfg(test)]
mod tests {
    use super::accessors::{read_accessor, read_accessor_normalized};
    use super::*;
    use serde_json::json;

    #[test]
    fn normalized_integer_accessors_scale_to_unit_range() {
        let mut buf: Vec<u8> = Vec::new();
        buf.extend_from_slice(&65535u16.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.push(127i8 as u8);
        buf.push((-127i8) as u8);
        buf.push((-128i8) as u8);
        buf.push(0);
        let gltf = json!({
            "accessors": [
                { "componentType": 5123, "type": "VEC2", "count": 1, "bufferView": 0, "normalized": true },
                { "componentType": 5120, "type": "VEC3", "count": 1, "bufferView": 1, "normalized": true },
                { "componentType": 5123, "type": "VEC2", "count": 1, "bufferView": 0 }
            ],
            "bufferViews": [
                { "buffer": 0, "byteOffset": 0, "byteLength": 4 },
                { "buffer": 0, "byteOffset": 4, "byteLength": 3 }
            ],
            "buffers": [{ "byteLength": buf.len() }]
        });
        let buffers = vec![buf];

        let uv = read_accessor_normalized(&gltf, &buffers, 0);
        assert_eq!(uv, vec![vec![1.0, 0.0]]);

        let n = read_accessor_normalized(&gltf, &buffers, 1);
        assert_eq!(n[0][0], 1.0);
        assert_eq!(n[0][1], -1.0);
        assert_eq!(n[0][2], -1.0);

        let raw = read_accessor_normalized(&gltf, &buffers, 2);
        assert_eq!(raw, vec![vec![65535.0, 0.0]]);
    }

    #[test]
    fn parse_gates_normalized_attribute_scaling() {
        let mut bin: Vec<u8> = Vec::new();
        for f in [0f32; 3] {
            bin.extend_from_slice(&f.to_le_bytes());
        }
        for v in [65535u16, 0u16] {
            bin.extend_from_slice(&v.to_le_bytes());
        }
        let json_bytes = serde_json::to_vec(&json!({
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [{"nodes": [0]}],
            "nodes": [{"name": "n", "mesh": 0}],
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0, "TEXCOORD_0": 1}}]}],
            "accessors": [
                {"componentType": 5126, "type": "VEC3", "count": 1, "bufferView": 0},
                {"componentType": 5123, "type": "VEC2", "count": 1, "bufferView": 1,
                 "normalized": true}
            ],
            "bufferViews": [
                {"buffer": 0, "byteOffset": 0, "byteLength": 12},
                {"buffer": 0, "byteOffset": 12, "byteLength": 4}
            ],
            "buffers": [{"byteLength": bin.len()}]
        }))
        .unwrap();
        let mut json_chunk = json_bytes;
        while !json_chunk.len().is_multiple_of(4) {
            json_chunk.push(b' ');
        }
        while !bin.len().is_multiple_of(4) {
            bin.push(0);
        }
        let total = 12 + 8 + json_chunk.len() + 8 + bin.len();
        let mut glb: Vec<u8> = Vec::new();
        glb.extend_from_slice(b"glTF");
        glb.extend_from_slice(&2u32.to_le_bytes());
        glb.extend_from_slice(&(total as u32).to_le_bytes());
        glb.extend_from_slice(&(json_chunk.len() as u32).to_le_bytes());
        glb.extend_from_slice(b"JSON");
        glb.extend_from_slice(&json_chunk);
        glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        glb.extend_from_slice(b"BIN\0");
        glb.extend_from_slice(&bin);

        let raw = super::parse(&glb, ".glb", None, false, false).expect("parse raw");
        let scaled = super::parse(&glb, ".glb", None, false, true).expect("parse scaled");
        let raw_uv = raw.nodes[0].primitives[0].uvs.as_ref().expect("raw uvs")[0];
        let scaled_uv = scaled.nodes[0].primitives[0]
            .uvs
            .as_ref()
            .expect("scaled uvs")[0];
        assert_eq!(raw_uv, [65535.0, 1.0]);
        assert_eq!(scaled_uv, [1.0, 1.0]);
    }

    fn sparse_position_case(idx_ct: i64) {
        let count = 4usize;

        let mut idx_bytes = Vec::new();
        for &i in &[1u32, 3u32] {
            match idx_ct {
                5121 => idx_bytes.push(i as u8),
                5123 => idx_bytes.extend_from_slice(&(i as u16).to_le_bytes()),
                5125 => idx_bytes.extend_from_slice(&i.to_le_bytes()),
                _ => unreachable!(),
            }
        }
        let mut val_bytes = Vec::new();
        for f in [1f32, 2.0, 3.0, 4.0, 5.0, 6.0] {
            val_bytes.extend_from_slice(&f.to_le_bytes());
        }
        let idx_len = idx_bytes.len();
        let mut buf = idx_bytes;
        buf.extend_from_slice(&val_bytes);

        let gltf = json!({
            "accessors": [{
                "componentType": 5126, "type": "VEC3", "count": count,
                "sparse": {
                    "count": 2,
                    "indices": { "bufferView": 0, "componentType": idx_ct },
                    "values": { "bufferView": 1 }
                }
            }],
            "bufferViews": [
                { "buffer": 0, "byteOffset": 0, "byteLength": idx_len },
                { "buffer": 0, "byteOffset": idx_len, "byteLength": 24 }
            ],
            "buffers": [{ "byteLength": buf.len() }]
        });
        let out = read_accessor(&gltf, &[buf], 0);
        assert_eq!(out.len(), count);
        assert_eq!(out[0], vec![0.0, 0.0, 0.0]);
        assert_eq!(out[1], vec![1.0, 2.0, 3.0]);
        assert_eq!(out[2], vec![0.0, 0.0, 0.0]);
        assert_eq!(out[3], vec![4.0, 5.0, 6.0]);
    }

    #[test]
    fn sparse_position_u8_indices() {
        sparse_position_case(5121);
    }
    #[test]
    fn sparse_position_u16_indices() {
        sparse_position_case(5123);
    }
    #[test]
    fn sparse_position_u32_indices() {
        sparse_position_case(5125);
    }

    #[test]
    fn sparse_overlays_dense_base() {
        let mut base = Vec::new();
        for f in [10f32, 11.0, 12.0, 20.0, 21.0, 22.0] {
            base.extend_from_slice(&f.to_le_bytes());
        }
        let base_len = base.len();
        let mut buf = base;
        buf.push(1u8);
        let idx_off = base_len;
        let val_off = base_len + 1;
        for f in [99f32, 98.0, 97.0] {
            buf.extend_from_slice(&f.to_le_bytes());
        }
        let gltf = json!({
            "accessors": [{
                "componentType": 5126, "type": "VEC3", "count": 2, "bufferView": 0,
                "sparse": {
                    "count": 1,
                    "indices": { "bufferView": 1, "componentType": 5121 },
                    "values": { "bufferView": 2 }
                }
            }],
            "bufferViews": [
                { "buffer": 0, "byteOffset": 0, "byteLength": base_len },
                { "buffer": 0, "byteOffset": idx_off, "byteLength": 1 },
                { "buffer": 0, "byteOffset": val_off, "byteLength": 12 }
            ],
            "buffers": [{ "byteLength": buf.len() }]
        });
        let out = read_accessor(&gltf, &[buf], 0);
        assert_eq!(out[0], vec![10.0, 11.0, 12.0]);
        assert_eq!(out[1], vec![99.0, 98.0, 97.0]);
    }

    const ANIM: &str = r#""animations": [{"channels": [], "samplers": []}],"#;

    #[test]
    fn node_names_original_mode_without_animations() {
        let gltf = r#"{
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [{"nodes": [0]}],
            "nodes": [
                {"name": "Root", "children": [1, 2, 3, 4]},
                {"name": "", "mesh": 0},
                {"name": "Mesh"},
                {"name": "Mesh"},
                {}
            ],
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}],
            "accessors": [{"componentType": 5126, "type": "VEC3", "count": 1}]
        }"#;
        let scene = super::parse(gltf.as_bytes(), ".gltf", None, false, false).expect("parse");
        let u = &scene.unique_node_names;
        assert_eq!(u[1], "");
        assert_eq!(u[2], "Mesh");
        assert_eq!(u[3], "Mesh");
        assert_eq!(u[4], "Node-4");
    }

    #[test]
    fn node_names_original_unique_mode_with_animations() {
        let gltf = format!(
            r#"{{
            "asset": {{"version": "2.0"}},
            {ANIM}
            "scene": 0,
            "scenes": [{{"nodes": [0, 6]}}],
            "nodes": [
                {{"name": "Root", "children": [1, 2, 3, 4, 5]}},
                {{"name": "", "mesh": 0}},
                {{"name": "Mesh"}},
                {{"name": "Mesh"}},
                {{"name": "Mesh_0"}},
                {{"name": "Mesh"}},
                {{"name": "Root"}}
            ],
            "meshes": [{{"primitives": [{{"attributes": {{"POSITION": 0}}}}]}}],
            "accessors": [{{"componentType": 5126, "type": "VEC3", "count": 1}}]
        }}"#
        );
        let scene = super::parse(gltf.as_bytes(), ".gltf", None, false, false).expect("parse");
        let u = &scene.unique_node_names;
        assert_eq!(u[1], "Node-1");
        assert_eq!(u[2], "Mesh");
        assert_eq!(u[3], "Mesh_0");
        assert_eq!(u[4], "Mesh_0_0");
        assert_eq!(u[5], "Mesh_1");
        assert_eq!(u[0], "Root");
        assert_eq!(u[6], "Root_0");
    }

    #[test]
    fn node_names_skip_colliders() {
        let gltf = format!(
            r#"{{
            "asset": {{"version": "2.0"}},
            {ANIM}
            "scene": 0,
            "scenes": [{{"nodes": [0]}}],
            "nodes": [
                {{"name": "Root", "children": [1, 2, 3]}},
                {{"name": "wall_collider"}},
                {{"name": "wall_collider"}},
                {{"name": "wall_collider"}}
            ]
        }}"#
        );
        let scene = super::parse(gltf.as_bytes(), ".gltf", None, false, false).expect("parse");
        let u = &scene.unique_node_names;
        assert_eq!(u[1], "wall_collider");
        assert_eq!(u[2], "wall_collider");
        assert_eq!(u[3], "wall_collider");
    }

    const EXT_BUF_GLTF: &str = r#"{
        "asset": {"version": "2.0"},
        "buffers": [{"byteLength": 42, "uri": "diamond.bin"}]
    }"#;

    #[test]
    fn external_buffer_unresolved_is_an_error() {
        let err = load_gltf_inputs(EXT_BUF_GLTF.as_bytes(), ".gltf", None)
            .expect_err("missing resolver must fail");
        assert!(err.to_string().contains("diamond.bin"), "{err}");

        let none = |_: &str| -> Option<Vec<u8>> { None };
        let err = load_gltf_inputs(EXT_BUF_GLTF.as_bytes(), ".gltf", Some(&none))
            .expect_err("unresolved uri must fail");
        assert!(err.to_string().contains("diamond.bin"), "{err}");
    }

    #[test]
    fn external_buffer_short_read_is_an_error() {
        let short = |_: &str| -> Option<Vec<u8>> { Some(vec![0u8; 10]) };
        let err = load_gltf_inputs(EXT_BUF_GLTF.as_bytes(), ".gltf", Some(&short))
            .expect_err("short buffer must fail");
        assert!(err.to_string().contains("byteLength"), "{err}");
    }

    #[test]
    fn external_buffer_resolved_bytes_are_used() {
        let ok = |uri: &str| -> Option<Vec<u8>> { (uri == "diamond.bin").then(|| vec![7u8; 42]) };
        let (_, buffers) =
            load_gltf_inputs(EXT_BUF_GLTF.as_bytes(), ".gltf", Some(&ok)).expect("resolves");
        assert_eq!(buffers.len(), 1);
        assert_eq!(buffers[0], vec![7u8; 42]);
    }
}
