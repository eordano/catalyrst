use super::accessors::{jarr, ji, js};
use super::Resolve;
use anyhow::{anyhow, Result};
use serde_json::Value as J;

pub(super) fn decode_image_rgba8_unity(bytes: &[u8]) -> Option<image::RgbaImage> {
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

pub(super) fn decode_data_uri(uri: &str) -> Option<Vec<u8>> {
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
