use serde_json::Value as J;

fn comp_size(component_type: i64) -> usize {
    match component_type {
        5120 | 5121 => 1,
        5122 | 5123 => 2,
        5125 | 5126 => 4,
        _ => panic!("unsupported componentType {component_type}"),
    }
}

pub(super) fn type_dim(t: &str) -> usize {
    match t {
        "SCALAR" => 1,
        "VEC2" => 2,
        "VEC3" => 3,
        "VEC4" => 4,
        "MAT4" => 16,
        other => panic!("unsupported accessor type {other}"),
    }
}

pub(super) fn ji(v: &J, key: &str) -> Option<i64> {
    v.get(key).and_then(|x| x.as_i64())
}
pub(super) fn jf(v: &J, key: &str) -> Option<f64> {
    v.get(key).and_then(|x| x.as_f64())
}
pub(super) fn js<'a>(v: &'a J, key: &str) -> Option<&'a str> {
    v.get(key).and_then(|x| x.as_str())
}
pub(super) fn jarr<'a>(v: &'a J, key: &str) -> Option<&'a Vec<J>> {
    v.get(key).and_then(|x| x.as_array())
}

fn decode_component(buf: &[u8], off: usize, component_type: i64) -> f64 {
    if off + comp_size(component_type) > buf.len() {
        return 0.0;
    }
    match component_type {
        5120 => i8::from_le_bytes([buf[off]]) as f64,
        5121 => buf[off] as f64,
        5122 => i16::from_le_bytes([buf[off], buf[off + 1]]) as f64,
        5123 => u16::from_le_bytes([buf[off], buf[off + 1]]) as f64,
        5125 => u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]) as f64,
        5126 => f32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]) as f64,
        _ => panic!("unsupported componentType {component_type}"),
    }
}

fn apply_sparse(gltf: &J, buffers: &[Vec<u8>], acc: &J, dim: usize, out: &mut [Vec<f64>]) {
    let sp = match acc.get("sparse") {
        Some(s) => s,
        None => return,
    };
    let sp_count = ji(sp, "count").unwrap_or(0) as usize;
    if sp_count == 0 {
        return;
    }

    let Some(buffer_views) = jarr(gltf, "bufferViews") else {
        return;
    };

    let idx_obj = sp.get("indices").expect("sparse.indices");
    let idx_bv =
        &buffer_views[ji(idx_obj, "bufferView").expect("sparse.indices.bufferView") as usize];
    let idx_buf = &buffers[ji(idx_bv, "buffer").unwrap_or(0) as usize];
    let idx_ct = ji(idx_obj, "componentType").expect("sparse.indices.componentType");
    let idx_csize = comp_size(idx_ct);
    let idx_start =
        (ji(idx_bv, "byteOffset").unwrap_or(0) + ji(idx_obj, "byteOffset").unwrap_or(0)) as usize;

    let val_obj = sp.get("values").expect("sparse.values");
    let val_bv =
        &buffer_views[ji(val_obj, "bufferView").expect("sparse.values.bufferView") as usize];
    let val_buf = &buffers[ji(val_bv, "buffer").unwrap_or(0) as usize];
    let val_ct = ji(acc, "componentType").expect("accessor componentType");
    let val_csize = comp_size(val_ct);
    let val_start =
        (ji(val_bv, "byteOffset").unwrap_or(0) + ji(val_obj, "byteOffset").unwrap_or(0)) as usize;

    for s in 0..sp_count {
        let io = idx_start + s * idx_csize;
        let vi: usize = match idx_ct {
            5121 => idx_buf[io] as usize,
            5123 => u16::from_le_bytes([idx_buf[io], idx_buf[io + 1]]) as usize,
            5125 => u32::from_le_bytes([
                idx_buf[io],
                idx_buf[io + 1],
                idx_buf[io + 2],
                idx_buf[io + 3],
            ]) as usize,
            other => panic!("unsupported sparse indices componentType {other}"),
        };
        if vi >= out.len() {
            continue;
        }
        let vo = val_start + s * val_csize * dim;
        let mut elem = Vec::with_capacity(dim);
        for d in 0..dim {
            elem.push(decode_component(val_buf, vo + d * val_csize, val_ct));
        }
        out[vi] = elem;
    }
}

pub(super) fn read_accessor(gltf: &J, buffers: &[Vec<u8>], acc_idx: i64) -> Vec<Vec<f64>> {
    let accessors = jarr(gltf, "accessors").expect("accessors");
    let acc = &accessors[acc_idx as usize];

    let component_type = ji(acc, "componentType").expect("componentType");
    let csize = comp_size(component_type);
    let dim = type_dim(js(acc, "type").expect("accessor type"));
    let count = ji(acc, "count").expect("accessor count") as usize;

    let mut out: Vec<Vec<f64>> = match ji(acc, "bufferView") {
        Some(bv_idx) => {
            let bv = match jarr(gltf, "bufferViews").and_then(|bvs| bvs.get(bv_idx as usize)) {
                Some(bv) => bv,
                None => return vec![vec![0.0; dim]; count],
            };
            let buf = &buffers[ji(bv, "buffer").unwrap_or(0) as usize];
            let start =
                (ji(bv, "byteOffset").unwrap_or(0) + ji(acc, "byteOffset").unwrap_or(0)) as usize;
            let stride = match ji(bv, "byteStride") {
                Some(s) if s != 0 => s as usize,
                _ => csize * dim,
            };
            let mut out = Vec::with_capacity(count);
            for i in 0..count {
                let off = start + i * stride;
                let mut elem = Vec::with_capacity(dim);
                for d in 0..dim {
                    elem.push(decode_component(buf, off + d * csize, component_type));
                }
                out.push(elem);
            }
            out
        }
        None => vec![vec![0.0f64; dim]; count],
    };

    apply_sparse(gltf, buffers, acc, dim, &mut out);
    out
}

fn accessor_normalization_divisor(gltf: &J, acc_idx: i64) -> Option<f64> {
    let accessors = jarr(gltf, "accessors")?;
    let acc = accessors.get(acc_idx as usize)?;
    let normalized = acc
        .get("normalized")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if !normalized {
        return None;
    }
    match ji(acc, "componentType")? {
        5120 => Some(127.0),
        5121 => Some(255.0),
        5122 => Some(32767.0),
        5123 => Some(65535.0),
        _ => None,
    }
}

pub(super) fn read_accessor_normalized(
    gltf: &J,
    buffers: &[Vec<u8>],
    acc_idx: i64,
) -> Vec<Vec<f64>> {
    let mut out = read_accessor(gltf, buffers, acc_idx);
    if let Some(div) = accessor_normalization_divisor(gltf, acc_idx) {
        for elem in out.iter_mut() {
            for v in elem.iter_mut() {
                *v = (*v / div).max(-1.0);
            }
        }
    }
    out
}

pub(super) fn read_attribute_accessor(
    gltf: &J,
    buffers: &[Vec<u8>],
    acc_idx: i64,
    normalized_attribute_scaling: bool,
) -> Vec<Vec<f64>> {
    if normalized_attribute_scaling {
        read_accessor_normalized(gltf, buffers, acc_idx)
    } else {
        read_accessor(gltf, buffers, acc_idx)
    }
}

pub(super) fn read_morph_accessor(
    gltf: &J,
    buffers: &[Vec<u8>],
    acc_idx: i64,
    n_base: usize,
    dim: usize,
) -> Vec<Vec<f64>> {
    let accessors = jarr(gltf, "accessors").expect("accessors");
    let acc = &accessors[acc_idx as usize];

    let mut out: Vec<Vec<f64>> = if ji(acc, "bufferView").is_some() {
        let dense = read_accessor(gltf, buffers, acc_idx);
        if dense.len() == n_base {
            dense
        } else {
            let mut o = vec![vec![0.0f64; dim]; n_base];
            for (i, v) in dense.into_iter().take(n_base).enumerate() {
                o[i] = v;
            }
            o
        }
    } else {
        vec![vec![0.0f64; dim]; n_base]
    };

    apply_sparse(gltf, buffers, acc, dim, &mut out);
    out
}
