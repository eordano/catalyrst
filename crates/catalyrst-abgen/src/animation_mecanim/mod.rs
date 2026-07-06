mod audit;
mod build;
mod curves;

pub use audit::{binding_key_dump, binding_max_diffs, binding_tie_audit, clip_partition_counts};
pub use build::{build_animator_component, build_animator_controller, build_mecanim_clips};
pub use curves::{CONST_CURVE_SLOPE_TOL, CONST_CURVE_VALUE_TOL};

const INTERP_STEP: &str = "STEP";
const INTERP_CUBICSPLINE: &str = "CUBICSPLINE";
const INTERP_LINEAR: &str = "LINEAR";

const SAMPLE_RATE: f64 = 60.0;
const WRAP_LOOP: i64 = 2;
const LOOP_PARAMETER: &str = "Loop";

const PARAM_TYPE_BOOL: i64 = 4;
const PARAM_TYPE_TRIGGER: i64 = 9;

const ATTR_POSITION: i64 = 1;
const ATTR_ROTATION: i64 = 2;
const ATTR_SCALE: i64 = 3;
const TRANSFORM_CLASS_ID: i64 = 4;

const SELECTOR_EXIT_DEST: i64 = 30000;

fn crc32(s: &str) -> u32 {
    crate::hashes::crc32(s.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::curves::{
        bake_scalar_keys, classify_constant, encode_streamed_clip, partition_curves, Key,
    };
    use super::*;
    use crate::animation::glb;
    use crate::value::Value;

    #[test]
    fn crc32_known_values() {
        assert_eq!(crc32("Base Layer"), 0x2d18_2308);
        assert_eq!(crc32("Loop"), 0x016d_b2d0);
        assert_eq!(crc32("GravityWeight"), 0x7d7f_be84);
    }

    fn k(v: f64) -> Key {
        Key {
            time: 0.0,
            value: v,
            slope: 0.0,
            a: 0.0,
            b: 0.0,
        }
    }
    fn flat(v: f64) -> Vec<Key> {
        vec![k(v), Key { time: 1.0, ..k(v) }]
    }
    fn varying(a: f64, b: f64) -> Vec<Key> {
        vec![k(a), Key { time: 1.0, ..k(b) }]
    }

    #[test]
    fn constant_collapse_is_binding_atomic() {
        let scalar_curves: Vec<(i64, Vec<Key>)> = vec![
            (0, flat(1.0)),
            (1, varying(0.0, 2.0)),
            (2, flat(3.0)),
            (3, flat(0.0)),
            (4, flat(0.0)),
            (5, varying(0.0, 0.1)),
            (6, flat(1.0)),
            (7, flat(1.0)),
            (8, flat(1.0)),
            (9, flat(1.0)),
        ];
        let bindings: Vec<(String, i64, bool)> = vec![
            ("p".into(), ATTR_POSITION, false),
            ("p".into(), ATTR_ROTATION, false),
            ("p".into(), ATTR_SCALE, false),
        ];

        let class = classify_constant(&scalar_curves, &bindings, CONST_CURVE_VALUE_TOL);
        assert_eq!(&class[0..3], &[false, false, false]);
        assert_eq!(&class[3..7], &[false, false, false, false]);
        assert_eq!(&class[7..10], &[true, true, true]);

        let (streamed, constant, n) = partition_curves(&scalar_curves, &class);
        assert_eq!(constant, vec![1.0, 1.0, 1.0]);
        assert_eq!(n, 7);
        let idxs: Vec<i64> = streamed.iter().map(|(i, _)| *i).collect();
        assert_eq!(idxs, vec![0, 1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn step_curves_never_collapse() {
        let scalar_curves: Vec<(i64, Vec<Key>)> =
            vec![(0, flat(1.0)), (1, flat(2.0)), (2, flat(3.0))];
        let bindings: Vec<(String, i64, bool)> = vec![("p".into(), ATTR_POSITION, true)];
        let class = classify_constant(&scalar_curves, &bindings, CONST_CURVE_VALUE_TOL);
        assert_eq!(class, vec![false, false, false]);
    }

    #[test]
    fn near_constant_within_tolerance_collapses() {
        let near: Vec<Key> = vec![
            k(0.5),
            Key {
                time: 1.0,
                ..k(0.5 + 1.19e-7)
            },
        ];
        let wide: Vec<Key> = vec![
            k(0.5),
            Key {
                time: 1.0,
                ..k(0.5 + 1.0e-6)
            },
        ];
        let scalar_curves: Vec<(i64, Vec<Key>)> = vec![
            (0, near),
            (1, flat(0.2)),
            (2, flat(0.3)),
            (3, wide),
            (4, flat(0.2)),
            (5, flat(0.3)),
        ];
        let bindings: Vec<(String, i64, bool)> = vec![
            ("a".into(), ATTR_POSITION, false),
            ("b".into(), ATTR_POSITION, false),
        ];
        let class = classify_constant(&scalar_curves, &bindings, CONST_CURVE_VALUE_TOL);
        assert_eq!(&class[0..3], &[true, true, true]);
        assert_eq!(&class[3..6], &[false, false, false]);
    }

    #[test]
    fn rotation_all_constant_collapses() {
        let scalar_curves: Vec<(i64, Vec<Key>)> = vec![
            (0, flat(0.0)),
            (1, flat(0.0)),
            (2, flat(0.0)),
            (3, flat(1.0)),
        ];
        let bindings: Vec<(String, i64, bool)> = vec![("p".into(), ATTR_ROTATION, false)];
        let class = classify_constant(&scalar_curves, &bindings, CONST_CURVE_VALUE_TOL);
        assert_eq!(class, vec![true, true, true, true]);
        let (_s, constant, n) = partition_curves(&scalar_curves, &class);
        assert_eq!(n, 0);
        assert_eq!(constant, vec![0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn streamed_xcheck() {
        let c0 = bake_scalar_keys(&[0.0, 1.0], &[0.0, 1.0], "LINEAR");
        let c1 = bake_scalar_keys(&[0.0, 0.5, 1.0], &[2.0, 3.0, 4.0], "LINEAR");
        let data = encode_streamed_clip(&[(0i64, c0), (1i64, c1)]);
        let nums: Vec<u32> = data.iter().map(|v| v.as_i64().unwrap() as u32).collect();

        let expect: Vec<u32> = vec![
            4286578687, 2, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1073741824, 0, 2, 0, 0, 0, 1065353216, 0, 1,
            0, 0, 1073741824, 1073741824, 1056964608, 1, 1, 0, 0, 1073741824, 1077936128,
            1065353216, 2, 0, 0, 0, 0, 1065353216, 1, 0, 0, 0, 1082130432, 2139095040, 0,
        ];
        assert_eq!(nums, expect);
    }

    #[test]
    fn streamed_clip_leading_and_trailing_frames() {
        let keys = bake_scalar_keys(&[0.0, 1.0], &[0.0, 1.0], INTERP_LINEAR);
        let data = encode_streamed_clip(&[(0i64, keys)]);

        assert!(!data.is_empty());
        let first = data[0].as_i64().unwrap() as u32;
        assert_eq!(first, 0xFF7F_FFFF);
    }

    fn mecanim_base_clip() -> Value {
        map! {
            "m_MuscleClip" => map! {
                "m_Clip" => map! { "data" => map!{} },
            },
        }
    }

    fn f32_bytes(vals: &[f32]) -> Vec<u8> {
        vals.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    const TEXT_GLTF_TWO_BUFFERS: &str = r#"{
        "asset": {"version": "2.0"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"name": "Armature", "children": [1]}, {"name": "Bone"}],
        "animations": [{
            "name": "TestClip",
            "channels": [{"sampler": 0, "target": {"node": 1, "path": "translation"}}],
            "samplers": [{"input": 0, "output": 1, "interpolation": "LINEAR"}]
        }],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": 2, "type": "SCALAR"},
            {"bufferView": 1, "componentType": 5126, "count": 2, "type": "VEC3"}
        ],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": 8},
            {"buffer": 1, "byteOffset": 0, "byteLength": 24}
        ],
        "buffers": [
            {"byteLength": 8, "uri": "times.bin"},
            {"byteLength": 24, "uri": "values.bin"}
        ]
    }"#;

    fn two_buffers() -> Vec<Vec<u8>> {
        vec![
            f32_bytes(&[0.0, 1.0]),
            f32_bytes(&[0.0, 0.0, 0.0, 1.0, 2.0, 3.0]),
        ]
    }

    #[test]
    fn text_gltf_multi_buffer_builds_mecanim_clips() {
        let gltf: serde_json::Value = serde_json::from_str(TEXT_GLTF_TWO_BUFFERS).unwrap();
        let clips = build_mecanim_clips(&gltf, &two_buffers(), &mecanim_base_clip());
        assert_eq!(clips.len(), 1);
        let clip = &clips[0];
        assert_eq!(clip.get("m_Name").unwrap().as_str(), Some("TestClip"));

        let gb = clip
            .get("m_ClipBindingConstant")
            .unwrap()
            .get("genericBindings")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(gb.len(), 1);
        assert_eq!(
            gb[0].get("attribute").unwrap().as_i64(),
            Some(ATTR_POSITION)
        );

        let mc = clip.get("m_MuscleClip").unwrap();
        let data = mc.get("m_Clip").unwrap().get("data").unwrap();
        assert_eq!(
            data.get("m_StreamedClip")
                .unwrap()
                .get("curveCount")
                .unwrap()
                .as_i64(),
            Some(3)
        );
        assert!(matches!(
            data.get("m_ConstantClip").unwrap().get("data"),
            Some(Value::Array(a)) if a.is_empty()
        ));
        assert!(mc.get("m_StopTime") == Some(&Value::Float(1.0)));
    }

    fn pack_glb(json: &str, bin: &[u8]) -> Vec<u8> {
        let mut j = json.as_bytes().to_vec();
        while !j.len().is_multiple_of(4) {
            j.push(b' ');
        }
        let mut b = bin.to_vec();
        while !b.len().is_multiple_of(4) {
            b.push(0);
        }
        let total = 12 + 8 + j.len() + 8 + b.len();
        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&(j.len() as u32).to_le_bytes());
        out.extend_from_slice(&0x4E4F_534Au32.to_le_bytes());
        out.extend_from_slice(&j);
        out.extend_from_slice(&(b.len() as u32).to_le_bytes());
        out.extend_from_slice(&0x004E_4942u32.to_le_bytes());
        out.extend_from_slice(&b);
        out
    }

    #[test]
    fn glb_and_text_gltf_produce_identical_clips() {
        let glb_json = r#"{
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [{"nodes": [0]}],
            "nodes": [{"name": "Armature", "children": [1]}, {"name": "Bone"}],
            "animations": [{
                "name": "TestClip",
                "channels": [{"sampler": 0, "target": {"node": 1, "path": "translation"}}],
                "samplers": [{"input": 0, "output": 1, "interpolation": "LINEAR"}]
            }],
            "accessors": [
                {"bufferView": 0, "componentType": 5126, "count": 2, "type": "SCALAR"},
                {"bufferView": 1, "componentType": 5126, "count": 2, "type": "VEC3"}
            ],
            "bufferViews": [
                {"buffer": 0, "byteOffset": 0, "byteLength": 8},
                {"buffer": 0, "byteOffset": 8, "byteLength": 24}
            ],
            "buffers": [{"byteLength": 32}]
        }"#;
        let bin = f32_bytes(&[0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0]);
        let glb_bytes = pack_glb(glb_json, &bin);
        let g = glb::parse(&glb_bytes);

        let base = mecanim_base_clip();
        let clips_bin = build_mecanim_clips(&g.json, std::slice::from_ref(&g.bin), &base);
        let gltf_txt: serde_json::Value = serde_json::from_str(TEXT_GLTF_TWO_BUFFERS).unwrap();
        let clips_txt = build_mecanim_clips(&gltf_txt, &two_buffers(), &base);

        assert_eq!(clips_bin.len(), 1);
        assert!(
            clips_bin == clips_txt,
            "GLB-embedded and text-.gltf multi-buffer forms of the same \
             animation must produce identical mecanim clips"
        );
    }

    #[test]
    fn controller_default_state_and_loop_param() {
        let base = map! { "m_Name" => "x" };
        let ctrl = build_animator_controller(&[("Wave".to_string(), 5)], &base);

        let va = ctrl
            .get("m_Controller")
            .unwrap()
            .get("m_Values")
            .unwrap()
            .get("data")
            .unwrap()
            .get("m_ValueArray")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(va.len(), 2);
        assert_eq!(va[0].get("m_Type").unwrap().as_i64(), Some(PARAM_TYPE_BOOL));
        assert_eq!(
            va[1].get("m_Type").unwrap().as_i64(),
            Some(PARAM_TYPE_TRIGGER)
        );

        let states = ctrl
            .get("m_Controller")
            .unwrap()
            .get("m_StateMachineArray")
            .unwrap()
            .as_array()
            .unwrap()[0]
            .get("data")
            .unwrap()
            .get("m_StateConstantArray")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(states.len(), 2);
    }
}
