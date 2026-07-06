use anyhow::{bail, Result};
use serde_json::{json, Map, Value};

use crate::lodgen::model::{AlphaClass, LodModel};

fn align4(bin: &mut Vec<u8>) {
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
}

fn add_view(views: &mut Vec<Value>, offset: usize, len: usize, target: Option<u32>) -> usize {
    let mut v = Map::new();
    v.insert("buffer".to_string(), json!(0));
    v.insert("byteLength".to_string(), json!(len));
    v.insert("byteOffset".to_string(), json!(offset));
    if let Some(t) = target {
        v.insert("target".to_string(), json!(t));
    }
    views.push(Value::Object(v));
    views.len() - 1
}

pub fn emit_empty_glb(root_name: &str) -> Result<Vec<u8>> {
    let mut root = Map::new();
    root.insert(
        "asset".to_string(),
        json!({"generator": "abgen-lodgen", "version": "2.0"}),
    );
    root.insert("nodes".to_string(), json!([{"name": root_name}]));
    root.insert("scene".to_string(), json!(0));
    root.insert("scenes".to_string(), json!([{"nodes": [0]}]));
    let mut json_bytes = serde_json::to_vec(&Value::Object(root))?;
    while json_bytes.len() % 4 != 0 {
        json_bytes.push(b' ');
    }
    let total = 12 + 8 + json_bytes.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&json_bytes);
    Ok(out)
}

pub fn emit_glb(model: &LodModel) -> Result<Vec<u8>> {
    if model.primitives.is_empty() {
        bail!("emit_glb: model has no primitives");
    }
    let mut bin: Vec<u8> = Vec::new();
    let mut views: Vec<Value> = Vec::new();
    let mut accessors: Vec<Value> = Vec::new();
    let mut prims: Vec<Value> = Vec::new();

    for (pi, prim) in model.primitives.iter().enumerate() {
        if prim.positions.is_empty() || prim.indices.is_empty() {
            bail!("emit_glb: primitive {pi} is empty");
        }
        if prim.normals.len() != prim.positions.len() || prim.uvs.len() != prim.positions.len() {
            bail!("emit_glb: primitive {pi} attribute length mismatch");
        }
        if prim.indices.len() % 3 != 0 {
            bail!("emit_glb: primitive {pi} index count not a multiple of 3");
        }
        if prim.material >= model.materials.len() {
            bail!(
                "emit_glb: primitive {pi} references missing material {}",
                prim.material
            );
        }
        let max_idx = prim.indices.iter().copied().max().unwrap_or(0);
        if max_idx as usize >= prim.positions.len() {
            bail!("emit_glb: primitive {pi} index {max_idx} out of range");
        }

        align4(&mut bin);
        let pos_off = bin.len();
        let mut mn = [f32::INFINITY; 3];
        let mut mx = [f32::NEG_INFINITY; 3];
        for p in &prim.positions {
            for i in 0..3 {
                mn[i] = mn[i].min(p[i]);
                mx[i] = mx[i].max(p[i]);
                bin.extend_from_slice(&p[i].to_le_bytes());
            }
        }
        let pos_view = add_view(&mut views, pos_off, prim.positions.len() * 12, Some(34962));
        let pos_acc = accessors.len();
        accessors.push(json!({
            "bufferView": pos_view,
            "componentType": 5126,
            "count": prim.positions.len(),
            "type": "VEC3",
            "min": mn,
            "max": mx
        }));

        align4(&mut bin);
        let nrm_off = bin.len();
        for n in &prim.normals {
            for c in n {
                bin.extend_from_slice(&c.to_le_bytes());
            }
        }
        let nrm_view = add_view(&mut views, nrm_off, prim.normals.len() * 12, Some(34962));
        let nrm_acc = accessors.len();
        accessors.push(json!({
            "bufferView": nrm_view,
            "componentType": 5126,
            "count": prim.normals.len(),
            "type": "VEC3"
        }));

        align4(&mut bin);
        let uv_off = bin.len();
        for uv in &prim.uvs {
            for c in uv {
                bin.extend_from_slice(&c.to_le_bytes());
            }
        }
        let uv_view = add_view(&mut views, uv_off, prim.uvs.len() * 8, Some(34962));
        let uv_acc = accessors.len();
        accessors.push(json!({
            "bufferView": uv_view,
            "componentType": 5126,
            "count": prim.uvs.len(),
            "type": "VEC2"
        }));

        align4(&mut bin);
        let idx_off = bin.len();
        let (ctype, width) = if max_idx < 65535 {
            (5123u32, 2usize)
        } else {
            (5125u32, 4usize)
        };
        for &i in &prim.indices {
            if ctype == 5123 {
                bin.extend_from_slice(&(i as u16).to_le_bytes());
            } else {
                bin.extend_from_slice(&i.to_le_bytes());
            }
        }
        let idx_view = add_view(&mut views, idx_off, prim.indices.len() * width, Some(34963));
        let idx_acc = accessors.len();
        accessors.push(json!({
            "bufferView": idx_view,
            "componentType": ctype,
            "count": prim.indices.len(),
            "type": "SCALAR"
        }));

        prims.push(json!({
            "attributes": {"NORMAL": nrm_acc, "POSITION": pos_acc, "TEXCOORD_0": uv_acc},
            "indices": idx_acc,
            "material": prim.material
        }));
    }

    let mut images_json: Vec<Value> = Vec::new();
    for img in &model.images {
        align4(&mut bin);
        let off = bin.len();
        bin.extend_from_slice(&img.bytes);
        let view = add_view(&mut views, off, img.bytes.len(), None);
        images_json.push(json!({"bufferView": view, "mimeType": img.mime}));
    }

    let mut mats_json: Vec<Value> = Vec::new();
    for (mi, m) in model.materials.iter().enumerate() {
        if let Some(i) = m.image {
            if i >= model.images.len() {
                bail!("emit_glb: material {mi} references missing image {i}");
            }
        }
        let mut pbr = Map::new();
        pbr.insert("baseColorFactor".to_string(), json!(m.base_color));
        if let Some(i) = m.image {
            pbr.insert("baseColorTexture".to_string(), json!({"index": i}));
        }
        pbr.insert("metallicFactor".to_string(), json!(0.0));
        pbr.insert("roughnessFactor".to_string(), json!(1.0));
        let mut mat = Map::new();
        if m.class == AlphaClass::Mask {
            mat.insert("alphaCutoff".to_string(), json!(m.cutoff));
        }
        mat.insert("alphaMode".to_string(), json!(m.class.gltf_name()));
        if m.double_sided {
            mat.insert("doubleSided".to_string(), json!(true));
        }
        mat.insert("name".to_string(), json!(m.name));
        mat.insert("pbrMetallicRoughness".to_string(), Value::Object(pbr));
        mats_json.push(Value::Object(mat));
    }

    align4(&mut bin);

    let mut root = Map::new();
    root.insert("accessors".to_string(), Value::Array(accessors));
    root.insert(
        "asset".to_string(),
        json!({"generator": "abgen-lodgen", "version": "2.0"}),
    );
    root.insert("bufferViews".to_string(), Value::Array(views));
    root.insert("buffers".to_string(), json!([{"byteLength": bin.len()}]));
    if !images_json.is_empty() {
        root.insert("images".to_string(), Value::Array(images_json));
        root.insert(
            "samplers".to_string(),
            json!([{"wrapS": 10497, "wrapT": 10497}]),
        );
        root.insert(
            "textures".to_string(),
            Value::Array(
                (0..model.images.len())
                    .map(|i| json!({"sampler": 0, "source": i}))
                    .collect(),
            ),
        );
    }
    if !mats_json.is_empty() {
        root.insert("materials".to_string(), Value::Array(mats_json));
    }
    root.insert("meshes".to_string(), json!([{"primitives": prims}]));
    root.insert(
        "nodes".to_string(),
        json!([{"mesh": 0, "name": model.root_name}]),
    );
    root.insert("scene".to_string(), json!(0));
    root.insert("scenes".to_string(), json!([{"nodes": [0]}]));

    let mut json_bytes = serde_json::to_vec(&Value::Object(root))?;
    while json_bytes.len() % 4 != 0 {
        json_bytes.push(b' ');
    }

    let total = 12 + 8 + json_bytes.len() + 8 + bin.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&json_bytes);
    out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    out.extend_from_slice(&[0x42, 0x49, 0x4E, 0x00]);
    out.extend_from_slice(&bin);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lodgen::model::*;

    fn tiny_png() -> Vec<u8> {
        let mut img = image::RgbaImage::new(2, 2);
        img.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        img.put_pixel(1, 0, image::Rgba([0, 255, 0, 255]));
        img.put_pixel(0, 1, image::Rgba([0, 0, 255, 255]));
        img.put_pixel(1, 1, image::Rgba([255, 255, 0, 128]));
        let mut cur = std::io::Cursor::new(Vec::new());
        img.write_to(&mut cur, image::ImageFormat::Png).unwrap();
        cur.into_inner()
    }

    fn sample_model() -> LodModel {
        LodModel {
            root_name: "sample_root".to_string(),
            primitives: vec![
                LodPrimitive {
                    positions: vec![
                        [-1.5, 0.0, 2.0],
                        [1.0, 0.25, -3.5],
                        [0.5, 2.0, 0.75],
                        [-0.25, -1.0, 1.5],
                    ],
                    normals: vec![
                        [0.0, 0.0, 1.0],
                        [0.0, 1.0, 0.0],
                        [1.0, 0.0, 0.0],
                        [0.0, 0.0, -1.0],
                    ],
                    uvs: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.25, 0.75]],
                    indices: vec![0, 1, 2, 0, 2, 3],
                    material: 0,
                    ..Default::default()
                },
                LodPrimitive {
                    positions: vec![[10.0, 0.0, 0.0], [11.0, 0.0, 0.0], [10.0, 1.0, 0.0]],
                    normals: vec![[0.6, 0.8, 0.0]; 3],
                    uvs: vec![[0.5, 0.5], [0.75, 0.5], [0.5, 0.25]],
                    indices: vec![0, 1, 2],
                    material: 1,
                    ..Default::default()
                },
            ],
            materials: vec![
                LodMaterial {
                    name: "matA".to_string(),
                    class: AlphaClass::Opaque,
                    base_color: [0.25, 0.5, 0.75, 1.0],
                    cutoff: 0.5,
                    image: Some(0),
                    double_sided: false,
                },
                LodMaterial {
                    name: "matB".to_string(),
                    class: AlphaClass::Mask,
                    base_color: [1.0, 0.5, 0.25, 0.8],
                    cutoff: 0.7,
                    image: None,
                    double_sided: true,
                },
            ],
            images: vec![LodImage {
                bytes: tiny_png(),
                mime: "image/png".to_string(),
            }],
            log: Vec::new(),
        }
    }

    fn tri_model() -> LodModel {
        LodModel {
            root_name: "tri".to_string(),
            primitives: vec![LodPrimitive {
                positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                normals: vec![[0.6, 0.0, 0.8]; 3],
                uvs: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
                indices: vec![0, 1, 2],
                material: 0,
                ..Default::default()
            }],
            materials: vec![LodMaterial {
                name: "m".to_string(),
                class: AlphaClass::Opaque,
                base_color: [1.0, 1.0, 1.0, 1.0],
                cutoff: 0.5,
                image: None,
                double_sided: false,
            }],
            images: Vec::new(),
            log: Vec::new(),
        }
    }

    fn chunks(glb: &[u8]) -> (Value, Vec<u8>) {
        assert_eq!(&glb[0..4], b"glTF");
        let total = u32::from_le_bytes(glb[8..12].try_into().unwrap()) as usize;
        assert_eq!(total, glb.len());
        let jlen = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
        assert_eq!(&glb[16..20], b"JSON");
        let json: Value = serde_json::from_slice(&glb[20..20 + jlen]).unwrap();
        let bstart = 20 + jlen;
        let blen = u32::from_le_bytes(glb[bstart..bstart + 4].try_into().unwrap()) as usize;
        assert_eq!(&glb[bstart + 4..bstart + 8], &[0x42, 0x49, 0x4E, 0x00]);
        let bin = glb[bstart + 8..bstart + 8 + blen].to_vec();
        (json, bin)
    }

    fn rebuild(json: &Value, bin: &[u8]) -> Vec<u8> {
        let mut jb = serde_json::to_vec(json).unwrap();
        while !jb.len().is_multiple_of(4) {
            jb.push(b' ');
        }
        let mut bb = bin.to_vec();
        while !bb.len().is_multiple_of(4) {
            bb.push(0);
        }
        let total = 12 + 8 + jb.len() + 8 + bb.len();
        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&(jb.len() as u32).to_le_bytes());
        out.extend_from_slice(b"JSON");
        out.extend_from_slice(&jb);
        out.extend_from_slice(&(bb.len() as u32).to_le_bytes());
        out.extend_from_slice(&[0x42, 0x49, 0x4E, 0x00]);
        out.extend_from_slice(&bb);
        out
    }

    fn assert_vec3_close(a: &[[f32; 3]], b: &[[f32; 3]], eps: f32) {
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            for i in 0..3 {
                assert!(
                    (x[i] - y[i]).abs() <= eps,
                    "vec3 mismatch {:?} vs {:?}",
                    x,
                    y
                );
            }
        }
    }

    fn assert_vec2_close(a: &[[f32; 2]], b: &[[f32; 2]], eps: f32) {
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            for i in 0..2 {
                assert!(
                    (x[i] - y[i]).abs() <= eps,
                    "vec2 mismatch {:?} vs {:?}",
                    x,
                    y
                );
            }
        }
    }

    #[test]
    fn round_trip_two_primitives() {
        let model = sample_model();
        let glb = emit_glb(&model).unwrap();
        let back = from_glb_bytes(&glb, "sample_root").unwrap();
        assert_eq!(back.root_name, "sample_root");
        assert_eq!(back.primitives.len(), 2);
        for (a, b) in model.primitives.iter().zip(back.primitives.iter()) {
            assert_vec3_close(&a.positions, &b.positions, 1e-6);
            assert_vec3_close(&a.normals, &b.normals, 1e-6);
            assert_vec2_close(&a.uvs, &b.uvs, 1e-6);
            assert_eq!(a.indices, b.indices);
            assert_eq!(a.material, b.material);
        }
        assert_eq!(back.materials.len(), 2);
        for (a, b) in model.materials.iter().zip(back.materials.iter()) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.class, b.class);
            assert_eq!(a.base_color, b.base_color);
            assert_eq!(a.cutoff, b.cutoff);
            assert_eq!(a.image, b.image);
            assert_eq!(a.double_sided, b.double_sided);
        }
        assert_eq!(back.images.len(), 1);
        assert_eq!(back.images[0].bytes, model.images[0].bytes);
        assert_eq!(back.images[0].mime, "image/png");
    }

    fn wide_model(nverts: usize, tri: [u32; 3]) -> LodModel {
        LodModel {
            root_name: "wide".to_string(),
            primitives: vec![LodPrimitive {
                positions: (0..nverts)
                    .map(|i| [(i % 4096) as f32, (i / 4096) as f32, 0.0])
                    .collect(),
                normals: vec![[0.0, 0.0, 1.0]; nverts],
                uvs: vec![[0.0, 0.0]; nverts],
                indices: tri.to_vec(),
                material: 0,
                ..Default::default()
            }],
            materials: vec![LodMaterial {
                name: "m".to_string(),
                class: AlphaClass::Opaque,
                base_color: [1.0, 1.0, 1.0, 1.0],
                cutoff: 0.5,
                image: None,
                double_sided: false,
            }],
            images: Vec::new(),
            log: Vec::new(),
        }
    }

    fn index_component_type(glb: &[u8]) -> i64 {
        let (json, _) = chunks(glb);
        let acc = json["meshes"][0]["primitives"][0]["indices"]
            .as_i64()
            .unwrap();
        json["accessors"][acc as usize]["componentType"]
            .as_i64()
            .unwrap()
    }

    #[test]
    fn index_width_boundary() {
        let m16 = wide_model(65535, [0, 1, 65534]);
        let glb16 = emit_glb(&m16).unwrap();
        assert_eq!(index_component_type(&glb16), 5123);
        let back16 = from_glb_bytes(&glb16, "wide").unwrap();
        assert_eq!(back16.primitives[0].indices, vec![0, 1, 65534]);

        let m_restart = wide_model(65536, [0, 1, 65535]);
        let glb_restart = emit_glb(&m_restart).unwrap();
        assert_eq!(index_component_type(&glb_restart), 5125);
        let back_restart = from_glb_bytes(&glb_restart, "wide").unwrap();
        assert_eq!(back_restart.primitives[0].indices, vec![0, 1, 65535]);

        let m32 = wide_model(65537, [0, 1, 65536]);
        let glb32 = emit_glb(&m32).unwrap();
        assert_eq!(index_component_type(&glb32), 5125);
        let back32 = from_glb_bytes(&glb32, "wide").unwrap();
        assert_eq!(back32.primitives[0].indices, vec![0, 1, 65536]);
    }

    #[test]
    fn position_min_max() {
        let model = sample_model();
        let glb = emit_glb(&model).unwrap();
        let (json, _) = chunks(&glb);
        let acc = json["meshes"][0]["primitives"][0]["attributes"]["POSITION"]
            .as_i64()
            .unwrap() as usize;
        let mn: Vec<f64> = json["accessors"][acc]["min"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect();
        let mx: Vec<f64> = json["accessors"][acc]["max"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect();
        assert_eq!(mn, vec![-1.5, -1.0, -3.5]);
        assert_eq!(mx, vec![1.0, 2.0, 2.0]);
    }

    #[test]
    fn chunk_alignment_and_parse() {
        let glb = emit_glb(&sample_model()).unwrap();
        let total = u32::from_le_bytes(glb[8..12].try_into().unwrap()) as usize;
        assert_eq!(total, glb.len());
        let jlen = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
        assert_eq!(jlen % 4, 0);
        let bstart = 20 + jlen;
        let blen = u32::from_le_bytes(glb[bstart..bstart + 4].try_into().unwrap()) as usize;
        assert_eq!(blen % 4, 0);
        assert_eq!(bstart + 8 + blen, glb.len());
        let (json, _) = chunks(&glb);
        for bv in json["bufferViews"].as_array().unwrap() {
            assert_eq!(bv["byteOffset"].as_i64().unwrap() % 4, 0);
        }
        assert!(crate::gltf::parse(&glb, ".glb", None, false, true).is_ok());
    }

    #[test]
    fn zero_uv_fallback() {
        let glb = emit_glb(&sample_model()).unwrap();
        let (mut json, bin) = chunks(&glb);
        for prim in json["meshes"][0]["primitives"].as_array_mut().unwrap() {
            prim["attributes"]
                .as_object_mut()
                .unwrap()
                .remove("TEXCOORD_0");
        }
        let patched = rebuild(&json, &bin);
        let back = from_glb_bytes(&patched, "sample_root").unwrap();
        assert_eq!(back.primitives.len(), 2);
        for prim in &back.primitives {
            assert_eq!(prim.uvs.len(), prim.positions.len());
            for uv in &prim.uvs {
                assert_eq!(*uv, [0.0, 0.0]);
            }
        }
    }

    #[test]
    fn winding_flip_under_mirror() {
        let model = tri_model();
        let glb = emit_glb(&model).unwrap();
        let plain = from_glb_bytes(&glb, "tri").unwrap();
        assert_eq!(plain.primitives[0].indices, vec![0, 1, 2]);
        assert_vec3_close(
            &plain.primitives[0].positions,
            &model.primitives[0].positions,
            1e-6,
        );
        assert_vec3_close(
            &plain.primitives[0].normals,
            &model.primitives[0].normals,
            1e-6,
        );

        let (mut json, bin) = chunks(&glb);
        json["nodes"][0]["scale"] = json!([-1.0, 1.0, 1.0]);
        let patched = rebuild(&json, &bin);
        let mirrored = from_glb_bytes(&patched, "tri").unwrap();
        assert_eq!(mirrored.primitives[0].indices, vec![0, 2, 1]);
        let expected_pos: Vec<[f32; 3]> = model.primitives[0]
            .positions
            .iter()
            .map(|p| [-p[0], p[1], p[2]])
            .collect();
        assert_vec3_close(&mirrored.primitives[0].positions, &expected_pos, 1e-6);
        let expected_nrm: Vec<[f32; 3]> = model.primitives[0]
            .normals
            .iter()
            .map(|n| [-n[0], n[1], n[2]])
            .collect();
        assert_vec3_close(&mirrored.primitives[0].normals, &expected_nrm, 1e-6);
    }

    #[test]
    fn collider_subtree_zero_tris() {
        let glb = emit_glb(&tri_model()).unwrap();
        let plain = from_glb_bytes(&glb, "tri").unwrap();
        assert!(plain.total_tris() > 0);

        let (mut json, bin) = chunks(&glb);
        json["nodes"] = json!([
            {"children": [1], "name": "foo_collider"},
            {"mesh": 0, "name": "inner"}
        ]);
        json["scenes"] = json!([{"nodes": [0]}]);
        let patched = rebuild(&json, &bin);
        let back = from_glb_bytes(&patched, "tri").unwrap();
        assert_eq!(back.total_tris(), 0);
        assert!(back.primitives.is_empty());
    }

    #[test]
    fn deterministic_emit() {
        let a = emit_glb(&sample_model()).unwrap();
        let b = emit_glb(&sample_model()).unwrap();
        assert_eq!(a, b);
    }
}
