use crate::value::{Map, Value};

const INTERP_STEP: &str = "STEP";
const INTERP_CUBICSPLINE: &str = "CUBICSPLINE";
const INTERP_LINEAR: &str = "LINEAR";

const ROTATION_ORDER: i64 = 4;
const SAMPLE_RATE: f64 = 60.0;
const WRAP_LOOP: i64 = 2;

const PATH_TRANSLATION: &str = "translation";
const PATH_ROTATION: &str = "rotation";
const PATH_SCALE: &str = "scale";
const PATH_WEIGHTS: &str = "weights";

const CLASS_ID_SMR: i64 = 137;
const DEFAULT_WEIGHT: f64 = 0.3333333432674408;

pub(crate) mod glb {
    use serde_json::Value as J;

    pub struct Glb {
        pub json: J,
        pub bin: Vec<u8>,
    }

    pub fn parse(bytes: &[u8]) -> Glb {
        assert!(bytes.len() >= 12, "glb too short");
        let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        assert_eq!(magic, 0x4654_6C67, "bad glb magic");

        let mut json: Option<J> = None;
        let mut bin: Vec<u8> = Vec::new();
        let mut off = 12usize;
        while off + 8 <= bytes.len() {
            let clen =
                u32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
                    as usize;
            let ctype = u32::from_le_bytes([
                bytes[off + 4],
                bytes[off + 5],
                bytes[off + 6],
                bytes[off + 7],
            ]);
            let cstart = off + 8;
            let cend = cstart + clen;
            if cend > bytes.len() {
                break;
            }
            let chunk = &bytes[cstart..cend];
            match ctype {
                0x4E4F_534A => {
                    json = Some(serde_json::from_slice(chunk).expect("glb json parse"));
                }
                0x004E_4942 => {
                    bin = chunk.to_vec();
                }
                _ => {}
            }
            off = cend;
        }
        Glb {
            json: json.expect("glb missing JSON chunk"),
            bin,
        }
    }

    fn comp_size(component_type: i64) -> usize {
        match component_type {
            5120 | 5121 => 1,
            5122 | 5123 => 2,
            5125 | 5126 => 4,
            other => panic!("unknown componentType {other}"),
        }
    }

    fn type_dim(t: &str) -> usize {
        match t {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            "MAT4" => 16,
            other => panic!("unknown accessor type {other}"),
        }
    }

    fn read_one(buf: &[u8], off: usize, component_type: i64) -> f64 {
        match component_type {
            5120 => buf[off] as i8 as f64,
            5121 => buf[off] as f64,
            5122 => i16::from_le_bytes([buf[off], buf[off + 1]]) as f64,
            5123 => u16::from_le_bytes([buf[off], buf[off + 1]]) as f64,
            5125 => u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]) as f64,
            5126 => f32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]) as f64,
            other => panic!("unknown componentType {other}"),
        }
    }

    pub fn read_accessor_with_buffers(
        gltf: &J,
        buffers: &[Vec<u8>],
        acc_idx: usize,
    ) -> Vec<Vec<f64>> {
        let acc = &gltf["accessors"][acc_idx];
        let component_type = acc["componentType"].as_i64().expect("componentType");
        let t = acc["type"].as_str().expect("accessor type");
        let dim = type_dim(t);
        let csize = comp_size(component_type);
        let count = acc["count"].as_u64().expect("accessor count") as usize;

        let Some(bv_idx) = acc["bufferView"].as_u64().map(|x| x as usize) else {
            return vec![vec![0.0; dim]; count];
        };
        let bv = &gltf["bufferViews"][bv_idx];
        let buffer_idx = bv["buffer"].as_u64().unwrap_or(0) as usize;
        let buf = &buffers[buffer_idx];
        let bv_off = bv["byteOffset"].as_u64().unwrap_or(0) as usize;
        let acc_off = acc["byteOffset"].as_u64().unwrap_or(0) as usize;
        let start = bv_off + acc_off;
        let stride = bv["byteStride"]
            .as_u64()
            .map(|s| s as usize)
            .unwrap_or(csize * dim);
        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            let off = start + i * stride;
            let mut elem = Vec::with_capacity(dim);
            for c in 0..dim {
                elem.push(read_one(buf, off + c * csize, component_type));
            }
            out.push(elem);
        }
        out
    }

    pub fn node_names_and_parents(glb: &Glb) -> (Vec<String>, Vec<i64>) {
        node_names_and_parents_from_json(&glb.json)
    }

    pub fn node_names_and_parents_from_json(gltf: &J) -> (Vec<String>, Vec<i64>) {
        let nodes = gltf["nodes"].as_array().cloned().unwrap_or_default();
        let meshes = gltf["meshes"].as_array().cloned().unwrap_or_default();
        let n = nodes.len();
        let mut parent = vec![-1i64; n];
        let mut children_of: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (i, node) in nodes.iter().enumerate() {
            if let Some(children) = node["children"].as_array() {
                for c in children {
                    if let Some(ci) = c.as_u64() {
                        let ci = ci as usize;
                        parent[ci] = i as i64;
                        children_of[i].push(ci);
                    }
                }
            }
        }

        let mut names: Vec<String> = nodes
            .iter()
            .enumerate()
            .map(|(i, node)| {
                let explicit = node["name"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string());
                if let Some(s) = explicit {
                    return s;
                }
                if let Some(mi) = node["mesh"].as_u64() {
                    if let Some(mesh) = meshes.get(mi as usize) {
                        if let Some(mn) = mesh["name"].as_str() {
                            if !mn.is_empty() {
                                return mn.to_string();
                            }
                        }
                    }
                }
                format!("Node-{i}")
            })
            .collect();

        let mut root_siblings: Vec<usize> = (0..n).filter(|&i| parent[i] == -1).collect();
        root_siblings.sort();
        let disambig = |order: &[usize], names: &mut Vec<String>| {
            let mut counts: std::collections::HashMap<String, usize> =
                std::collections::HashMap::new();
            for &ci in order {
                let base = names[ci].clone();
                let seen = counts.entry(base.clone()).or_insert(0);
                if *seen > 0 {
                    names[ci] = format!("{base}_{}", *seen - 1);
                }
                *seen += 1;
            }
        };
        disambig(&root_siblings, &mut names);
        for kids in &children_of {
            if kids.len() > 1 {
                disambig(kids, &mut names);
            }
        }

        let mut referenced = vec![false; n];
        for kids in &children_of {
            for &c in kids {
                referenced[c] = true;
            }
        }
        if let Some(scenes) = gltf["scenes"].as_array() {
            for s in scenes {
                if let Some(roots) = s["nodes"].as_array() {
                    for r in roots {
                        if let Some(ri) = r.as_u64() {
                            if (ri as usize) < n {
                                referenced[ri as usize] = true;
                            }
                        }
                    }
                }
            }
        }
        for (i, r) in referenced.iter().enumerate() {
            if !r {
                names[i] = String::new();
            }
        }
        (names, parent)
    }

    pub fn animation_path(node_idx: usize, names: &[String], parent: &[i64]) -> String {
        let mut parts: Vec<&str> = Vec::new();
        let mut i = node_idx as i64;
        while i >= 0 {
            parts.insert(0, &names[i as usize]);
            i = parent[i as usize];
        }
        parts.join("/")
    }
}

fn vec_keyframe(time: f64, val: &[f64], in_t: &[f64], out_t: &[f64], quat: bool) -> Value {
    let keys: &[&str] = if quat {
        &["x", "y", "z", "w"]
    } else {
        &["x", "y", "z"]
    };
    let v = |t: &[f64]| -> Value {
        let mut m = Map::new();
        for (i, k) in keys.iter().enumerate() {
            m.insert(*k, t[i]);
        }
        Value::Map(m)
    };
    let zerow = |val: f64| -> Value {
        let mut m = Map::new();
        for k in keys.iter() {
            m.insert(*k, val);
        }
        Value::Map(m)
    };
    map! {
        "time" => time,
        "value" => v(val),
        "inSlope" => v(in_t),
        "outSlope" => v(out_t),
        "weightedMode" => 0,
        "inWeight" => zerow(0.0),
        "outWeight" => zerow(0.0),
    }
}

fn vec_keyframe_w(
    time: f64,
    val: &[f64],
    in_t: &[f64],
    out_t: &[f64],
    in_w: &[f64],
    out_w: &[f64],
    quat: bool,
) -> Value {
    let keys: &[&str] = if quat {
        &["x", "y", "z", "w"]
    } else {
        &["x", "y", "z"]
    };
    let v = |t: &[f64]| -> Value {
        let mut m = Map::new();
        for (i, k) in keys.iter().enumerate() {
            m.insert(*k, t[i]);
        }
        Value::Map(m)
    };
    map! {
        "time" => time,
        "value" => v(val),
        "inSlope" => v(in_t),
        "outSlope" => v(out_t),
        "weightedMode" => 0,
        "inWeight" => v(in_w),
        "outWeight" => v(out_w),
    }
}

fn bake_vec_curve(times: &[f64], values: &[Vec<f64>], interp: &str, quat: bool) -> Vec<Value> {
    let n = times.len();
    let width = if quat { 4 } else { 3 };
    let zero = vec![0.0f64; width];
    let mut out: Vec<Value> = Vec::new();

    if interp == INTERP_STEP {
        if n == 1 {
            let mut in_t = vec![0.0f64; width];
            in_t[0] = f64::INFINITY;
            let mut wt = vec![DEFAULT_WEIGHT; width];
            wt[0] = 0.0;
            out.push(vec_keyframe_w(
                times[0], &values[0], &in_t, &zero, &wt, &wt, quat,
            ));
            return out;
        }
        let inf = vec![f64::INFINITY; width];
        for i in 0..n {
            out.push(vec_keyframe(times[i], &values[i], &inf, &zero, quat));
        }
        return out;
    }
    if interp == INTERP_CUBICSPLINE {
        let keys: &[&str] = if quat {
            &["x", "y", "z", "w"]
        } else {
            &["x", "y", "z"]
        };
        for i in 0..n {
            let in_t = &values[i * 3];
            let val = &values[i * 3 + 1];
            let out_t = &values[i * 3 + 2];
            let mut kf = vec_keyframe(times[i], val, in_t, out_t, quat);

            let mut iw = Map::new();
            let mut ow = Map::new();
            for (kk, k) in keys.iter().enumerate() {
                let w = if n == 1 && kk > 0 {
                    DEFAULT_WEIGHT
                } else {
                    0.5
                };
                iw.insert(*k, w);
                ow.insert(*k, w);
            }
            kf.insert("weightedMode", 3);
            kf.insert("inWeight", Value::Map(iw));
            kf.insert("outWeight", Value::Map(ow));
            out.push(kf);
        }
        return out;
    }

    if n == 1 {
        let mut wt = vec![DEFAULT_WEIGHT; width];
        wt[0] = 0.0;
        out.push(vec_keyframe_w(
            times[0], &values[0], &zero, &zero, &wt, &wt, quat,
        ));
        return out;
    }

    const K_TIME_EPSILON: f32 = 1e-5;
    let mut prev_t = times[0];
    let mut prev_v: Vec<f64> = values[0].clone();
    let mut in_t: Vec<f64> = zero.clone();
    for i in 1..n {
        let t = times[i];
        let mut v: Vec<f64> = values[i].clone();
        if prev_t >= t {
            continue;
        }
        if quat {
            let dot: f64 = (0..4).map(|k| prev_v[k] * v[k]).sum();
            if dot < 0.0 {
                v = v.iter().map(|x| -x).collect();
            }
        }
        let d_t = (t as f32) - (prev_t as f32);
        let out_t: Vec<f64> = (0..width)
            .map(|k| {
                let d_v = (v[k] as f32) - (prev_v[k] as f32);
                if d_t < K_TIME_EPSILON {
                    let neg = (d_v < 0.0) ^ (d_t < 0.0);
                    if neg {
                        f64::NEG_INFINITY
                    } else {
                        f64::INFINITY
                    }
                } else {
                    (d_v / d_t) as f64
                }
            })
            .collect();
        out.push(vec_keyframe(prev_t, &prev_v, &in_t, &out_t, quat));
        in_t = out_t;
        prev_t = t;
        prev_v = v;
    }
    out.push(vec_keyframe(prev_t, &prev_v, &in_t, &zero, quat));
    out
}

fn scalar_keyframe(time: f64, val: f64, in_slope: f64, out_slope: f64) -> Value {
    map! {
        "time" => time,
        "value" => val,
        "inSlope" => in_slope,
        "outSlope" => out_slope,
        "weightedMode" => 0,
        "inWeight" => 0.0f64,
        "outWeight" => 0.0f64,
    }
}

fn scalar_keyframe_w(
    time: f64,
    val: f64,
    in_slope: f64,
    out_slope: f64,
    in_weight: f64,
    out_weight: f64,
) -> Value {
    map! {
        "time" => time,
        "value" => val,
        "inSlope" => in_slope,
        "outSlope" => out_slope,
        "weightedMode" => 0,
        "inWeight" => in_weight,
        "outWeight" => out_weight,
    }
}

fn bake_scalar_curve(times: &[f64], values: &[f64], interp: &str) -> Vec<Value> {
    let n = times.len();
    let mut out: Vec<Value> = Vec::new();

    if interp == INTERP_STEP {
        if n == 1 {
            out.push(scalar_keyframe_w(
                times[0],
                values[0],
                f64::INFINITY,
                0.0,
                0.0,
                DEFAULT_WEIGHT,
            ));
            return out;
        }
        for i in 0..n {
            out.push(scalar_keyframe(times[i], values[i], f64::INFINITY, 0.0));
        }
        return out;
    }
    if interp == INTERP_CUBICSPLINE {
        for i in 0..n {
            let in_t = values[i * 3];
            let val = values[i * 3 + 1];
            let out_t = values[i * 3 + 2];
            let mut kf = scalar_keyframe(times[i], val, in_t, out_t);
            kf.insert("inWeight", 0.5f64);
            kf.insert("outWeight", 0.5f64);
            out.push(kf);
        }
        return out;
    }

    const K_TIME_EPSILON: f32 = 1e-5;
    let mut prev_t = times[0];
    let mut prev_v = values[0];
    let mut in_slope: f64 = 0.0;
    for i in 1..n {
        let t = times[i];
        let v = values[i];
        if prev_t >= t {
            continue;
        }
        let d_t = (t as f32) - (prev_t as f32);
        let out_slope: f64 = {
            let d_v = (v as f32) - (prev_v as f32);
            if d_t < K_TIME_EPSILON {
                let neg = (d_v < 0.0) ^ (d_t < 0.0);
                if neg {
                    f64::NEG_INFINITY
                } else {
                    f64::INFINITY
                }
            } else {
                (d_v / d_t) as f64
            }
        };
        out.push(scalar_keyframe(prev_t, prev_v, in_slope, out_slope));
        in_slope = out_slope;
        prev_t = t;
        prev_v = v;
    }
    out.push(scalar_keyframe(prev_t, prev_v, in_slope, 0.0));
    out
}

fn float_curve_entry(m_curve: Vec<Value>, attribute: &str, path: &str) -> Value {
    map! {
        "curve" => map! {
            "m_Curve" => Value::Array(m_curve),
            "m_PreInfinity" => 2,
            "m_PostInfinity" => 2,
            "m_RotationOrder" => ROTATION_ORDER,
        },
        "attribute" => attribute,
        "path" => path,
        "classID" => CLASS_ID_SMR,
        "script" => map!{"m_FileID" => 0, "m_PathID" => 0},
        "flags" => 0,
    }
}

fn curve_entry(m_curve: Vec<Value>, path: &str) -> Value {
    map! {
        "curve" => map! {
            "m_Curve" => Value::Array(m_curve),
            "m_PreInfinity" => 2,
            "m_PostInfinity" => 2,
            "m_RotationOrder" => ROTATION_ORDER,
        },
        "path" => path,
    }
}

fn conv_translation(v: &[f64]) -> Vec<f64> {
    let g = |i: usize| v.get(i).copied().unwrap_or(0.0);
    vec![-g(0), g(1), g(2)]
}
fn conv_scale(v: &[f64]) -> Vec<f64> {
    let g = |i: usize| v.get(i).copied().unwrap_or(1.0);
    vec![g(0), g(1), g(2)]
}
fn conv_rotation(q: &[f64]) -> Vec<f64> {
    let g = |i: usize, d: f64| q.get(i).copied().unwrap_or(d);
    vec![g(0, 0.0), -g(1, 0.0), -g(2, 0.0), g(3, 1.0)]
}

fn flush_cubic_tangent_neg_zero(raw: &mut [Vec<f64>]) {
    for (i, sample) in raw.iter_mut().enumerate() {
        if i % 3 == 1 {
            continue;
        }
        for x in sample.iter_mut() {
            if *x == 0.0 {
                *x = 0.0;
            }
        }
    }
}

fn primitive_cluster_key(prim: &serde_json::Value) -> Vec<i64> {
    let attrs = &prim["attributes"];
    let ji = |v: &serde_json::Value, k: &str| -> i64 { v[k].as_i64().unwrap_or(-1) };
    let mut key: Vec<i64> = vec![
        ji(attrs, "POSITION"),
        ji(attrs, "NORMAL"),
        ji(attrs, "TANGENT"),
        -100,
    ];
    for ui in 0..8 {
        match attrs.get(format!("TEXCOORD_{ui}")) {
            Some(a) if a.as_i64().is_some() => key.push(a.as_i64().unwrap()),
            _ => break,
        }
    }
    key.push(-101);
    key.push(ji(attrs, "COLOR_0"));

    key.push(-102);
    if let Some(targets) = prim["targets"].as_array() {
        for t in targets {
            key.push(ji(t, "POSITION"));
            key.push(ji(t, "NORMAL"));
            key.push(ji(t, "TANGENT"));
        }
    }
    key
}

fn blendshape_curve_paths(
    gltf: &serde_json::Value,
    mesh_idx: usize,
    node_path: &str,
) -> Vec<String> {
    let mesh = &gltf["meshes"][mesh_idx];
    let prims = match mesh["primitives"].as_array() {
        Some(p) if !p.is_empty() => p,
        _ => return vec![node_path.to_string()],
    };

    let mut clusters: Vec<(Vec<i64>, bool)> = Vec::new();
    for p in prims {
        let key = primitive_cluster_key(p);
        let has_morph = p["targets"]
            .as_array()
            .map(|a| !a.is_empty())
            .unwrap_or(false);
        match clusters.iter_mut().find(|(k, _)| *k == key) {
            Some((_, hm)) => {
                *hm = *hm || has_morph;
            }
            None => clusters.push((key, has_morph)),
        }
    }

    if clusters.len() <= 1 {
        return vec![node_path.to_string()];
    }
    let mesh_name = mesh["name"].as_str().filter(|s| !s.is_empty());
    let child_base: String = mesh_name
        .map(|s| s.to_string())
        .unwrap_or_else(|| "Primitive".to_string());
    let mut paths: Vec<String> = Vec::new();
    for (ci, (_, has_morph)) in clusters.iter().enumerate() {
        if ci == 0 {
            paths.push(node_path.to_string());
        } else if *has_morph {
            paths.push(format!("{node_path}/{child_base}_{ci}"));
        }
    }
    paths
}

pub fn build_animation_clips_from_gltf(
    gltf: &serde_json::Value,
    buffers: &[Vec<u8>],
) -> Vec<Value> {
    let animations = match gltf["animations"].as_array() {
        Some(a) if !a.is_empty() => a.clone(),
        _ => return Vec::new(),
    };
    let (names, parent) = glb::node_names_and_parents_from_json(gltf);

    let mut clips: Vec<Value> = Vec::new();
    for (ai, anim) in animations.iter().enumerate() {
        let mut rot: Vec<Value> = Vec::new();
        let mut pos: Vec<Value> = Vec::new();
        let mut scl: Vec<Value> = Vec::new();
        let mut float_curves: Vec<Value> = Vec::new();

        let channels = anim["channels"].as_array().cloned().unwrap_or_default();
        let samplers = anim["samplers"].as_array().cloned().unwrap_or_default();

        let mut seen_targets: std::collections::HashSet<(i64, String)> =
            std::collections::HashSet::new();
        for ch in &channels {
            let sampler_idx = ch["sampler"].as_u64().expect("channel sampler") as usize;
            let sampler = match samplers.get(sampler_idx) {
                Some(s) => s,
                None => continue,
            };
            let target = &ch["target"];
            let node = match target["node"].as_i64() {
                Some(n) if n >= 0 => n as usize,
                _ => continue,
            };
            let tpath_key = target["path"].as_str().unwrap_or("").to_string();
            if !seen_targets.insert((node as i64, tpath_key)) {
                continue;
            }
            let path = glb::animation_path(node, &names, &parent);
            let input_idx = sampler["input"].as_u64().expect("sampler input") as usize;
            let times: Vec<f64> = glb::read_accessor_with_buffers(gltf, buffers, input_idx)
                .into_iter()
                .map(|t| t[0])
                .collect();
            let interp = sampler["interpolation"]
                .as_str()
                .unwrap_or(INTERP_LINEAR)
                .to_string();
            let output_idx = sampler["output"].as_u64().expect("sampler output") as usize;
            let tpath = target["path"].as_str().unwrap_or("");
            if tpath == PATH_ROTATION {
                let mut raw = glb::read_accessor_with_buffers(gltf, buffers, output_idx);
                if interp == INTERP_CUBICSPLINE {
                    flush_cubic_tangent_neg_zero(&mut raw);
                }
                let vals: Vec<Vec<f64>> = raw.iter().map(|q| conv_rotation(q)).collect();
                rot.push(curve_entry(
                    bake_vec_curve(&times, &vals, &interp, true),
                    &path,
                ));
            } else if tpath == PATH_TRANSLATION {
                let mut raw = glb::read_accessor_with_buffers(gltf, buffers, output_idx);
                if interp == INTERP_CUBICSPLINE {
                    flush_cubic_tangent_neg_zero(&mut raw);
                }
                let vals: Vec<Vec<f64>> = raw.iter().map(|v| conv_translation(v)).collect();
                pos.push(curve_entry(
                    bake_vec_curve(&times, &vals, &interp, false),
                    &path,
                ));
            } else if tpath == PATH_SCALE {
                let mut raw = glb::read_accessor_with_buffers(gltf, buffers, output_idx);
                if interp == INTERP_CUBICSPLINE {
                    flush_cubic_tangent_neg_zero(&mut raw);
                }
                let vals: Vec<Vec<f64>> = raw.iter().map(|v| conv_scale(v)).collect();
                scl.push(curve_entry(
                    bake_vec_curve(&times, &vals, &interp, false),
                    &path,
                ));
            } else if tpath == PATH_WEIGHTS {
                let mesh_idx = match gltf["nodes"][node]["mesh"].as_i64() {
                    Some(m) if m >= 0 => m as usize,
                    _ => continue,
                };
                let mesh = &gltf["meshes"][mesh_idx];
                let n_targets = mesh["primitives"][0]["targets"]
                    .as_array()
                    .map(|a| a.len())
                    .unwrap_or(0);
                if n_targets == 0 {
                    continue;
                }

                let target_names: Vec<String> = (0..n_targets)
                    .map(|t| {
                        mesh["extras"]["targetNames"][t]
                            .as_str()
                            .filter(|s| !s.is_empty())
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| format!("{t}"))
                    })
                    .collect();
                let raw = glb::read_accessor_with_buffers(gltf, buffers, output_idx);
                let flat: Vec<f64> = raw.into_iter().map(|v| v[0]).collect();
                if flat.len() != times.len() * n_targets {
                    continue;
                }

                let curve_paths = blendshape_curve_paths(gltf, mesh_idx, &path);
                for cpath in &curve_paths {
                    for ti in 0..n_targets {
                        let values_t: Vec<f64> =
                            (0..times.len()).map(|i| flat[i * n_targets + ti]).collect();
                        let kf = bake_scalar_curve(&times, &values_t, &interp);
                        let attribute = format!("blendShape.{}", target_names[ti]);
                        float_curves.push(float_curve_entry(kf, &attribute, cpath));
                    }
                }
            }
        }

        let name = anim["name"]
            .as_str()
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("Clip_{ai}"));

        clips.push(map! {
            "m_Name" => name,
            "m_Legacy" => true,
            "m_Compressed" => false,
            "m_UseHighQualityCurve" => true,
            "m_RotationCurves" => Value::Array(rot),
            "m_CompressedRotationCurves" => arr![],
            "m_EulerCurves" => arr![],
            "m_PositionCurves" => Value::Array(pos),
            "m_ScaleCurves" => Value::Array(scl),
            "m_FloatCurves" => Value::Array(float_curves),
            "m_PPtrCurves" => arr![],
            "m_SampleRate" => SAMPLE_RATE,
            "m_WrapMode" => WRAP_LOOP,
            "m_Bounds" => map! {
                "m_Center" => map!{"x" => 0.0, "y" => 0.0, "z" => 0.0},
                "m_Extent" => map!{"x" => 0.0, "y" => 0.0, "z" => 0.0},
            },
            "m_ClipBindingConstant" => map! {
                "genericBindings" => arr![],
                "pptrCurveMapping" => arr![],
            },
            "m_HasGenericRootTransform" => false,
            "m_HasMotionFloatCurves" => false,
            "m_Events" => arr![],
        });
    }
    clips
}

pub fn build_animation_component(go_pid: i64, clip_name_pids: &[(String, i64)]) -> Value {
    let first = clip_name_pids.first().map(|(_, p)| *p).unwrap_or(0);
    let mut slot_for: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    let mut animations: Vec<Value> = Vec::new();
    for (name, pid) in clip_name_pids {
        let pp = crate::value::pptr(0, *pid);
        if let Some(&idx) = slot_for.get(name.as_str()) {
            animations[idx] = pp;
        } else {
            slot_for.insert(name.as_str(), animations.len());
            animations.push(pp);
        }
    }
    map! {
        "m_GameObject" => crate::value::pptr(0, go_pid),
        "m_Enabled" => 1,
        "m_Animation" => crate::value::pptr(0, first),
        "m_Animations" => Value::Array(animations),
        "m_WrapMode" => 0,
        "m_PlayAutomatically" => true,
        "m_AnimatePhysics" => false,
        "m_UpdateMode" => 0,
        "m_CullingType" => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn animation_component_shape() {
        let c = build_animation_component(7, &[("a".into(), 10), ("b".into(), 11)]);
        assert_eq!(
            c.get("m_Animation")
                .unwrap()
                .get("m_PathID")
                .unwrap()
                .as_i64(),
            Some(10)
        );
        assert_eq!(c.get("m_Animations").unwrap().as_array().unwrap().len(), 2);
        assert_eq!(c.get("m_PlayAutomatically").unwrap().as_bool(), Some(true));
    }

    #[test]
    fn animation_component_dedupes_by_name() {
        let c = build_animation_component(
            7,
            &[
                ("door_o".into(), 10),
                ("door_o".into(), 11),
                ("col_c".into(), 12),
            ],
        );
        let arr = c.get("m_Animations").unwrap().as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0].get("m_PathID").unwrap().as_i64(), Some(11));
        assert_eq!(arr[1].get("m_PathID").unwrap().as_i64(), Some(12));
        assert_eq!(
            c.get("m_Animation")
                .unwrap()
                .get("m_PathID")
                .unwrap()
                .as_i64(),
            Some(10)
        );
    }

    #[test]
    fn empty_animations_component() {
        let c = build_animation_component(1, &[]);
        assert_eq!(
            c.get("m_Animation")
                .unwrap()
                .get("m_PathID")
                .unwrap()
                .as_i64(),
            Some(0)
        );
    }
}
