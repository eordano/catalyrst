use crate::value::Value;

const FMT_FLOAT32: i64 = 0;
const FMT_UNORM16: i64 = 4;
const FMT_UINT32: i64 = 10;

#[inline]
fn fmt_bytes(fmt: i64) -> i64 {
    match fmt {
        FMT_UNORM16 => 2,
        _ => 4,
    }
}

const CH_POSITION: usize = 0;
const CH_NORMAL: usize = 1;
const CH_TANGENT: usize = 2;
const CH_COLOR: usize = 3;
const CH_TEXCOORD0: usize = 4;
const CH_BLENDWEIGHT: usize = 12;
const CH_BLENDINDICES: usize = 13;
const NUM_CHANNELS: usize = 14;

pub struct MeshAttributes<'a> {
    pub positions: &'a [[f64; 3]],
    pub normals: Option<&'a [[f64; 3]]>,
    pub tangents: Option<&'a [[f64; 4]]>,
    pub colors: Option<&'a [[f64; 4]]>,
    pub uv_sets: &'a [Vec<[f64; 2]>],
    pub weights: Option<&'a [[f64; 4]]>,
    pub joints: Option<&'a [[u32; 4]]>,

    pub color_unorm16: bool,
}

fn ch(stream: i64, offset: i64, fmt: i64, dim: i64) -> Value {
    map! {
        "stream" => stream,
        "offset" => offset,
        "format" => fmt,
        "dimension" => dim,
    }
}

#[inline]
fn ch_field(c: &Value, key: &str) -> i64 {
    c.get(key).and_then(|v| v.as_i64()).unwrap_or(0)
}

fn build_channels(attrs: &MeshAttributes) -> Vec<Value> {
    let has_normals = attrs.normals.is_some();
    let has_tangents = attrs.tangents.is_some();
    let has_colors = attrs.colors.is_some();
    let n_uv = attrs.uv_sets.len();
    let has_bones = attrs.weights.is_some() && attrs.joints.is_some();

    let mut channels: Vec<Value> = (0..NUM_CHANNELS).map(|_| ch(0, 0, 0, 0)).collect();

    channels[CH_POSITION] = ch(0, 0, FMT_FLOAT32, 3);
    let mut off = 12i64;
    if has_normals {
        channels[CH_NORMAL] = ch(0, off, FMT_FLOAT32, 3);
        off += 12;
    }
    if has_tangents {
        channels[CH_TANGENT] = ch(0, off, FMT_FLOAT32, 4);
    }

    let mut stream = 1i64;

    let color_fmt = if attrs.color_unorm16 {
        FMT_UNORM16
    } else {
        FMT_FLOAT32
    };
    let color_bytes = fmt_bytes(color_fmt) * 4;

    if has_bones {
        let mut attr_off = 0i64;
        if has_colors {
            channels[CH_COLOR] = ch(stream, attr_off, color_fmt, 4);
            attr_off += color_bytes;
        }
        for i in 0..n_uv {
            channels[CH_TEXCOORD0 + i] = ch(stream, attr_off, FMT_FLOAT32, 2);
            attr_off += 8;
        }
        if has_colors || n_uv > 0 {
            stream += 1;
        }
        channels[CH_BLENDWEIGHT] = ch(stream, 0, FMT_FLOAT32, 4);
        channels[CH_BLENDINDICES] = ch(stream, 16, FMT_UINT32, 4);
    } else {
        if has_colors {
            channels[CH_COLOR] = ch(stream, 0, color_fmt, 4);
            stream += 1;
        }
        for i in 0..n_uv {
            channels[CH_TEXCOORD0 + i] = ch(stream, (i as i64) * 8, FMT_FLOAT32, 2);
        }
    }

    channels
}

pub fn normalize_color(raw: &[f64], component_type: i32, dim: usize) -> [f64; 4] {
    let vals: Vec<f64> = match component_type {
        5126 => raw.to_vec(),
        5121 => raw.iter().map(|v| v / 255.0).collect(),
        5123 => raw.iter().map(|v| v / 65535.0).collect(),
        _ => panic!("unsupported color componentType {component_type}"),
    };
    if dim == 3 {
        [vals[0], vals[1], vals[2], 1.0]
    } else {
        [vals[0], vals[1], vals[2], vals[3]]
    }
}

pub fn normalize_weights(raw: &[f64], component_type: i32) -> [f64; 4] {
    let f = |v: f64| -> f64 {
        match component_type {
            5126 => v,
            5121 => v / 255.0,
            5123 => v / 65535.0,
            _ => panic!("unsupported weights componentType {component_type}"),
        }
    };
    [f(raw[0]), f(raw[1]), f(raw[2]), f(raw[3])]
}

pub fn sort_and_normalize_bones(weights: [f64; 4], joints: [u32; 4]) -> ([f64; 4], [u32; 4]) {
    let mut w = [
        weights[0] as f32,
        weights[1] as f32,
        weights[2] as f32,
        weights[3] as f32,
    ];
    let mut j = joints;

    let already_sorted = (0..3).all(|i| w[i] >= w[i + 1]);
    if !already_sorted {
        for i in 0..4 {
            let mut maxv = w[i];
            let mut maxi = i;
            for k in (i + 1)..4 {
                if w[k] > maxv {
                    maxv = w[k];
                    maxi = k;
                }
            }
            if maxi > i {
                w.swap(i, maxi);
                j.swap(i, maxi);
            }
        }
    }

    let weight_sum_f64: f64 = (w[0] as f64) + (w[1] as f64) + (w[2] as f64) + (w[3] as f64);
    if weight_sum_f64 > 0.0 {
        let mult = 1.0f32 / (weight_sum_f64 as f32);
        for x in w.iter_mut() {
            *x *= mult;
        }
    }

    ([w[0] as f64, w[1] as f64, w[2] as f64, w[3] as f64], j)
}

pub fn vertex_buffer(attrs: &MeshAttributes) -> (Vec<u8>, Vec<Value>) {
    let channels = build_channels(attrs);
    let n = attrs.positions.len();

    let mut by_stream: std::collections::BTreeMap<i64, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (ci, c) in channels.iter().enumerate() {
        if ch_field(c, "dimension") > 0 {
            by_stream.entry(ch_field(c, "stream")).or_default().push(ci);
        }
    }

    let mut out: Vec<u8> = Vec::new();
    for (si, (_s, cis)) in by_stream.iter().enumerate() {
        if si > 0 {
            while !out.len().is_multiple_of(16) {
                out.push(0);
            }
        }
        let stride_raw = cis
            .iter()
            .map(|&ci| {
                ch_field(&channels[ci], "offset")
                    + ch_field(&channels[ci], "dimension")
                        * fmt_bytes(ch_field(&channels[ci], "format"))
            })
            .max()
            .unwrap_or(0);
        let stride = ((stride_raw + 3) & !3) as usize;
        for v in 0..n {
            let mut row = vec![0u8; stride];
            for &ci in cis.iter() {
                let off = ch_field(&channels[ci], "offset") as usize;
                if ci == CH_POSITION {
                    let p = attrs.positions[v];
                    pack_f32(&mut row, off, &[p[0], p[1], p[2]]);
                } else if ci == CH_NORMAL {
                    let nrm = attrs
                        .normals
                        .unwrap()
                        .get(v)
                        .copied()
                        .unwrap_or([0.0, 0.0, 0.0]);
                    pack_f32(&mut row, off, &[nrm[0], nrm[1], nrm[2]]);
                } else if ci == CH_TANGENT {
                    let t = attrs
                        .tangents
                        .unwrap()
                        .get(v)
                        .copied()
                        .unwrap_or([0.0, 0.0, 0.0, 0.0]);
                    pack_f32(&mut row, off, &[t[0], t[1], t[2], t[3]]);
                } else if ci == CH_COLOR {
                    let c = attrs
                        .colors
                        .unwrap()
                        .get(v)
                        .copied()
                        .unwrap_or([1.0, 1.0, 1.0, 1.0]);
                    if attrs.color_unorm16 {
                        pack_unorm16(&mut row, off, &[c[0], c[1], c[2], c[3]]);
                    } else {
                        pack_f32(&mut row, off, &[c[0], c[1], c[2], c[3]]);
                    }
                } else if ci >= CH_TEXCOORD0 && ci < CH_TEXCOORD0 + attrs.uv_sets.len() {
                    let uv = attrs.uv_sets[ci - CH_TEXCOORD0]
                        .get(v)
                        .copied()
                        .unwrap_or([0.0, 0.0]);
                    pack_f32(&mut row, off, &[uv[0], uv[1]]);
                } else if ci == CH_BLENDWEIGHT {
                    let w = attrs
                        .weights
                        .unwrap()
                        .get(v)
                        .copied()
                        .unwrap_or([0.0, 0.0, 0.0, 0.0]);
                    pack_f32(&mut row, off, &[w[0], w[1], w[2], w[3]]);
                } else if ci == CH_BLENDINDICES {
                    let j = attrs
                        .joints
                        .unwrap()
                        .get(v)
                        .copied()
                        .unwrap_or([0, 0, 0, 0]);
                    pack_u32(&mut row, off, &[j[0], j[1], j[2], j[3]]);
                }
            }
            out.extend_from_slice(&row);
        }
    }
    (out, channels)
}

#[inline]
fn pack_f32(row: &mut [u8], off: usize, vals: &[f64]) {
    for (i, v) in vals.iter().enumerate() {
        let b = (*v as f32).to_le_bytes();
        row[off + i * 4..off + i * 4 + 4].copy_from_slice(&b);
    }
}

#[inline]
fn pack_unorm16(row: &mut [u8], off: usize, vals: &[f64]) {
    for (i, v) in vals.iter().enumerate() {
        let clamped = v.clamp(0.0, 1.0);
        let q = (clamped * 65535.0 + 0.5).floor() as u16;
        row[off + i * 2..off + i * 2 + 2].copy_from_slice(&q.to_le_bytes());
    }
}

#[inline]
fn pack_u32(row: &mut [u8], off: usize, vals: &[u32]) {
    for (i, v) in vals.iter().enumerate() {
        let b = v.to_le_bytes();
        row[off + i * 4..off + i * 4 + 4].copy_from_slice(&b);
    }
}

pub fn convert_bind_matrix(m: [f64; 16]) -> [f64; 16] {
    let mut r = m;
    r[1] = -r[1];
    r[2] = -r[2];
    r[4] = -r[4];
    r[8] = -r[8];
    r[12] = -r[12];
    r
}

fn mul_point3x4(cols: &[f64; 16], x: f64, y: f64, z: f64) -> [f64; 3] {
    let (xf, yf, zf) = (x as f32, y as f32, z as f32);
    let mut out = [0.0f64; 3];
    for r in 0..3 {
        let m0 = cols[r] as f32;
        let m1 = cols[4 + r] as f32;
        let m2 = cols[8 + r] as f32;
        let m3 = cols[12 + r] as f32;
        let mut a = m0 * xf;
        a += m1 * yf;
        a += m2 * zf;
        a += m3;
        out[r] = a as f64;
    }
    out
}

pub fn compute_bones_aabb(
    positions: &[[f64; 3]],
    weights: &[[f64; 4]],
    joints: &[[u32; 4]],
    bind_poses: &[[f64; 16]],
    morph_targets: &[crate::scene::MorphTarget],
) -> Value {
    let nb = bind_poses.len();
    let inf = f64::INFINITY;
    let mut mins = vec![[inf, inf, inf]; nb];
    let mut maxs = vec![[-inf, -inf, -inf]; nb];

    for vi in 0..positions.len() {
        let w = weights[vi];
        let j = joints[vi];
        for k in 0..4 {
            if w[k] > 0.0 {
                let b = j[k] as usize;
                if b >= nb {
                    continue;
                }
                let base = positions[vi];
                let mut mn = base;
                let mut mx = base;
                for tgt in morph_targets {
                    if vi >= tgt.positions.len() {
                        continue;
                    }
                    let d = tgt.positions[vi];
                    let m = [base[0] + d[0], base[1] + d[1], base[2] + d[2]];
                    for ax in 0..3 {
                        if m[ax] < mn[ax] {
                            mn[ax] = m[ax];
                        }
                        if m[ax] > mx[ax] {
                            mx[ax] = m[ax];
                        }
                    }
                }
                let c0 = mul_point3x4(&bind_poses[b], mn[0], mn[1], mn[2]);
                let c1 = mul_point3x4(&bind_poses[b], mx[0], mx[1], mx[2]);
                for ax in 0..3 {
                    let a = c0[ax].min(c1[ax]);
                    let z = c0[ax].max(c1[ax]);
                    if a < mins[b][ax] {
                        mins[b][ax] = a;
                    }
                    if z > maxs[b][ax] {
                        maxs[b][ax] = z;
                    }
                }
            }
        }
    }
    let arr: Vec<Value> = (0..nb)
        .map(|b| {
            let mn = mins[b];
            let mx = maxs[b];
            crate::map! {
                "m_Min" => crate::map!{"x" => mn[0], "y" => mn[1], "z" => mn[2]},
                "m_Max" => crate::map!{"x" => mx[0], "y" => mx[1], "z" => mx[2]},
            }
        })
        .collect();
    Value::Array(arr)
}

pub fn bind_pose_tree(matrix_cols: [f64; 16]) -> Value {
    let mut out = Value::map();
    for col in 0..4 {
        for row in 0..4 {
            out.insert(format!("e{row}{col}"), matrix_cols[col * 4 + row]);
        }
    }
    out
}

pub fn skinned_mesh_renderer_tree(
    base: &Value,
    go_pid: i64,
    mesh_pid: i64,
    material_pptrs: Vec<Value>,
    bone_pptrs: Vec<Value>,
    root_bone_pptr: Value,
    blend_shape_weights: &[f32],
) -> Value {
    let mut t = base.clone();
    t.insert("m_GameObject", map! {"m_FileID" => 0, "m_PathID" => go_pid});
    t.insert("m_Mesh", map! {"m_FileID" => 0, "m_PathID" => mesh_pid});
    t.insert("m_Materials", Value::Array(material_pptrs));
    t.insert("m_Bones", Value::Array(bone_pptrs));
    t.insert("m_RootBone", root_bone_pptr);
    t.insert("m_BoneNameHashes", Value::Array(vec![]));
    t.insert("m_RootBoneNameHash", 0);
    t.insert(
        "m_AABB",
        map! {
            "m_Center" => map!{"x" => 0.0, "y" => 0.0, "z" => 0.0},
            "m_Extent" => map!{"x" => 0.0, "y" => 0.0, "z" => 0.0},
        },
    );
    t.insert("m_DirtyAABB", true);
    t.insert("m_UpdateWhenOffscreen", true);

    let weights_arr: Vec<Value> = blend_shape_weights
        .iter()
        .map(|w| Value::from(*w as f64))
        .collect();
    t.insert("m_BlendShapeWeights", Value::Array(weights_arr));
    t
}

pub fn build_m_shapes(
    morph_targets: &[crate::scene::MorphTarget],
    morph_target_names: &[String],
) -> (Value, bool) {
    if morph_targets.is_empty() {
        let m = map! {
            "vertices" => Value::Array(vec![]),
            "shapes" => Value::Array(vec![]),
            "channels" => Value::Array(vec![]),
            "fullWeights" => Value::Array(vec![]),
        };
        return (m, false);
    }

    let mut vertices: Vec<Value> = Vec::new();
    let mut shapes: Vec<Value> = Vec::new();
    let mut channels: Vec<Value> = Vec::new();
    let mut full_weights: Vec<Value> = Vec::new();

    const EPS: f64 = 1.1920928955078125e-7;
    let nonzero =
        |v: [f64; 3]| -> bool { v[0].abs() > EPS || v[1].abs() > EPS || v[2].abs() > EPS };

    const KEEP_EPS: f64 = 1e-5;
    let l2 = |v: [f64; 3]| -> f64 { (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt() };

    for (ti, tgt) in morph_targets.iter().enumerate() {
        let n = tgt.positions.len();
        let first_vertex = vertices.len() as i64;
        let mut kept: i64 = 0;
        let mut any_kept_normal = false;

        let tan_present = tgt.tangents.is_some();
        for vi in 0..n {
            let p = tgt.positions[vi];
            let nrm_vec = tgt.normals.as_ref().map(|nv| nv[vi]).unwrap_or([0.0; 3]);
            let keep = l2(p) >= KEEP_EPS || l2(nrm_vec) >= KEEP_EPS;
            if !keep {
                continue;
            }
            let any_nrm = tgt
                .normals
                .as_ref()
                .map(|nv| nonzero(nv[vi]))
                .unwrap_or(false);

            let normal = nrm_vec;
            if any_nrm {
                any_kept_normal = true;
            }
            let tangent = if tan_present { normal } else { [0.0_f64; 3] };
            vertices.push(map! {
                "vertex" => map!{"x" => p[0], "y" => p[1], "z" => p[2]},
                "normal" => map!{"x" => normal[0], "y" => normal[1], "z" => normal[2]},
                "tangent" => map!{"x" => tangent[0], "y" => tangent[1], "z" => tangent[2]},
                "index" => Value::from(vi as i64),
            });
            kept += 1;
        }

        shapes.push(map! {
            "firstVertex" => first_vertex,
            "vertexCount" => kept,
            "hasNormals" => any_kept_normal,
            "hasTangents" => false,
        });

        let name = morph_target_names
            .get(ti)
            .cloned()
            .unwrap_or_else(|| format!("{ti}"));
        let hash = crate::hashes::crc32(name.as_bytes()) as i64;
        channels.push(map! {
            "name" => Value::from(name),
            "nameHash" => hash,
            "frameIndex" => Value::from(ti as i64),
            "frameCount" => 1,
        });

        full_weights.push(Value::from(1.0f64));
    }

    let kept_any = !vertices.is_empty();
    let m = map! {
        "vertices" => Value::Array(vertices),
        "shapes" => Value::Array(shapes),
        "channels" => Value::Array(channels),
        "fullWeights" => Value::Array(full_weights),
    };
    (m, kept_any)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chans(normals: bool, tan: bool, color: bool, nuv: usize, bones: bool) -> Vec<Value> {
        let positions = vec![[0.0, 0.0, 0.0]];
        let normals_v = vec![[0.0, 0.0, 1.0]];
        let tangents_v = vec![[1.0, 0.0, 0.0, 1.0]];
        let colors_v = vec![[1.0, 1.0, 1.0, 1.0]];
        let uv_sets: Vec<Vec<[f64; 2]>> = vec![vec![[0.0, 0.0]]; nuv];
        let weights_v = vec![[1.0, 0.0, 0.0, 0.0]];
        let joints_v = vec![[0u32, 0, 0, 0]];
        let attrs = MeshAttributes {
            positions: &positions,
            normals: if normals { Some(&normals_v) } else { None },
            tangents: if tan { Some(&tangents_v) } else { None },
            colors: if color { Some(&colors_v) } else { None },
            uv_sets: &uv_sets,
            weights: if bones { Some(&weights_v) } else { None },
            joints: if bones { Some(&joints_v) } else { None },
            color_unorm16: false,
        };
        build_channels(&attrs)
    }

    fn active(ch: &[Value]) -> std::collections::HashMap<usize, (i64, i64, i64, i64)> {
        let mut m = std::collections::HashMap::new();
        for (i, c) in ch.iter().enumerate() {
            let d = ch_field(c, "dimension");
            if d > 0 {
                m.insert(
                    i,
                    (
                        ch_field(c, "stream"),
                        ch_field(c, "offset"),
                        ch_field(c, "format"),
                        d,
                    ),
                );
            }
        }
        m
    }

    #[test]
    fn channel_layouts() {
        let c = active(&chans(true, false, false, 1, false));
        assert_eq!(c[&0], (0, 0, 0, 3));
        assert_eq!(c[&1], (0, 12, 0, 3));
        assert_eq!(c[&4], (1, 0, 0, 2));

        let c = active(&chans(true, true, false, 1, false));
        assert_eq!(c[&2], (0, 24, 0, 4));
        assert_eq!(c[&4], (1, 0, 0, 2));

        let c = active(&chans(true, false, true, 1, false));
        assert_eq!(c[&3], (1, 0, 0, 4));
        assert_eq!(c[&4], (2, 0, 0, 2));

        let c = active(&chans(true, false, true, 2, false));
        assert_eq!(c[&3], (1, 0, 0, 4));
        assert_eq!(c[&4], (2, 0, 0, 2));
        assert_eq!(c[&5], (2, 8, 0, 2));

        let c = active(&chans(true, false, false, 1, true));
        assert_eq!(c[&4], (1, 0, 0, 2));
        assert_eq!(c[&12], (2, 0, 0, 4));
        assert_eq!(c[&13], (2, 16, 10, 4));

        let c = active(&chans(true, false, false, 2, true));
        assert_eq!(c[&4], (1, 0, 0, 2));
        assert_eq!(c[&5], (1, 8, 0, 2));
        assert_eq!(c[&12], (2, 0, 0, 4));
        assert_eq!(c[&13], (2, 16, 10, 4));

        let c = active(&chans(true, false, true, 1, true));
        assert_eq!(c[&3], (1, 0, 0, 4));
        assert_eq!(c[&4], (1, 16, 0, 2));
        assert_eq!(c[&12], (2, 0, 0, 4));
        assert_eq!(c[&13], (2, 16, 10, 4));

        let c = active(&chans(true, false, true, 2, true));
        assert_eq!(c[&3], (1, 0, 0, 4));
        assert_eq!(c[&4], (1, 16, 0, 2));
        assert_eq!(c[&5], (1, 24, 0, 2));
        assert_eq!(c[&12], (2, 0, 0, 4));
        assert_eq!(c[&13], (2, 16, 10, 4));
    }

    #[test]
    fn color_encoding() {
        assert_eq!(
            normalize_color(&[255.0, 128.0, 0.0], 5121, 3),
            [1.0, 128.0 / 255.0, 0.0, 1.0]
        );
        assert_eq!(
            normalize_color(&[0.5, 0.5, 0.5], 5126, 3),
            [0.5, 0.5, 0.5, 1.0]
        );
        assert_eq!(
            normalize_color(&[10.0, 20.0, 30.0, 40.0], 5121, 4),
            [10.0 / 255.0, 20.0 / 255.0, 30.0 / 255.0, 40.0 / 255.0]
        );
    }

    #[test]
    fn bone_sort_normalize() {
        let (w, j) = sort_and_normalize_bones([0.1, 0.7, 0.2, 0.0], [5, 6, 7, 8]);
        assert_eq!(w[0], 0.699999988079071);
        assert_eq!(j[0], 6);
        assert_eq!(j, [6, 7, 5, 8]);
        assert!((w.iter().sum::<f64>() - 1.0).abs() < 1e-6);

        let (w, j) = sort_and_normalize_bones([0.6, 0.3, 0.1, 0.0], [1, 2, 3, 4]);
        assert_eq!(
            w,
            [
                0.6000000238418579,
                0.30000001192092896,
                0.10000000149011612,
                0.0
            ]
        );
        assert_eq!(j, [1, 2, 3, 4]);
    }

    fn le_f32(v: f64) -> [u8; 4] {
        (v as f32).to_le_bytes()
    }

    #[test]
    fn interleaved_buffer_skinned() {
        let positions = vec![[1.0, 2.0, 3.0]];
        let normals = vec![[0.0, 0.0, 1.0]];
        let uv_sets = vec![vec![[0.25, 0.75]]];
        let weights = vec![[1.0, 0.0, 0.0, 0.0]];
        let joints = vec![[7u32, 0, 0, 0]];
        let attrs = MeshAttributes {
            positions: &positions,
            normals: Some(&normals),
            tangents: None,
            colors: None,
            uv_sets: &uv_sets,
            weights: Some(&weights),
            joints: Some(&joints),
            color_unorm16: false,
        };
        let (buf, _ch) = vertex_buffer(&attrs);

        assert_eq!(buf.len(), 80);
        assert_eq!(&buf[0..4], &le_f32(1.0));
        assert_eq!(&buf[4..8], &le_f32(2.0));
        assert_eq!(&buf[8..12], &le_f32(3.0));
        assert_eq!(&buf[12..16], &le_f32(0.0));
        assert_eq!(&buf[16..20], &le_f32(0.0));
        assert_eq!(&buf[20..24], &le_f32(1.0));

        assert_eq!(&buf[32..36], &le_f32(0.25));
        assert_eq!(&buf[36..40], &le_f32(0.75));

        assert_eq!(&buf[48..52], &le_f32(1.0));
        assert_eq!(&buf[64..68], &7u32.to_le_bytes());
    }

    #[test]
    fn interleaved_buffer_color_skin() {
        let positions = vec![[1.0, 2.0, 3.0]];
        let normals = vec![[0.0, 0.0, 1.0]];
        let colors = vec![[0.1, 0.2, 0.3, 0.4]];
        let uv_sets = vec![vec![[0.5, 0.6]]];
        let weights = vec![[1.0, 0.0, 0.0, 0.0]];
        let joints = vec![[9u32, 0, 0, 0]];
        let attrs = MeshAttributes {
            positions: &positions,
            normals: Some(&normals),
            tangents: None,
            colors: Some(&colors),
            uv_sets: &uv_sets,
            weights: Some(&weights),
            joints: Some(&joints),
            color_unorm16: false,
        };
        let (buf, _ch) = vertex_buffer(&attrs);

        assert_eq!(buf.len(), 96);
        assert_eq!(&buf[32..36], &le_f32(0.1));
        assert_eq!(&buf[36..40], &le_f32(0.2));
        assert_eq!(&buf[40..44], &le_f32(0.3));
        assert_eq!(&buf[44..48], &le_f32(0.4));
        assert_eq!(&buf[48..52], &le_f32(0.5));
        assert_eq!(&buf[52..56], &le_f32(0.6));
        assert_eq!(&buf[80..84], &9u32.to_le_bytes());
    }

    #[test]
    fn bind_matrix_flip_and_tree() {
        let ident = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        assert_eq!(convert_bind_matrix(ident), ident);
        let m: [f64; 16] = std::array::from_fn(|i| i as f64);
        let f = convert_bind_matrix(m);
        assert_eq!(f[1], -1.0);
        assert_eq!(f[2], -2.0);
        assert_eq!(f[4], -4.0);
        assert_eq!(f[8], -8.0);
        assert_eq!(f[12], -12.0);
        let tt = bind_pose_tree(ident);
        assert_eq!(tt.get("e00").unwrap().as_f64(), Some(1.0));
        assert_eq!(tt.get("e11").unwrap().as_f64(), Some(1.0));
        assert_eq!(tt.get("e33").unwrap().as_f64(), Some(1.0));
        assert_eq!(tt.get("e01").unwrap().as_f64(), Some(0.0));
    }

    #[test]
    fn smr_builder_empty_hashes_aabb() {
        let base = map! {
            "m_Quality" => 0,
            "m_CastShadows" => 1,
            "m_GameObject" => Value::Null,
            "m_Mesh" => Value::Null,
            "m_Materials" => Value::Null,
            "m_Bones" => Value::Null,
            "m_RootBone" => Value::Null,
            "m_BoneNameHashes" => arr![1, 2],
            "m_RootBoneNameHash" => 99,
            "m_AABB" => Value::Null,
            "m_DirtyAABB" => false,
            "m_UpdateWhenOffscreen" => false,
        };
        let smr = skinned_mesh_renderer_tree(
            &base,
            10,
            20,
            vec![crate::value::pptr(0, 30)],
            vec![crate::value::pptr(0, 40)],
            crate::value::pptr(0, 40),
            &[],
        );
        assert_eq!(smr.get("m_Quality").unwrap().as_i64(), Some(0));
        assert_eq!(
            smr.get("m_BoneNameHashes")
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            0
        );
        assert_eq!(smr.get("m_RootBoneNameHash").unwrap().as_i64(), Some(0));
        let center = smr.get("m_AABB").unwrap().get("m_Center").unwrap();
        assert_eq!(center.get("x").unwrap().as_f64(), Some(0.0));
        assert_eq!(center.get("y").unwrap().as_f64(), Some(0.0));
        assert_eq!(center.get("z").unwrap().as_f64(), Some(0.0));
        assert_eq!(smr.get("m_DirtyAABB").unwrap().as_bool(), Some(true));
        assert_eq!(
            smr.get("m_UpdateWhenOffscreen").unwrap().as_bool(),
            Some(true)
        );
        assert_eq!(
            smr.get("m_Mesh").unwrap().get("m_PathID").unwrap().as_i64(),
            Some(20)
        );
    }
}
