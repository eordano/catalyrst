use super::accessors::{
    jarr, jf, ji, js, read_accessor, read_attribute_accessor, read_morph_accessor, type_dim,
};
use super::load::{decode_data_uri, decode_image_rgba8_unity};
use super::transform::node_trs;
use super::Resolve;
use crate::mesh_layout;
use crate::scene::{
    AttrSig, Material, MorphSig, MorphTarget, Node, Primitive, Sampler, Scene, Skin, TexRef,
    TexTransform,
};
use crate::tangents::calculate_tangents;
use anyhow::Result;
use serde_json::Value as J;

pub(super) fn parse_impl(
    gltf: &J,
    buffers: &[Vec<u8>],
    resolve: Resolve,
    magenta_missing: bool,
    normalized_attribute_scaling: bool,
    classify: bool,
) -> Result<Scene> {
    let empty_vec: Vec<J> = Vec::new();

    let buffer_views = jarr(gltf, "bufferViews").cloned().unwrap_or_default();
    let mut images: Vec<Option<image::RgbaImage>> = Vec::new();
    let mut image_embedded: Vec<bool> = Vec::new();
    let mut image_bytes: Vec<Option<Vec<u8>>> = Vec::new();
    let mut image_uri: Vec<Option<String>> = Vec::new();
    for img in jarr(gltf, "images").unwrap_or(&empty_vec) {
        let has_uri = js(img, "uri").is_some();
        let bv_idx = ji(img, "bufferView");
        let embedded = bv_idx.is_some() && !has_uri;

        let external_uri: Option<String> = match js(img, "uri") {
            Some(u) if bv_idx.is_none() && !u.starts_with("data:") => Some(u.to_string()),
            _ => None,
        };
        let raw: Option<Vec<u8>> = if classify {
            None
        } else if let Some(bvi) = bv_idx {
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

    let raw_samplers = jarr(gltf, "samplers").cloned().unwrap_or_default();
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
    for tex in jarr(gltf, "textures").unwrap_or(&empty_vec) {
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

    let textures = jarr(gltf, "textures").cloned().unwrap_or_default();

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
            let cos = crate::detmath::cosf(rot);
            let sin = crate::detmath::sinf(rot);
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
    for (i, m) in jarr(gltf, "materials")
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

    let meshes = jarr(gltf, "meshes").cloned().unwrap_or_default();

    let build_primitives = |mesh_idx: i64,
                            mesh_name: &str,
                            skin_index: Option<usize>|
     -> Vec<Primitive> {
        let mut prims = Vec::new();
        if classify {
            return prims;
        }
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
            let positions: Vec<[f64; 3]> = read_accessor(gltf, buffers, pos_acc)
                .iter()
                .map(|p| [-p[0], p[1], p[2]])
                .collect();
            let nverts = positions.len();

            let (position_min_decl, position_max_decl) = {
                let accessors = jarr(gltf, "accessors").expect("accessors");
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
                Some(a) => read_attribute_accessor(gltf, buffers, a, normalized_attribute_scaling)
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
                        let uv: Vec<[f64; 2]> =
                            read_attribute_accessor(gltf, buffers, a, normalized_attribute_scaling)
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
                read_attribute_accessor(gltf, buffers, a, normalized_attribute_scaling)
                    .iter()
                    .map(|t| [t[0], t[1], -t[2], t[3]])
                    .collect()
            });

            let colors: Option<Vec<[f64; 4]>> = ji(attrs, "COLOR_0").map(|a| {
                let accessors = jarr(gltf, "accessors").unwrap();
                let acc = &accessors[a as usize];
                let dim = type_dim(js(acc, "type").unwrap());
                let ct = ji(acc, "componentType").unwrap() as i32;
                read_accessor(gltf, buffers, a)
                    .iter()
                    .map(|c| mesh_layout::normalize_color(c, ct, dim))
                    .collect()
            });

            let mut weights: Option<Vec<[f64; 4]>> = None;
            let mut joints: Option<Vec<[u32; 4]>> = None;
            if let (Some(wa), Some(ja)) = (ji(attrs, "WEIGHTS_0"), ji(attrs, "JOINTS_0")) {
                let accessors = jarr(gltf, "accessors").unwrap();
                let wct = ji(&accessors[wa as usize], "componentType").unwrap() as i32;
                let raw_w: Vec<[f64; 4]> = read_accessor(gltf, buffers, wa)
                    .iter()
                    .map(|w| mesh_layout::normalize_weights(w, wct))
                    .collect();
                let raw_j: Vec<[u32; 4]> = read_accessor(gltf, buffers, ja)
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
                Some(a) => read_accessor(gltf, buffers, a)
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
                        read_morph_accessor(gltf, buffers, a, nverts, 3)
                            .into_iter()
                            .map(|p| [-p[0], p[1], p[2]])
                            .collect::<Vec<[f64; 3]>>()
                    });

                    let positions = pos.unwrap_or_else(|| vec![[0.0; 3]; nverts]);
                    let normals: Option<Vec<[f64; 3]>> = ji(t, "NORMAL").map(|a| {
                        read_morph_accessor(gltf, buffers, a, nverts, 3)
                            .into_iter()
                            .map(|n| [-n[0], n[1], n[2]])
                            .collect()
                    });
                    let tangents: Option<Vec<[f64; 3]>> = ji(t, "TANGENT").map(|a| {
                        read_morph_accessor(gltf, buffers, a, nverts, 3)
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

    let json_nodes = jarr(gltf, "nodes").cloned().unwrap_or_default();
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
    for sk in jarr(gltf, "skins").unwrap_or(&empty_vec) {
        let sk_joints: Vec<usize> = jarr(sk, "joints")
            .map(|a| a.iter().map(|v| v.as_i64().unwrap() as usize).collect())
            .unwrap_or_default();
        let bind_poses: Vec<[f64; 16]> = match ji(sk, "inverseBindMatrices") {
            Some(a) => read_accessor(gltf, buffers, a)
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

    let scenes = jarr(gltf, "scenes").cloned().unwrap_or_default();
    let scene_idx = ji(gltf, "scene").unwrap_or(0);

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

    let has_animation = jarr(gltf, "animations").is_some_and(|a| !a.is_empty());

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
