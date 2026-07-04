use crate::mesh_layout;
use crate::scene::{
    AttrSig, Material, MorphSig, MorphTarget, Node, Primitive, Sampler, Scene, Skin, TexRef,
    TexTransform,
};
use crate::tangents::calculate_tangents;
use crate::value::Value;
use anyhow::{anyhow, Result};
use serde_json::Value as J;

pub type Resolve<'a> = Option<&'a (dyn Fn(&str) -> Option<Vec<u8>> + Sync)>;

fn comp_size(component_type: i64) -> usize {
    match component_type {
        5120 | 5121 => 1,
        5122 | 5123 => 2,
        5125 | 5126 => 4,
        _ => panic!("unsupported componentType {component_type}"),
    }
}

fn type_dim(t: &str) -> usize {
    match t {
        "SCALAR" => 1,
        "VEC2" => 2,
        "VEC3" => 3,
        "VEC4" => 4,
        "MAT4" => 16,
        other => panic!("unsupported accessor type {other}"),
    }
}

fn ji(v: &J, key: &str) -> Option<i64> {
    v.get(key).and_then(|x| x.as_i64())
}
fn jf(v: &J, key: &str) -> Option<f64> {
    v.get(key).and_then(|x| x.as_f64())
}
fn js<'a>(v: &'a J, key: &str) -> Option<&'a str> {
    v.get(key).and_then(|x| x.as_str())
}
fn jarr<'a>(v: &'a J, key: &str) -> Option<&'a Vec<J>> {
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

fn read_accessor(gltf: &J, buffers: &[Vec<u8>], acc_idx: i64) -> Vec<Vec<f64>> {
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

fn read_morph_accessor(
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

fn dot3_f32(a: [f32; 3], b: [f32; 3]) -> f32 {
    f32::mul_add(a[2], b[2], f32::mul_add(a[1], b[1], a[0] * b[0]))
}
fn dot4_f32(a: [f32; 4], b: [f32; 4]) -> f32 {
    f32::mul_add(
        a[3],
        b[3],
        f32::mul_add(a[2], b[2], f32::mul_add(a[1], b[1], a[0] * b[0])),
    )
}
fn normalize3_f32(c: [f32; 3]) -> [f32; 3] {
    let inv = 1.0f32 / dot3_f32(c, c).sqrt();
    [c[0] * inv, c[1] * inv, c[2] * inv]
}

fn quat_from_3x3_unity(c0: [f32; 3], c1: [f32; 3], c2: [f32; 3]) -> [f32; 4] {
    let (ux, uy, uz) = (c0[0], c0[1], c0[2]);
    let (vx, vy, vz) = (c1[0], c1[1], c1[2]);
    let (wx, wy, wz) = (c2[0], c2[1], c2[2]);
    let u_sign = ux.to_bits() & 0x8000_0000;
    let t = vy + f32::from_bits(wz.to_bits() ^ u_sign);
    let u_mask: u32 = if u_sign != 0 { 0xFFFF_FFFF } else { 0 };
    let t_mask: u32 = if t.to_bits() & 0x8000_0000 != 0 {
        0xFFFF_FFFF
    } else {
        0
    };
    let tr = 1.0f32 + ux.abs();
    let base = [0u32, 0x8000_0000, 0x8000_0000, 0x8000_0000];
    let ux_xor = [0u32, 0x8000_0000, 0u32, 0x8000_0000];
    let tx_xor = [0x8000_0000u32, 0x8000_0000, 0x8000_0000, 0u32];
    let mut sf = [0u32; 4];
    for i in 0..4 {
        sf[i] = base[i] ^ (u_mask & ux_xor[i]) ^ (t_mask & tx_xor[i]);
    }
    let lhs = [tr, uy, wx, vz];
    let rhs_in = [t, vx, uz, wy];
    let mut v = [0f32; 4];
    for i in 0..4 {
        v[i] = lhs[i] + f32::from_bits(rhs_in[i].to_bits() ^ sf[i]);
    }
    if u_mask != 0 {
        v = [v[2], v[3], v[0], v[1]];
    }
    if t_mask == 0 {
        v = [v[3], v[2], v[1], v[0]];
    }
    let inv = 1.0f32 / dot4_f32(v, v).sqrt();
    [v[0] * inv, v[1] * inv, v[2] * inv, v[3] * inv]
}

fn trs_from_matrix(m: &[f64; 16]) -> ([f64; 3], [f64; 4], [f64; 3]) {
    let t = [m[12], m[13], m[14]];
    let mut c0 = [m[0] as f32, -(m[1] as f32), -(m[2] as f32)];
    let mut c1 = [-(m[4] as f32), m[5] as f32, m[6] as f32];
    let mut c2 = [-(m[8] as f32), m[9] as f32, m[10] as f32];
    let len0 = dot3_f32(c0, c0).sqrt();
    let len1 = dot3_f32(c1, c1).sqrt();
    let len2 = dot3_f32(c2, c2).sqrt();
    for i in 0..3 {
        c0[i] /= len0;
        c1[i] /= len1;
        c2[i] /= len2;
    }
    let mut s = [len0, len1, len2];
    let cross = [
        c0[1] * c1[2] - c0[2] * c1[1],
        c0[2] * c1[0] - c0[0] * c1[2],
        c0[0] * c1[1] - c0[1] * c1[0],
    ];
    if dot3_f32(cross, c2) < 0.0 {
        for i in 0..3 {
            c0[i] = -c0[i];
            c1[i] = -c1[i];
            c2[i] = -c2[i];
        }
        for i in 0..3 {
            s[i] = -s[i];
        }
    }
    c0 = normalize3_f32(c0);
    c1 = normalize3_f32(c1);
    c2 = normalize3_f32(c2);
    let q = quat_from_3x3_unity(c0, c1, c2);
    (
        t,
        [q[0] as f64, q[1] as f64, q[2] as f64, q[3] as f64],
        [s[0] as f64, s[1] as f64, s[2] as f64],
    )
}

fn normalize_quat_f32(q: [f64; 4]) -> [f64; 4] {
    let qq = [q[0] as f32, q[1] as f32, q[2] as f32, q[3] as f32];
    let sq = [qq[0] * qq[0], qq[1] * qq[1], qq[2] * qq[2], qq[3] * qq[3]];

    let s0 = (sq[0] + sq[1]) + (sq[2] + sq[3]);

    let s7 = (sq[0] + sq[3]) + (sq[1] + sq[2]);
    if s0 == 0.0 || s7 == 0.0 {
        return [qq[0] as f64, qq[1] as f64, qq[2] as f64, qq[3] as f64];
    }
    let n0 = s0.sqrt();
    let n7 = s7.sqrt();
    [
        (qq[0] / n0) as f64,
        (qq[1] / n7) as f64,
        (qq[2] / n0) as f64,
        (qq[3] / n7) as f64,
    ]
}

fn node_trs(node: &J) -> ([f64; 3], [f64; 4], [f64; 3], bool) {
    if let Some(marr) = jarr(node, "matrix") {
        if marr.len() == 16 {
            let mut m = [0.0f64; 16];
            for (i, v) in marr.iter().enumerate() {
                m[i] = v.as_f64().unwrap_or(0.0);
            }
            let (t, r, s) = trs_from_matrix(&m);

            let r = normalize_quat_f32(r);
            return (t, r, s, true);
        }
    }
    let (t, has_translation) = match jarr(node, "translation") {
        Some(a) => (
            [
                a[0].as_f64().unwrap(),
                a[1].as_f64().unwrap(),
                a[2].as_f64().unwrap(),
            ],
            true,
        ),
        None => ([0.0, 0.0, 0.0], false),
    };

    let r = match jarr(node, "rotation") {
        Some(a) => {
            let rq = [
                a[0].as_f64().unwrap(),
                -a[1].as_f64().unwrap(),
                -a[2].as_f64().unwrap(),
                a[3].as_f64().unwrap(),
            ];
            normalize_quat_f32(rq)
        }
        None => [0.0, 0.0, 0.0, 1.0],
    };
    let s = jarr(node, "scale")
        .map(|a| {
            [
                a[0].as_f64().unwrap(),
                a[1].as_f64().unwrap(),
                a[2].as_f64().unwrap(),
            ]
        })
        .unwrap_or([1.0, 1.0, 1.0]);
    (t, r, s, has_translation)
}

fn decode_image_rgba8_unity(bytes: &[u8]) -> Option<image::RgbaImage> {
    let d = image::load_from_memory(bytes).ok()?;
    use image::DynamicImage::*;

    let needs_trunc = matches!(
        d,
        ImageLuma16(_) | ImageLumaA16(_) | ImageRgb16(_) | ImageRgba16(_)
    );
    if !needs_trunc {
        return Some(d.to_rgba8());
    }
    let src = d.to_rgba16();
    let (w, h) = (src.width(), src.height());
    let raw = src.as_raw();
    let mut out = vec![0u8; raw.len()];
    for (o, &s) in out.iter_mut().zip(raw.iter()) {
        *o = (s >> 8) as u8;
    }
    image::RgbaImage::from_raw(w, h, out)
}

fn decode_data_uri(uri: &str) -> Option<Vec<u8>> {
    if !uri.starts_with("data:") {
        return None;
    }
    let comma = uri.find(',')?;
    base64_decode(&uri[comma + 1..])
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    const fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0u32;
    for &c in s.as_bytes() {
        if c == b'=' || c.is_ascii_whitespace() {
            continue;
        }
        let v = val(c)?;
        buf = (buf << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Some(out)
}

fn parse_glb(bytes: &[u8]) -> Result<(J, Vec<u8>)> {
    if bytes.len() < 12 {
        return Err(anyhow!("GLB too short"));
    }
    let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    if magic != 0x4654_6C67 {
        return Err(anyhow!("bad GLB magic"));
    }

    let mut pos = 12usize;
    let mut json_chunk: Option<J> = None;
    let mut bin_chunk: Vec<u8> = Vec::new();
    while pos + 8 <= bytes.len() {
        let clen = u32::from_le_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]])
            as usize;
        let ctype = u32::from_le_bytes([
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ]);
        let data_start = pos + 8;
        let data_end = data_start + clen;
        if data_end > bytes.len() {
            break;
        }
        let data = &bytes[data_start..data_end];
        if ctype == 0x4E4F_534A {
            json_chunk = Some(serde_json::from_slice(data)?);
        } else if ctype == 0x004E_4942 {
            bin_chunk = data.to_vec();
        }
        pos = data_end;
    }
    let json = json_chunk.ok_or_else(|| anyhow!("GLB has no JSON chunk"))?;
    Ok((json, bin_chunk))
}

fn glb_json_chunk(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.len() < 12 || &bytes[0..4] != b"glTF" {
        return None;
    }
    let mut pos = 12usize;
    while pos + 8 <= bytes.len() {
        let clen = u32::from_le_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]])
            as usize;
        let ctype = u32::from_le_bytes([
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ]);
        let data_start = pos + 8;
        let data_end = data_start + clen;
        if data_end > bytes.len() {
            break;
        }
        if ctype == 0x4E4F_534A {
            return Some(bytes[data_start..data_end].to_vec());
        }
        pos = data_end;
    }
    None
}

fn fold_integer_neg_zero_node_rotations(gltf: &mut J, json_raw: &[u8]) {
    use serde_json::value::RawValue;

    #[derive(serde::Deserialize)]
    struct RawNode<'a> {
        #[serde(borrow, default)]
        rotation: Option<Vec<&'a RawValue>>,
    }
    #[derive(serde::Deserialize)]
    struct RawRoot<'a> {
        #[serde(borrow, default)]
        nodes: Option<Vec<RawNode<'a>>>,
    }

    let is_integer_neg_zero = |tok: &str| -> bool {
        let t = tok.trim();
        if let Some(rest) = t.strip_prefix('-') {
            !rest.is_empty() && rest.bytes().all(|b| b == b'0')
        } else {
            false
        }
    };

    let raw_root: RawRoot = match serde_json::from_slice(json_raw) {
        Ok(r) => r,
        Err(_) => return,
    };
    let raw_nodes = match raw_root.nodes {
        Some(n) => n,
        None => return,
    };

    let nodes = match gltf.get_mut("nodes").and_then(|n| n.as_array_mut()) {
        Some(n) => n,
        None => return,
    };

    for (ni, raw_node) in raw_nodes.iter().enumerate() {
        let Some(raw_rot) = raw_node.rotation.as_ref() else {
            continue;
        };
        let Some(rot) = nodes
            .get_mut(ni)
            .and_then(|n| n.get_mut("rotation"))
            .and_then(|r| r.as_array_mut())
        else {
            continue;
        };
        for (k, raw_comp) in raw_rot.iter().enumerate() {
            if k < rot.len() && is_integer_neg_zero(raw_comp.get()) {
                rot[k] = J::from(0);
            }
        }
    }
}

pub fn load_gltf_inputs(
    glb_bytes: &[u8],
    ext: &str,
    resolve: Resolve,
) -> Result<(J, Vec<Vec<u8>>)> {
    let is_gltf =
        ext.to_lowercase() == ".gltf" || (glb_bytes.len() >= 4 && &glb_bytes[0..4] != b"glTF");

    let (mut gltf, glb_blob, json_raw): (J, Vec<u8>, Vec<u8>) = if is_gltf {
        (
            serde_json::from_slice(glb_bytes)?,
            Vec::new(),
            glb_bytes.to_vec(),
        )
    } else {
        let (j, bin) = parse_glb(glb_bytes)?;
        let raw = glb_json_chunk(glb_bytes).unwrap_or_default();
        (j, bin, raw)
    };

    fold_integer_neg_zero_node_rotations(&mut gltf, &json_raw);

    let empty_vec: Vec<J> = Vec::new();
    let mut buffers: Vec<Vec<u8>> = Vec::new();
    for (bi, buf) in jarr(&gltf, "buffers")
        .unwrap_or(&empty_vec)
        .iter()
        .enumerate()
    {
        match js(buf, "uri") {
            None => buffers.push(glb_blob.clone()),
            Some(uri) if uri.starts_with("data:") => {
                buffers.push(decode_data_uri(uri).unwrap_or_default())
            }
            Some(uri) => {
                let declared = ji(buf, "byteLength").unwrap_or(0).max(0) as usize;
                let bytes = resolve.and_then(|f| f(uri)).ok_or_else(|| {
                    anyhow!(
                        "buffer[{bi}] external uri {uri:?} unresolved (missing content dependency)"
                    )
                })?;
                if bytes.len() < declared {
                    return Err(anyhow!(
                        "buffer[{bi}] external uri {uri:?} resolved to {} bytes < declared byteLength {declared}",
                        bytes.len()
                    ));
                }
                buffers.push(bytes);
            }
        }
    }
    if buffers.is_empty() {
        buffers = vec![glb_blob];
    }

    crate::draco::materialize(&mut gltf, &mut buffers)?;
    Ok((gltf, buffers))
}

pub fn parse(
    glb_bytes: &[u8],
    ext: &str,
    resolve: Resolve,
    magenta_missing: bool,
) -> Result<Scene> {
    let (gltf, buffers) = load_gltf_inputs(glb_bytes, ext, resolve)?;

    let empty_vec: Vec<J> = Vec::new();

    let buffer_views = jarr(&gltf, "bufferViews").cloned().unwrap_or_default();
    let mut images: Vec<Option<image::RgbaImage>> = Vec::new();
    let mut image_embedded: Vec<bool> = Vec::new();
    let mut image_bytes: Vec<Option<Vec<u8>>> = Vec::new();
    let mut image_uri: Vec<Option<String>> = Vec::new();
    for img in jarr(&gltf, "images").unwrap_or(&empty_vec) {
        let has_uri = js(img, "uri").is_some();
        let bv_idx = ji(img, "bufferView");
        let embedded = bv_idx.is_some() && !has_uri;

        let external_uri: Option<String> = match js(img, "uri") {
            Some(u) if bv_idx.is_none() && !u.starts_with("data:") => Some(u.to_string()),
            _ => None,
        };
        let raw: Option<Vec<u8>> = if let Some(bvi) = bv_idx {
            let bv = &buffer_views[bvi as usize];
            let buf = &buffers[ji(bv, "buffer").unwrap_or(0) as usize];
            let start = ji(bv, "byteOffset").unwrap_or(0) as usize;
            let len = ji(bv, "byteLength").unwrap_or(0) as usize;
            if start + len <= buf.len() {
                Some(buf[start..start + len].to_vec())
            } else {
                None
            }
        } else if let Some(uri) = js(img, "uri") {
            if uri.starts_with("data:") {
                decode_data_uri(uri)
            } else {
                resolve.and_then(|f| f(uri))
            }
        } else {
            None
        };

        let pil = raw.as_ref().and_then(|r| {
            let is_jpeg = r.len() >= 2 && r[0] == 0xFF && r[1] == 0xD8;
            if is_jpeg {
                if std::env::var_os("ABGEN_JPEG_GLB_9C").is_some() {
                    if let Some((rgba, w, h)) = libjpeg9c::decode_rgba(r, true) {
                        if let Some(im) = image::RgbaImage::from_raw(w, h, rgba) {
                            return Some(im);
                        }
                    }
                }
                if let Ok((rgba, w, h)) = crate::ffi::decode_jpeg_rgba(r) {
                    return image::RgbaImage::from_raw(w, h, rgba);
                }
            }
            decode_image_rgba8_unity(r)
        });

        let (pil, raw, external_uri) = if pil.is_none() && magenta_missing {
            let nm = external_uri.as_deref().unwrap_or("embedded texture");
            let mag = crate::placeholder::missing_texture("MISSING:", nm, 256);
            let mut buf = std::io::Cursor::new(Vec::new());
            let png = match mag.write_to(&mut buf, image::ImageFormat::Png) {
                Ok(()) => Some(buf.into_inner()),
                Err(_) => raw,
            };
            (Some(mag), png, None)
        } else {
            (pil, raw, external_uri)
        };
        images.push(pil);
        image_embedded.push(embedded);
        image_bytes.push(raw);
        image_uri.push(external_uri);
    }

    let raw_samplers = jarr(&gltf, "samplers").cloned().unwrap_or_default();
    let samplers: Vec<Sampler> = raw_samplers
        .iter()
        .map(|s| Sampler {
            mag_filter: ji(s, "magFilter"),
            min_filter: ji(s, "minFilter"),
            wrap_s: ji(s, "wrapS"),
            wrap_t: ji(s, "wrapT"),
        })
        .collect();

    let mut image_sampler: Vec<(Option<i64>, Option<i64>)> = vec![(None, None); images.len()];
    let mut image_wrap: Vec<(Option<i64>, Option<i64>)> = vec![(None, None); images.len()];
    for tex in jarr(&gltf, "textures").unwrap_or(&empty_vec) {
        let src = match ji(tex, "source") {
            Some(s) if (s as usize) < image_sampler.len() => s as usize,
            _ => continue,
        };
        if image_sampler[src] != (None, None) || image_wrap[src] != (None, None) {
            continue;
        }
        let (mag, mn, ws, wt) = match ji(tex, "sampler") {
            Some(si) if (si as usize) < samplers.len() => {
                let s = &samplers[si as usize];
                (s.mag_filter, s.min_filter, s.wrap_s, s.wrap_t)
            }
            _ => (None, None, None, None),
        };
        image_sampler[src] = (mag, mn);
        image_wrap[src] = (ws, wt);
    }

    let textures = jarr(&gltf, "textures").cloned().unwrap_or_default();

    let texture_refs: Vec<TexRef> = textures
        .iter()
        .filter_map(|tex| {
            let image = tex.get("source").and_then(|s| s.as_i64())? as usize;
            let sampler = tex
                .get("sampler")
                .and_then(|s| s.as_i64())
                .filter(|&i| i >= 0 && (i as usize) < samplers.len())
                .map(|i| i as usize);
            Some(TexRef { image, sampler })
        })
        .collect();

    let tex_transform = |tex_info: Option<&J>| -> Option<TexTransform> {
        let info = tex_info?;
        let ktt = info
            .get("extensions")
            .and_then(|e| e.get("KHR_texture_transform"))?;
        let g_off = jarr(ktt, "offset")
            .map(|a| [a[0].as_f64().unwrap_or(0.0), a[1].as_f64().unwrap_or(0.0)])
            .unwrap_or([0.0, 0.0]);
        let g_sca = jarr(ktt, "scale")
            .map(|a| [a[0].as_f64().unwrap_or(1.0), a[1].as_f64().unwrap_or(1.0)])
            .unwrap_or([1.0, 1.0]);
        let g_rot = ktt.get("rotation").and_then(|r| r.as_f64()).unwrap_or(0.0);
        let xform = if g_rot != 0.0 {
            let rot = g_rot as f32;
            let cos = rot.cos();
            let sin = rot.sin();
            let sx0 = g_sca[0] as f32;
            let sy0 = g_sca[1] as f32;
            let new_rot_y = sy0 * (-sin);
            let sx = sx0 * cos;
            let sy = sy0 * cos;
            let off_x = (g_off[0] as f32) - new_rot_y;
            let off_y = ((1.0 - g_off[1]) as f32) - sy;
            TexTransform {
                scale: [sx as f64, sy as f64],
                offset: [off_x as f64, off_y as f64],
            }
        } else {
            let m_off_y = (1.0_f32 - g_off[1] as f32 - g_sca[1] as f32) as f64;
            TexTransform {
                scale: [g_sca[0], g_sca[1]],
                offset: [g_off[0], m_off_y],
            }
        };

        if xform.is_identity() {
            None
        } else {
            Some(xform)
        }
    };

    let tex_ref = |tex_info: Option<&J>| -> Option<TexRef> {
        let j = tex_info?;
        let ti: i64 = match j.get("index") {
            Some(obj_idx) => obj_idx.as_i64()?,
            None => j.as_i64()?,
        };
        if ti < 0 || ti as usize >= textures.len() {
            return None;
        }
        let tex = &textures[ti as usize];
        let image = tex.get("source").and_then(|s| s.as_i64())? as usize;
        let sampler = tex
            .get("sampler")
            .and_then(|s| s.as_i64())
            .filter(|&i| i >= 0 && (i as usize) < samplers.len())
            .map(|i| i as usize);
        Some(TexRef { image, sampler })
    };

    let mut materials: Vec<Material> = Vec::new();
    for (i, m) in jarr(&gltf, "materials")
        .unwrap_or(&empty_vec)
        .iter()
        .enumerate()
    {
        let pbr = m.get("pbrMetallicRoughness");
        let base_color: [f64; 4] = pbr
            .and_then(|p| jarr(p, "baseColorFactor"))
            .map(|a| {
                [
                    a[0].as_f64().unwrap(),
                    a[1].as_f64().unwrap(),
                    a[2].as_f64().unwrap(),
                    a[3].as_f64().unwrap(),
                ]
            })
            .unwrap_or([1.0, 1.0, 1.0, 1.0]);
        let metallic = pbr.and_then(|p| jf(p, "metallicFactor")).unwrap_or(1.0);
        let roughness = pbr.and_then(|p| jf(p, "roughnessFactor")).unwrap_or(1.0);
        let alpha_mode = js(m, "alphaMode").unwrap_or("OPAQUE").to_string();
        let alpha_cutoff = jf(m, "alphaCutoff").unwrap_or(0.5);
        let emissive: [f64; 3] = jarr(m, "emissiveFactor")
            .map(|a| {
                [
                    a[0].as_f64().unwrap(),
                    a[1].as_f64().unwrap(),
                    a[2].as_f64().unwrap(),
                ]
            })
            .unwrap_or([0.0, 0.0, 0.0]);

        let normal_tex = m.get("normalTexture");
        let occlusion_tex = m.get("occlusionTexture");
        let normal_scale = normal_tex.and_then(|t| jf(t, "scale")).unwrap_or(1.0);
        let occlusion_strength = occlusion_tex.and_then(|t| jf(t, "strength")).unwrap_or(1.0);

        let spec_gloss_ext = m
            .get("extensions")
            .and_then(|e| e.get("KHR_materials_pbrSpecularGlossiness"));
        let (
            uses_spec_gloss,
            sg_diffuse_factor,
            sg_diffuse_tex_info,
            sg_specular_factor,
            sg_glossiness_factor,
            sg_spec_gloss_tex_info,
        ) = match spec_gloss_ext {
            Some(sg) => {
                let df = jarr(sg, "diffuseFactor")
                    .map(|a| {
                        [
                            a[0].as_f64().unwrap_or(1.0),
                            a[1].as_f64().unwrap_or(1.0),
                            a[2].as_f64().unwrap_or(1.0),
                            a[3].as_f64().unwrap_or(1.0),
                        ]
                    })
                    .unwrap_or([1.0, 1.0, 1.0, 1.0]);
                let sf = jarr(sg, "specularFactor")
                    .map(|a| {
                        [
                            a[0].as_f64().unwrap_or(1.0),
                            a[1].as_f64().unwrap_or(1.0),
                            a[2].as_f64().unwrap_or(1.0),
                        ]
                    })
                    .unwrap_or([1.0, 1.0, 1.0]);
                let gf = jf(sg, "glossinessFactor").unwrap_or(1.0);
                (
                    true,
                    df,
                    sg.get("diffuseTexture"),
                    sf,
                    gf,
                    sg.get("specularGlossinessTexture"),
                )
            }
            None => (false, [1.0; 4], None, [0.0; 3], 0.0, None),
        };

        let specular_ext = m
            .get("extensions")
            .and_then(|e| e.get("KHR_materials_specular"));
        let specular_color_tex_info = specular_ext.and_then(|sx| sx.get("specularColorTexture"));
        let uses_emissive_strength = m
            .get("extensions")
            .and_then(|e| e.get("KHR_materials_emissive_strength"))
            .is_some();

        let mut tex_transforms: std::collections::BTreeMap<String, TexTransform> =
            std::collections::BTreeMap::new();
        let mut record_xform = |slot: &str, info: Option<&J>| {
            if let Some(x) = tex_transform(info) {
                tex_transforms.insert(slot.to_string(), x);
            }
        };
        let base_color_tex_info = if uses_spec_gloss {
            sg_diffuse_tex_info
        } else {
            pbr.and_then(|p| p.get("baseColorTexture"))
        };

        let metal_rough_tex_info = if uses_spec_gloss {
            None
        } else {
            pbr.and_then(|p| p.get("metallicRoughnessTexture"))
        };
        record_xform("_BaseMap", base_color_tex_info);
        record_xform("_BumpMap", normal_tex);
        record_xform("_MetallicGlossMap", metal_rough_tex_info);
        record_xform("_OcclusionMap", occlusion_tex);
        record_xform("_EmissionMap", m.get("emissiveTexture"));
        record_xform("_SpecGlossMap", sg_spec_gloss_tex_info);
        record_xform("_SpecColorMap", specular_color_tex_info);

        let has_uv_channel = |info: Option<&J>| -> bool {
            info.and_then(|j| j.get("texCoord"))
                .and_then(|v| v.as_i64())
                .map(|n| n != 0 && n < 2)
                .unwrap_or(false)
        };
        let uses_uv_channel_select = has_uv_channel(base_color_tex_info)
            || has_uv_channel(metal_rough_tex_info)
            || has_uv_channel(normal_tex)
            || has_uv_channel(occlusion_tex)
            || has_uv_channel(m.get("emissiveTexture"))
            || has_uv_channel(sg_spec_gloss_tex_info)
            || has_uv_channel(specular_color_tex_info);

        let final_base_color = if uses_spec_gloss {
            sg_diffuse_factor
        } else {
            base_color
        };
        let final_base_color_image = if uses_spec_gloss {
            tex_ref(sg_diffuse_tex_info)
        } else {
            pbr.and_then(|p| tex_ref(p.get("baseColorTexture")))
        };

        materials.push(Material {
            name: js(m, "name")
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("material_{i}")),
            base_color: final_base_color,
            metallic,
            roughness,
            alpha_mode,
            alpha_cutoff,
            emissive,
            base_color_image: final_base_color_image,
            base_color_emit_image: pbr.and_then(|p| tex_ref(p.get("baseColorTexture"))),
            emissive_image: tex_ref(m.get("emissiveTexture")),
            normal_image: tex_ref(normal_tex),
            metallic_roughness_image: tex_ref(metal_rough_tex_info),
            metal_rough_emit_image: tex_ref(pbr.and_then(|p| p.get("metallicRoughnessTexture"))),
            occlusion_image: tex_ref(occlusion_tex),
            normal_scale,
            occlusion_strength,
            double_sided: m
                .get("doubleSided")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            tex_transforms,
            uses_uv_channel_select,
            uses_spec_gloss,
            spec_gloss_image: tex_ref(sg_spec_gloss_tex_info),
            specular_factor: sg_specular_factor,
            glossiness_factor: sg_glossiness_factor,
            specular_color_image: tex_ref(specular_color_tex_info),
            uses_emissive_strength,
        });
    }

    let meshes = jarr(&gltf, "meshes").cloned().unwrap_or_default();

    let build_primitives =
        |mesh_idx: i64, mesh_name: &str, skin_index: Option<usize>| -> Vec<Primitive> {
            let mut prims = Vec::new();
            let mesh = &meshes[mesh_idx as usize];
            let primitives = jarr(mesh, "primitives").cloned().unwrap_or_default();
            for (pi, prim) in primitives.iter().enumerate() {
                let attrs = match prim.get("attributes") {
                    Some(a) => a,
                    None => continue,
                };

                let pos_acc = match ji(attrs, "POSITION") {
                    Some(a) => a,
                    None => continue,
                };
                let positions: Vec<[f64; 3]> = read_accessor(&gltf, &buffers, pos_acc)
                    .iter()
                    .map(|p| [-p[0], p[1], p[2]])
                    .collect();
                let nverts = positions.len();

                let (position_min_decl, position_max_decl) = {
                    let accessors = jarr(&gltf, "accessors").expect("accessors");
                    let acc = &accessors[pos_acc as usize];
                    let read_vec3 = |key: &str| -> Option<[f64; 3]> {
                        let arr = jarr(acc, key)?;
                        if arr.len() < 3 {
                            return None;
                        }
                        Some([arr[0].as_f64()?, arr[1].as_f64()?, arr[2].as_f64()?])
                    };
                    match (read_vec3("min"), read_vec3("max")) {
                        (Some(mn), Some(mx)) => {
                            (Some([-mx[0], mn[1], mn[2]]), Some([-mn[0], mx[1], mx[2]]))
                        }
                        _ => (None, None),
                    }
                };

                let has_source_normals = ji(attrs, "NORMAL").is_some();
                let normals: Vec<[f64; 3]> = match ji(attrs, "NORMAL") {
                    Some(a) => read_accessor(&gltf, &buffers, a)
                        .iter()
                        .map(|n| [-n[0], n[1], n[2]])
                        .collect(),

                    None => vec![[0.0, 0.0, 1.0]; nverts],
                };

                let mut uv_sets: Vec<Vec<[f64; 2]>> = Vec::new();
                for ui in 0..8 {
                    match ji(attrs, &format!("TEXCOORD_{ui}")) {
                        None => break,
                        Some(a) => {
                            let uv: Vec<[f64; 2]> = read_accessor(&gltf, &buffers, a)
                                .iter()
                                .map(|u| [u[0], 1.0 - u[1]])
                                .collect();
                            uv_sets.push(uv);
                        }
                    }
                }
                let uvs: Option<Vec<[f64; 2]>> = if !uv_sets.is_empty() {
                    Some(uv_sets[0].clone())
                } else {
                    None
                };

                let tangents: Option<Vec<[f64; 4]>> = ji(attrs, "TANGENT").map(|a| {
                    read_accessor(&gltf, &buffers, a)
                        .iter()
                        .map(|t| [t[0], t[1], -t[2], t[3]])
                        .collect()
                });

                let colors: Option<Vec<[f64; 4]>> = ji(attrs, "COLOR_0").map(|a| {
                    let accessors = jarr(&gltf, "accessors").unwrap();
                    let acc = &accessors[a as usize];
                    let dim = type_dim(js(acc, "type").unwrap());
                    let ct = ji(acc, "componentType").unwrap() as i32;
                    read_accessor(&gltf, &buffers, a)
                        .iter()
                        .map(|c| mesh_layout::normalize_color(c, ct, dim))
                        .collect()
                });

                let mut weights: Option<Vec<[f64; 4]>> = None;
                let mut joints: Option<Vec<[u32; 4]>> = None;
                if let (Some(wa), Some(ja)) = (ji(attrs, "WEIGHTS_0"), ji(attrs, "JOINTS_0")) {
                    let accessors = jarr(&gltf, "accessors").unwrap();
                    let wct = ji(&accessors[wa as usize], "componentType").unwrap() as i32;
                    let raw_w: Vec<[f64; 4]> = read_accessor(&gltf, &buffers, wa)
                        .iter()
                        .map(|w| mesh_layout::normalize_weights(w, wct))
                        .collect();
                    let raw_j: Vec<[u32; 4]> = read_accessor(&gltf, &buffers, ja)
                        .iter()
                        .map(|j| [j[0] as u32, j[1] as u32, j[2] as u32, j[3] as u32])
                        .collect();
                    let mut wv = Vec::with_capacity(raw_w.len());
                    let mut jv = Vec::with_capacity(raw_w.len());
                    for (w, j) in raw_w.iter().zip(raw_j.iter()) {
                        let (sw, sj) = mesh_layout::sort_and_normalize_bones(*w, *j);
                        wv.push(sw);
                        jv.push(sj);
                    }
                    weights = Some(wv);
                    joints = Some(jv);
                }

                let raw_idx: Vec<u32> = match ji(prim, "indices") {
                    Some(a) => read_accessor(&gltf, &buffers, a)
                        .iter()
                        .map(|v| v[0] as u32)
                        .collect(),
                    None => (0..nverts as u32).collect(),
                };
                let mut idx: Vec<u32> = Vec::new();
                if raw_idx.len() >= 2 {
                    let mut k = 0usize;
                    while k + 2 < raw_idx.len() {
                        idx.push(raw_idx[k]);
                        idx.push(raw_idx[k + 2]);
                        idx.push(raw_idx[k + 1]);
                        k += 3;
                    }
                }

                let mut morph_targets: Vec<MorphTarget> = Vec::new();
                let mut gltf_morph_sig: Vec<MorphSig> = Vec::new();
                if let Some(tgts) = jarr(prim, "targets") {
                    for t in tgts {
                        gltf_morph_sig.push(MorphSig {
                            position: ji(t, "POSITION"),
                            normal: ji(t, "NORMAL"),
                            tangent: ji(t, "TANGENT"),
                        });
                        let pos = ji(t, "POSITION").map(|a| {
                            read_morph_accessor(&gltf, &buffers, a, nverts, 3)
                                .into_iter()
                                .map(|p| [-p[0], p[1], p[2]])
                                .collect::<Vec<[f64; 3]>>()
                        });

                        let positions = pos.unwrap_or_else(|| vec![[0.0; 3]; nverts]);
                        let normals: Option<Vec<[f64; 3]>> = ji(t, "NORMAL").map(|a| {
                            read_morph_accessor(&gltf, &buffers, a, nverts, 3)
                                .into_iter()
                                .map(|n| [-n[0], n[1], n[2]])
                                .collect()
                        });
                        let tangents: Option<Vec<[f64; 3]>> = ji(t, "TANGENT").map(|a| {
                            read_morph_accessor(&gltf, &buffers, a, nverts, 3)
                                .into_iter()
                                .map(|t| [-t[0], t[1], t[2]])
                                .collect()
                        });
                        morph_targets.push(MorphTarget {
                            positions,
                            normals,
                            tangents,
                        });
                    }
                }

                let morph_weights: Vec<f32> = if morph_targets.is_empty() {
                    Vec::new()
                } else {
                    let from_mesh = jarr(mesh, "weights")
                        .map(|a| {
                            a.iter()
                                .map(|v| v.as_f64().unwrap_or(0.0) as f32)
                                .collect::<Vec<f32>>()
                        })
                        .unwrap_or_default();

                    let mut w = from_mesh;
                    w.resize(morph_targets.len(), 0.0);
                    w
                };

                let morph_target_names: Vec<String> = if morph_targets.is_empty() {
                    Vec::new()
                } else {
                    let names: Vec<String> = mesh
                        .get("extras")
                        .and_then(|e| e.get("targetNames"))
                        .and_then(|a| a.as_array())
                        .map(|a| {
                            a.iter()
                                .map(|v| v.as_str().unwrap_or("").to_string())
                                .collect()
                        })
                        .unwrap_or_default();
                    (0..morph_targets.len())
                        .map(|i| match names.get(i) {
                            Some(s) if !s.is_empty() => s.clone(),
                            _ => format!("{i}"),
                        })
                        .collect()
                };

                let go_name = if pi == 0 {
                    mesh_name.to_string()
                } else {
                    format!("{mesh_name}_{pi}")
                };

                let mut tc_sig: Vec<i64> = Vec::new();
                for ui in 0..8 {
                    match ji(attrs, &format!("TEXCOORD_{ui}")) {
                        Some(a) => tc_sig.push(a),
                        None => break,
                    }
                }
                let attr_sig = AttrSig {
                    position: Some(pos_acc),
                    normal: ji(attrs, "NORMAL"),
                    tangent: ji(attrs, "TANGENT"),
                    texcoords: tc_sig,
                    color: ji(attrs, "COLOR_0"),
                    joints: ji(attrs, "JOINTS_0"),
                    weights: ji(attrs, "WEIGHTS_0"),
                };

                let from_draco = prim
                    .get("_abgen_from_draco")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                prims.push(Primitive {
                    positions,
                    normals,
                    has_source_normals,
                    uvs,
                    tangents,
                    indices: idx,
                    material_index: ji(prim, "material").map(|x| x as usize),
                    name: mesh_name.to_string(),
                    colors,
                    uv_sets,
                    weights,
                    joints,
                    skin_index,
                    go_name,
                    morph_targets,
                    morph_weights,
                    morph_target_names,
                    gltf_mesh_index: Some(mesh_idx as usize),
                    gltf_prim_index: pi,
                    gltf_attr_sig: Some(attr_sig),
                    gltf_morph_sig,
                    position_min_decl,
                    position_max_decl,
                    from_draco,
                });
            }
            prims
        };

    let json_nodes = jarr(&gltf, "nodes").cloned().unwrap_or_default();
    let mut nodes: Vec<Node> = Vec::new();
    for n in json_nodes.iter() {
        let (t, r, s, flip_translation_x) = node_trs(n);
        let name = js(n, "name").unwrap_or("").to_string();
        let skin_index = ji(n, "skin").map(|x| x as usize);
        let mut mesh_name_is_collider = false;
        let prims = match ji(n, "mesh") {
            Some(mesh_idx) => {
                let mesh = &meshes[mesh_idx as usize];
                let raw_mesh_name = js(mesh, "name").unwrap_or("");
                mesh_name_is_collider = raw_mesh_name.to_lowercase().contains("_collider");
                build_primitives(mesh_idx, raw_mesh_name, skin_index)
            }
            None => Vec::new(),
        };
        let children: Vec<usize> = jarr(n, "children")
            .map(|a| a.iter().map(|v| v.as_i64().unwrap() as usize).collect())
            .unwrap_or_default();

        let tx = if flip_translation_x { -t[0] } else { t[0] };

        let name_is_collider = name.to_lowercase().contains("_collider");

        let has_mesh = !prims.is_empty();
        nodes.push(Node {
            name: name.clone(),
            translation: [tx, t[1], t[2]],
            rotation: r,
            scale: s,
            primitives: prims,
            children,
            is_collider: has_mesh
                && (if name.is_empty() {
                    mesh_name_is_collider
                } else {
                    name_is_collider
                }),
            name_is_collider,
            extra_colliders: 0,
        });
    }

    {
        let mut stack: Vec<usize> = (0..nodes.len()).filter(|&i| nodes[i].is_collider).collect();
        while let Some(idx) = stack.pop() {
            let children = nodes[idx].children.clone();
            for c in children {
                if c < nodes.len() && !nodes[c].is_collider {
                    nodes[c].is_collider = true;
                    stack.push(c);
                }
            }
        }
    }

    {
        let name_collider: Vec<bool> = nodes
            .iter()
            .map(|n| !n.primitives.is_empty() && n.name.to_lowercase().contains("_collider"))
            .collect();
        let mut has_parent = vec![false; nodes.len()];
        for n in nodes.iter() {
            for &c in &n.children {
                if c < has_parent.len() {
                    has_parent[c] = true;
                }
            }
        }

        let mut stack: Vec<(usize, usize)> = (0..nodes.len())
            .rev()
            .filter(|&i| !has_parent[i])
            .map(|i| (i, 0usize))
            .collect();
        while let Some((idx, anc)) = stack.pop() {
            if idx >= nodes.len() {
                continue;
            }
            if !nodes[idx].primitives.is_empty() && nodes[idx].is_collider {
                let visits = anc + usize::from(name_collider[idx]);
                nodes[idx].extra_colliders = visits.saturating_sub(1);
            }
            let child_anc = anc + usize::from(name_collider[idx]);
            for &c in nodes[idx].children.clone().iter() {
                stack.push((c, child_anc));
            }
        }
    }

    let mut skins: Vec<Skin> = Vec::new();
    for sk in jarr(&gltf, "skins").unwrap_or(&empty_vec) {
        let sk_joints: Vec<usize> = jarr(sk, "joints")
            .map(|a| a.iter().map(|v| v.as_i64().unwrap() as usize).collect())
            .unwrap_or_default();
        let bind_poses: Vec<[f64; 16]> = match ji(sk, "inverseBindMatrices") {
            Some(a) => read_accessor(&gltf, &buffers, a)
                .iter()
                .map(|m| {
                    let mut arr = [0.0f64; 16];
                    for (i, v) in m.iter().enumerate() {
                        arr[i] = *v;
                    }
                    mesh_layout::convert_bind_matrix(arr)
                })
                .collect(),
            None => sk_joints
                .iter()
                .map(|_| {
                    [
                        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0,
                        1.0,
                    ]
                })
                .collect(),
        };
        skins.push(Skin {
            joints: sk_joints,
            skeleton: ji(sk, "skeleton").map(|x| x as usize),
            bind_poses,
        });
    }

    let scenes = jarr(&gltf, "scenes").cloned().unwrap_or_default();
    let scene_idx = ji(&gltf, "scene").unwrap_or(0);

    let scene_idx_clamped: usize = if scene_idx < 0 || (scene_idx as usize) >= scenes.len() {
        0
    } else {
        scene_idx as usize
    };
    let scene_roots_from = |s: &J| -> Vec<usize> {
        jarr(s, "nodes")
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_i64())
                    .filter(|&i| i >= 0 && (i as usize) < nodes.len())
                    .map(|i| i as usize)
                    .collect()
            })
            .unwrap_or_default()
    };
    let (roots, scene_name): (Vec<usize>, Option<String>) = if !scenes.is_empty() {
        let s = &scenes[scene_idx_clamped];
        let r = scene_roots_from(s);
        let n = js(s, "name").map(|s| s.to_string());
        (r, n)
    } else {
        ((0..nodes.len()).collect(), None)
    };
    let mut extra_scenes: Vec<(Option<String>, Vec<usize>)> = Vec::new();
    if !scenes.is_empty() {
        for (i, s) in scenes.iter().enumerate() {
            if i == scene_idx_clamped {
                continue;
            }
            let r = scene_roots_from(s);
            let n = js(s, "name").map(|s| s.to_string());
            extra_scenes.push((n, r));
        }
    }

    let mut normal_uses: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let mut other_uses: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for m in &materials {
        if let Some(ni) = m.normal_image {
            normal_uses.insert(ni.image);
        }
        for tr in [
            m.base_color_image,
            m.emissive_image,
            m.metal_rough_emit_image,
            m.occlusion_image,
            m.spec_gloss_image,
        ]
        .into_iter()
        .flatten()
        {
            other_uses.insert(tr.image);
        }
    }
    let normal_images: std::collections::HashSet<usize> =
        normal_uses.difference(&other_uses).copied().collect();

    let has_animation = jarr(&gltf, "animations").is_some_and(|a| !a.is_empty());

    let name_key_present: Vec<bool> = json_nodes
        .iter()
        .map(|n| n.get("name").and_then(|v| v.as_str()).is_some())
        .collect();
    let unique_node_names: Vec<String> = {
        let base_name = |i: usize| -> String {
            if has_animation {
                if !nodes[i].name.trim().is_empty() {
                    return nodes[i].name.clone();
                }
                if let Some(p) = nodes[i].primitives.first() {
                    if !p.name.trim().is_empty() {
                        return p.name.clone();
                    }
                }
                format!("Node-{i}")
            } else {
                if !nodes[i].name.is_empty() {
                    return nodes[i].name.clone();
                }
                if let Some(p) = nodes[i].primitives.first() {
                    if !p.name.is_empty() {
                        return p.name.clone();
                    }
                }
                if name_key_present.get(i).copied().unwrap_or(false) {
                    return String::new();
                }
                format!("Node-{i}")
            }
        };
        let mut names: Vec<String> = vec![String::new(); nodes.len()];
        let assign = |order: &[usize],
                      names: &mut Vec<String>,
                      base_name: &dyn Fn(usize) -> String| {
            let mut exclude: std::collections::HashSet<String> = std::collections::HashSet::new();
            for &ci in order {
                let name = base_name(ci);

                if !has_animation || nodes[ci].name_is_collider || name.is_empty() {
                    names[ci] = name;
                    continue;
                }
                if exclude.contains(&name) {
                    let mut i = 0usize;
                    let chosen = loop {
                        let ext = format!("{name}_{i}");
                        i += 1;
                        if !exclude.contains(&ext) {
                            break ext;
                        }
                    };
                    exclude.insert(chosen.clone());
                    names[ci] = chosen;
                } else {
                    exclude.insert(name.clone());
                    names[ci] = name;
                }
            }
        };

        for n in &nodes {
            if !n.children.is_empty() {
                let order: Vec<usize> = n
                    .children
                    .iter()
                    .copied()
                    .filter(|&c| c < nodes.len())
                    .collect();
                assign(&order, &mut names, &base_name);
            }
        }

        assign(&roots, &mut names, &base_name);
        for (_, scene_roots) in &extra_scenes {
            let order: Vec<usize> = scene_roots
                .iter()
                .copied()
                .filter(|&c| c < nodes.len())
                .collect();
            assign(&order, &mut names, &base_name);
        }
        names
    };

    let mut scene = Scene {
        nodes,
        root_nodes: roots,
        name: scene_name,
        materials,
        images,
        image_embedded,
        image_bytes,
        image_sampler,
        image_wrap,
        samplers,
        image_uri,
        texture_refs,
        normal_images,
        skins,
        extra_scenes,
        unique_node_names,
    };

    for node in scene.nodes.iter_mut() {
        for prim in node.primitives.iter_mut() {
            if prim.has_source_normals || prim.indices.len() < 3 {
                continue;
            }
            prim.normals = crate::normals::recalculate_normals(&prim.positions, &prim.indices);
        }
    }

    for node in scene.nodes.iter_mut() {
        for prim in node.primitives.iter_mut() {
            if prim.tangents.is_some() && !prim.from_draco {
                continue;
            }
            let mi = match prim.material_index {
                Some(mi) if mi < scene.materials.len() => mi,
                _ => continue,
            };
            if scene.materials[mi].normal_image.is_none() {
                continue;
            }
            let empty_uvs: Vec<[f64; 2]> = Vec::new();
            let uvs = prim.uvs.as_deref().unwrap_or(&empty_uvs);
            let tangents = calculate_tangents(&prim.positions, &prim.normals, uvs, &prim.indices);
            prim.tangents = Some(tangents);
        }
    }

    Ok(scene)
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
    use super::*;
    use serde_json::json;

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
        let scene = super::parse(gltf.as_bytes(), ".gltf", None, false).expect("parse");
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
        let scene = super::parse(gltf.as_bytes(), ".gltf", None, false).expect("parse");
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
        let scene = super::parse(gltf.as_bytes(), ".gltf", None, false).expect("parse");
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
