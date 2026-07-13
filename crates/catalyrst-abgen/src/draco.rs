use anyhow::{anyhow, Result};
use draco_decoder::{decode_mesh_with_config_sync, AttributeDataType};
use serde_json::{json, Map, Value as J};

const COMPONENT_TYPE_UNSIGNED_SHORT: i64 = 5123;
const COMPONENT_TYPE_UNSIGNED_INT: i64 = 5125;
const COMPONENT_TYPE_FLOAT: i64 = 5126;
const COMPONENT_TYPE_BYTE: i64 = 5120;
const COMPONENT_TYPE_UNSIGNED_BYTE: i64 = 5121;
const COMPONENT_TYPE_SHORT: i64 = 5122;
const COMPONENT_TYPE_INT: i64 = 5124;

fn type_str_for_dim(dim: u32) -> &'static str {
    match dim {
        1 => "SCALAR",
        2 => "VEC2",
        3 => "VEC3",
        4 => "VEC4",
        _ => "SCALAR",
    }
}

fn component_type_for(dt: AttributeDataType) -> i64 {
    match dt {
        AttributeDataType::Int8 => COMPONENT_TYPE_BYTE,
        AttributeDataType::UInt8 => COMPONENT_TYPE_UNSIGNED_BYTE,
        AttributeDataType::Int16 => COMPONENT_TYPE_SHORT,
        AttributeDataType::UInt16 => COMPONENT_TYPE_UNSIGNED_SHORT,
        AttributeDataType::Int32 => COMPONENT_TYPE_INT,
        AttributeDataType::UInt32 => COMPONENT_TYPE_UNSIGNED_INT,
        AttributeDataType::Float32 => COMPONENT_TYPE_FLOAT,
    }
}

fn has_draco(gltf: &J) -> bool {
    if let Some(req) = gltf.get("extensionsRequired").and_then(|x| x.as_array()) {
        for s in req {
            if s.as_str() == Some("KHR_draco_mesh_compression") {
                return true;
            }
        }
    }
    let meshes = match gltf.get("meshes").and_then(|x| x.as_array()) {
        Some(m) => m,
        None => return false,
    };
    for m in meshes {
        let prims = match m.get("primitives").and_then(|x| x.as_array()) {
            Some(p) => p,
            None => continue,
        };
        for p in prims {
            if p.get("extensions")
                .and_then(|e| e.get("KHR_draco_mesh_compression"))
                .is_some()
            {
                return true;
            }
        }
    }
    false
}

fn slice_from_buffer_view<'a>(bv: &J, buffers: &'a [Vec<u8>]) -> Result<&'a [u8]> {
    let buf_idx = bv.get("buffer").and_then(|x| x.as_i64()).unwrap_or(0) as usize;
    let buf = buffers
        .get(buf_idx)
        .ok_or_else(|| anyhow!("draco: bufferView.buffer={buf_idx} out of range"))?;
    let off = bv.get("byteOffset").and_then(|x| x.as_i64()).unwrap_or(0) as usize;
    let len = bv
        .get("byteLength")
        .and_then(|x| x.as_i64())
        .ok_or_else(|| anyhow!("draco: bufferView missing byteLength"))? as usize;
    if off + len > buf.len() {
        return Err(anyhow!("draco: bufferView out of range"));
    }
    Ok(&buf[off..off + len])
}

pub fn materialize(gltf: &mut J, buffers: &mut [Vec<u8>]) -> Result<()> {
    if !has_draco(gltf) {
        return Ok(());
    }
    if buffers.is_empty() {
        return Err(anyhow!("draco: no buffers"));
    }

    let mut meshes = gltf
        .get("meshes")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
    let mut accessors = gltf
        .get("accessors")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
    let mut buffer_views = gltf
        .get("bufferViews")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();

    let bv_snapshot = buffer_views.clone();

    let buffers_snapshot: Vec<Vec<u8>> = buffers.to_vec();

    let mut new_bytes: Vec<u8> = Vec::new();
    let base_offset = buffers[0].len();

    for mesh in meshes.iter_mut() {
        let prims = match mesh.get_mut("primitives").and_then(|x| x.as_array_mut()) {
            Some(p) => p,
            None => continue,
        };
        for prim in prims.iter_mut() {
            let draco_ext = prim
                .get("extensions")
                .and_then(|e| e.get("KHR_draco_mesh_compression"))
                .cloned();
            let Some(draco_ext) = draco_ext else { continue };

            let src_bv_idx = draco_ext
                .get("bufferView")
                .and_then(|x| x.as_i64())
                .ok_or_else(|| anyhow!("draco: ext.bufferView missing"))?
                as usize;
            let src_bv = bv_snapshot
                .get(src_bv_idx)
                .ok_or_else(|| anyhow!("draco: ext.bufferView out of range"))?;
            let src_bytes = slice_from_buffer_view(src_bv, &buffers_snapshot)?;

            let result = decode_mesh_with_config_sync(src_bytes)
                .ok_or_else(|| anyhow!("draco: decode failed"))?;
            let cfg = &result.config;
            let data = &result.data;

            let ext_attrs = draco_ext
                .get("attributes")
                .and_then(|x| x.as_object())
                .ok_or_else(|| anyhow!("draco: ext.attributes missing"))?;
            let mut sem_by_uid: std::collections::HashMap<u32, String> =
                std::collections::HashMap::new();
            for (sem, v) in ext_attrs {
                let uid = v
                    .as_u64()
                    .ok_or_else(|| anyhow!("draco: attribute uid not int"))?
                    as u32;
                sem_by_uid.insert(uid, sem.clone());
            }

            let prim_attrs = prim
                .get("attributes")
                .and_then(|x| x.as_object())
                .cloned()
                .unwrap_or_default();

            let idx_acc_idx = prim
                .get("indices")
                .and_then(|x| x.as_i64())
                .ok_or_else(|| anyhow!("draco: primitive missing indices"))?
                as usize;
            let idx_acc = accessors
                .get(idx_acc_idx)
                .ok_or_else(|| anyhow!("draco: indices accessor out of range"))?
                .clone();
            let idx_component_type = idx_acc
                .get("componentType")
                .and_then(|x| x.as_i64())
                .unwrap_or(COMPONENT_TYPE_UNSIGNED_INT);
            let idx_count = cfg.index_count() as usize;

            let draco_idx_u32: Vec<u32> = if cfg.index_count() <= u16::MAX as u32 {
                let il = cfg.index_length() as usize;
                let raw = &data[..il];
                raw.chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]) as u32)
                    .collect()
            } else {
                let il = cfg.index_length() as usize;
                let raw = &data[..il];
                raw.chunks_exact(4)
                    .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .collect()
            };

            let (idx_bytes, idx_byte_len) = match idx_component_type {
                COMPONENT_TYPE_UNSIGNED_BYTE => {
                    let mut b = Vec::with_capacity(idx_count);
                    for v in &draco_idx_u32 {
                        b.push(*v as u8);
                    }
                    (b, idx_count)
                }
                COMPONENT_TYPE_UNSIGNED_SHORT => {
                    let mut b = Vec::with_capacity(idx_count * 2);
                    for v in &draco_idx_u32 {
                        b.extend_from_slice(&(*v as u16).to_le_bytes());
                    }
                    (b, idx_count * 2)
                }
                _ => {
                    let mut b = Vec::with_capacity(idx_count * 4);
                    for v in &draco_idx_u32 {
                        b.extend_from_slice(&v.to_le_bytes());
                    }
                    (b, idx_count * 4)
                }
            };

            let idx_bv_byte_offset = base_offset + new_bytes.len();
            new_bytes.extend_from_slice(&idx_bytes);
            let new_idx_bv = json!({
                "buffer": 0,
                "byteOffset": idx_bv_byte_offset,
                "byteLength": idx_byte_len,
            });
            let new_idx_bv_idx = buffer_views.len();
            buffer_views.push(new_idx_bv);

            let mut new_idx_acc_obj: Map<String, J> = match idx_acc {
                J::Object(o) => o,
                _ => Map::new(),
            };
            new_idx_acc_obj.insert("bufferView".into(), json!(new_idx_bv_idx));
            new_idx_acc_obj.insert("byteOffset".into(), json!(0));
            new_idx_acc_obj.insert("componentType".into(), json!(idx_component_type));
            new_idx_acc_obj.insert("count".into(), json!(idx_count));
            new_idx_acc_obj.insert("type".into(), json!("SCALAR"));

            new_idx_acc_obj.remove("min");
            new_idx_acc_obj.remove("max");
            let new_idx_acc_idx = accessors.len();
            accessors.push(J::Object(new_idx_acc_obj));

            prim.as_object_mut()
                .unwrap()
                .insert("indices".into(), json!(new_idx_acc_idx));

            let mut new_prim_attrs = Map::new();

            for (k, v) in prim_attrs.iter() {
                if !sem_by_uid.values().any(|sem| sem == k) {
                    new_prim_attrs.insert(k.clone(), v.clone());
                }
            }
            for attr in cfg.attributes() {
                let uid = attr.unique_id();
                let sem = sem_by_uid
                    .get(&uid)
                    .ok_or_else(|| anyhow!("draco: decoded uid {uid} not in ext.attributes"))?
                    .clone();
                let off = attr.offset() as usize;
                let len = attr.lenght() as usize;
                if off + len > data.len() {
                    return Err(anyhow!("draco: attr {sem} out of decoded buffer"));
                }
                let bv_off = base_offset + new_bytes.len();
                new_bytes.extend_from_slice(&data[off..off + len]);
                let new_bv = json!({
                    "buffer": 0,
                    "byteOffset": bv_off,
                    "byteLength": len,
                });
                let new_bv_idx = buffer_views.len();
                buffer_views.push(new_bv);

                let prior_acc_idx = prim_attrs.get(&sem).and_then(|x| x.as_i64());
                let mut acc_obj: Map<String, J> =
                    match prior_acc_idx.and_then(|i| accessors.get(i as usize).cloned()) {
                        Some(J::Object(o)) => o,
                        _ => Map::new(),
                    };
                acc_obj.insert("bufferView".into(), json!(new_bv_idx));
                acc_obj.insert("byteOffset".into(), json!(0));
                acc_obj.insert(
                    "componentType".into(),
                    json!(component_type_for(attr.data_type())),
                );
                acc_obj.insert("count".into(), json!(cfg.vertex_count()));
                acc_obj.insert("type".into(), json!(type_str_for_dim(attr.dim())));

                let is_position = sem == "POSITION" && attr.dim() == 3;
                let has_min_max = acc_obj.contains_key("min") && acc_obj.contains_key("max");
                if !is_position {
                    acc_obj.remove("min");
                    acc_obj.remove("max");
                } else if !has_min_max {
                    if let Some((mn, mx)) = cfg.position_quant_bounds() {
                        acc_obj.insert(
                            "min".into(),
                            json!([mn[0] as f64, mn[1] as f64, mn[2] as f64]),
                        );
                        acc_obj.insert(
                            "max".into(),
                            json!([mx[0] as f64, mx[1] as f64, mx[2] as f64]),
                        );
                    }
                }
                let new_acc_idx = accessors.len();
                accessors.push(J::Object(acc_obj));
                new_prim_attrs.insert(sem, json!(new_acc_idx));
            }
            prim.as_object_mut()
                .unwrap()
                .insert("attributes".into(), J::Object(new_prim_attrs));

            if let Some(exts) = prim.get_mut("extensions").and_then(|x| x.as_object_mut()) {
                exts.remove("KHR_draco_mesh_compression");
            }

            prim.as_object_mut()
                .unwrap()
                .insert("_abgen_from_draco".into(), json!(true));
        }
    }

    buffers[0].extend_from_slice(&new_bytes);

    if let Some(bufs) = gltf.get_mut("buffers").and_then(|x| x.as_array_mut()) {
        if let Some(b0) = bufs.get_mut(0).and_then(|x| x.as_object_mut()) {
            b0.insert("byteLength".into(), json!(buffers[0].len()));
        }
    }

    if let Some(obj) = gltf.as_object_mut() {
        obj.insert("accessors".into(), J::Array(accessors));
        obj.insert("bufferViews".into(), J::Array(buffer_views));
        obj.insert("meshes".into(), J::Array(meshes));

        for key in ["extensionsRequired", "extensionsUsed"] {
            if let Some(arr) = obj.get_mut(key).and_then(|x| x.as_array_mut()) {
                arr.retain(|v| v.as_str() != Some("KHR_draco_mesh_compression"));
            }
        }
    }

    Ok(())
}
