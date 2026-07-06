use super::{
    ATTR_POSITION, ATTR_ROTATION, ATTR_SCALE, INTERP_CUBICSPLINE, INTERP_LINEAR, INTERP_STEP,
};
use crate::animation::glb;
use crate::value::Value;

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

#[derive(Clone, Copy)]
pub(super) struct Key {
    pub(super) time: f64,
    pub(super) value: f64,
    pub(super) slope: f64,
    pub(super) a: f64,
    pub(super) b: f64,
}

pub(super) fn bake_scalar_keys(times: &[f64], scalar_vals: &[f64], interp: &str) -> Vec<Key> {
    let n = times.len();
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![Key {
            time: times[0],
            value: scalar_vals[0],
            slope: 0.0,
            a: 0.0,
            b: 0.0,
        }];
    }
    let mut out = Vec::with_capacity(n);
    if interp == INTERP_STEP {
        for i in 0..n {
            out.push(Key {
                time: times[i],
                value: scalar_vals[i],
                slope: 0.0,
                a: 0.0,
                b: 0.0,
            });
        }
        return out;
    }

    for i in 0..n {
        let v = scalar_vals[i];
        let (slope, a, b) = if i < n - 1 {
            let dt = times[i + 1] - times[i];
            if dt > 0.0 {
                let dt = dt as f32;
                let dv = (scalar_vals[i + 1] - v) as f32;
                let sec = dv / dt;
                let inv = 1.0f32 / dt;
                let inv3 = inv * inv * inv;
                let a = (2.0f32 * (sec * dt - dv)) * inv3;

                let t1 = sec * dt;
                let bq = (((3.0f32 * dv - t1) - t1) - t1) * (inv * inv);

                let b = if bq == 0.0 { 0.0f32 } else { bq };
                (sec as f64, a as f64, b as f64)
            } else {
                (0.0, 0.0, 0.0)
            }
        } else {
            (0.0, 0.0, 0.0)
        };
        out.push(Key {
            time: times[i],
            value: v,
            slope,
            a,
            b,
        });
    }
    out
}

pub(super) fn encode_streamed_clip(curves: &[(i64, Vec<Key>)]) -> Vec<Value> {
    let mut times: Vec<f64> = Vec::new();
    for (_, keys) in curves.iter() {
        for k in keys.iter() {
            times.push(k.time);
        }
    }
    if times.is_empty() {
        return Vec::new();
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    times.dedup();

    let neg = f32::from_bits(0xFF7F_FFFF);
    let pos = f32::INFINITY;

    let find_time_index = |t: f64| -> usize {
        times
            .iter()
            .position(|&x| x.to_bits() == t.to_bits())
            .or_else(|| times.iter().position(|&x| x == t))
            .expect("time present")
    };
    let mut by_time: Vec<Vec<(i64, [f32; 4])>> = vec![Vec::new(); times.len()];
    for (ci, keys) in curves.iter() {
        for k in keys.iter() {
            let ti = find_time_index(k.time);
            by_time[ti].push((
                *ci,
                [k.a as f32, k.b as f32, k.slope as f32, k.value as f32],
            ));
        }
    }

    for frame in by_time.iter_mut() {
        frame.sort_by_key(|(ci, _)| *ci);
    }

    let mut buf: Vec<u8> = Vec::new();
    let emit_frame = |buf: &mut Vec<u8>, t: f32, keymap: &[(i64, [f32; 4])]| {
        buf.extend_from_slice(&t.to_le_bytes());
        buf.extend_from_slice(&(keymap.len() as i32).to_le_bytes());
        for (ci, coeffs) in keymap.iter() {
            buf.extend_from_slice(&(*ci as i32).to_le_bytes());
            for c in coeffs.iter() {
                buf.extend_from_slice(&c.to_le_bytes());
            }
        }
    };

    let mut lead: Vec<(i64, [f32; 4])> = curves
        .iter()
        .filter_map(|(ci, keys)| keys.first().map(|k| (*ci, [0.0, 0.0, 0.0, k.value as f32])))
        .collect();
    lead.sort_by_key(|(ci, _)| *ci);
    emit_frame(&mut buf, neg, &lead);
    for ti in 0..times.len() {
        let t = times[ti] as f32;
        emit_frame(&mut buf, t, &by_time[ti]);
    }
    emit_frame(&mut buf, pos, &[]);

    let mut out: Vec<Value> = Vec::with_capacity(buf.len() / 4);
    let mut i = 0;
    while i + 4 <= buf.len() {
        let u = u32::from_le_bytes([buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]);
        out.push(Value::Int(u as i64));
        i += 4;
    }
    out
}

pub(super) fn gather_clip_curves(
    gltf: &serde_json::Value,
    buffers: &[Vec<u8>],
    anim: &serde_json::Value,
    names: &[String],
    parent: &[i64],
) -> (Vec<(String, i64, bool)>, Vec<(i64, Vec<Key>)>) {
    let channels = anim["channels"].as_array().cloned().unwrap_or_default();
    let samplers = anim["samplers"].as_array().cloned().unwrap_or_default();

    let mut node_order: Vec<usize> = Vec::new();

    let mut per_node: Vec<(usize, Vec<(String, usize, String)>)> = Vec::new();

    let node_pos = |po: &Vec<(usize, Vec<(String, usize, String)>)>, n: usize| -> Option<usize> {
        po.iter().position(|(k, _)| *k == n)
    };

    for ch in &channels {
        let tgt = &ch["target"];
        let node = match tgt["node"].as_i64() {
            Some(n) if n >= 0 => n as usize,
            _ => continue,
        };
        let sampler_idx = ch["sampler"].as_u64().expect("channel sampler") as usize;
        let interp = samplers[sampler_idx]["interpolation"]
            .as_str()
            .unwrap_or(INTERP_LINEAR)
            .to_string();
        let path = tgt["path"].as_str().unwrap_or("").to_string();
        if node_pos(&per_node, node).is_none() {
            node_order.push(node);
            per_node.push((node, Vec::new()));
        }
        let idx = node_pos(&per_node, node).unwrap();

        if !per_node[idx].1.iter().any(|(p, _, _)| *p == path) {
            per_node[idx].1.push((path, sampler_idx, interp));
        }
    }

    let get_chan = |node: usize, gpath: &str| -> Option<(usize, String)> {
        let idx = node_pos(&per_node, node)?;
        per_node[idx]
            .1
            .iter()
            .find(|(p, _, _)| p == gpath)
            .map(|(_, s, i)| (*s, i.clone()))
    };

    let mut ordered_bindings: Vec<(String, i64, bool)> = Vec::new();
    let mut scalar_curves: Vec<(i64, Vec<Key>)> = Vec::new();
    let mut gidx: i64 = 0;

    let attrs: [(i64, &str, usize, fn(&[f64]) -> Vec<f64>); 3] = [
        (ATTR_POSITION, "translation", 3, conv_translation),
        (ATTR_ROTATION, "rotation", 4, conv_rotation),
        (ATTR_SCALE, "scale", 3, conv_scale),
    ];

    for (attr, gpath, dim, conv) in attrs.iter() {
        for &node_idx in node_order.iter() {
            let (sampler_idx, interp) = match get_chan(node_idx, gpath) {
                Some(v) => v,
                None => continue,
            };
            let path = glb::animation_path(node_idx, names, parent);
            let sampler = &samplers[sampler_idx];
            let input_idx = sampler["input"].as_u64().expect("sampler input") as usize;
            let times: Vec<f64> = glb::read_accessor_with_buffers(gltf, buffers, input_idx)
                .into_iter()
                .map(|t| t[0])
                .collect();
            let output_idx = sampler["output"].as_u64().expect("sampler output") as usize;
            let mut raw = glb::read_accessor_with_buffers(gltf, buffers, output_idx);
            if interp == INTERP_CUBICSPLINE {
                let n = times.len();
                let mut v = Vec::with_capacity(n);
                for i in 0..n {
                    v.push(raw[i * 3 + 1].clone());
                }
                raw = v;
            }
            let mut vals: Vec<Vec<f64>> = raw.iter().map(|v| conv(v)).collect();
            let mut times = times;
            if interp == INTERP_LINEAR {
                let mut kt: Vec<f64> = Vec::with_capacity(times.len());
                let mut kv: Vec<Vec<f64>> = Vec::with_capacity(vals.len());
                for (i, &t) in times.iter().enumerate() {
                    if i > 0 && *kt.last().unwrap() >= t {
                        continue;
                    }
                    kt.push(t);
                    kv.push(vals[i].clone());
                }
                times = kt;
                vals = kv;
                if *attr == ATTR_ROTATION {
                    for i in 1..vals.len() {
                        let dot: f32 = (vals[i - 1][0] as f32) * (vals[i][0] as f32)
                            + (vals[i - 1][1] as f32) * (vals[i][1] as f32)
                            + ((vals[i - 1][2] as f32) * (vals[i][2] as f32)
                                + (vals[i - 1][3] as f32) * (vals[i][3] as f32));
                        if dot < 0.0 {
                            for c in vals[i].iter_mut() {
                                *c = -*c;
                            }
                        }
                    }
                }
            }
            ordered_bindings.push((path, *attr, interp == INTERP_STEP));
            for comp in 0..*dim {
                let comp_vals: Vec<f64> = vals.iter().map(|v| v[comp]).collect();
                scalar_curves.push((gidx, bake_scalar_keys(&times, &comp_vals, &interp)));
                gidx += 1;
            }
        }
    }
    (ordered_bindings, scalar_curves)
}

pub const CONST_CURVE_VALUE_TOL: f32 = 9.536_743e-7;

pub const CONST_CURVE_SLOPE_TOL: f32 = 8.940705e-07;

pub(super) fn classify_constant(
    scalar_curves: &[(i64, Vec<Key>)],
    bindings: &[(String, i64, bool)],
    value_tol: f32,
) -> Vec<bool> {
    let curve_const = |keys: &[Key]| -> bool {
        match keys.first() {
            None => true,
            Some(k0) => {
                let v0 = k0.value as f32;
                keys.iter().all(|k| {
                    ((k.value as f32) - v0).abs() < value_tol
                        && (k.slope as f32).abs() < CONST_CURVE_SLOPE_TOL
                })
            }
        }
    };
    let mut class = vec![false; scalar_curves.len()];
    let mut ci = 0usize;
    for (_, attr, is_step) in bindings.iter() {
        let dim = if *attr == ATTR_ROTATION { 4 } else { 3 };
        let all_const = !*is_step && (ci..ci + dim).all(|i| curve_const(&scalar_curves[i].1));
        for flag in class.iter_mut().skip(ci).take(dim) {
            *flag = all_const;
        }
        ci += dim;
    }
    class
}

pub(super) fn partition_curves(
    scalar_curves: &[(i64, Vec<Key>)],
    class: &[bool],
) -> (Vec<(i64, Vec<Key>)>, Vec<f64>, i64) {
    let mut streamed: Vec<(i64, Vec<Key>)> = Vec::new();
    let mut constant: Vec<f64> = Vec::new();
    let mut new_i: i64 = 0;
    for (idx, (_, keys)) in scalar_curves.iter().enumerate() {
        if class[idx] {
            let v = keys.first().map(|k| k.value).unwrap_or(0.0);
            constant.push(v);
        } else {
            streamed.push((new_i, keys.clone()));
            new_i += 1;
        }
    }
    let n = streamed.len() as i64;
    (streamed, constant, n)
}
