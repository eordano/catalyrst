use super::templates::template_path;
use super::texture::apply_png_gamma;
use super::texture::encode_dxt5_mip_chain_real;
use super::texture::encode_standalone_dxt5;
use super::texture::encode_texture_bc7;
use super::texture::looks_like_normal_map;
use super::texture::pack_normal_map;
use super::texture::png_gamma_to_apply;
use super::texture::standalone_texture_readable;
use super::*;
use crate::unity::bundle_file::{Bundle as ReadBundle, FileContent};

#[test]
fn lowercased_lod_root_hashes_emit_metadata_textasset() {
    assert!(emits_metadata_textasset(
        "qmccggwqvb7v3b3vqxajzcjimmzhzrrvmk3ulkt6qxsesd_1",
        false
    ));
    assert!(emits_metadata_textasset("bafkreifz6o7w_1", false));
    assert!(!emits_metadata_textasset("QmScene", false));
    assert!(emits_metadata_textasset("QmScene", true));
}

#[test]
fn templates_missing_reports_every_absent_required_template() {
    let dir = std::env::temp_dir().join(format!("abgen_tmpl_missing_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let missing = templates_missing_in(&dir);
    assert_eq!(missing.len(), REQUIRED_TEMPLATES.len());
    for f in REQUIRED_TEMPLATES {
        assert!(missing.contains(&f.to_string()), "{f} should be reported");
    }

    std::fs::write(dir.join("all-types.windows.bundle"), b"stub").unwrap();
    let missing = templates_missing_in(&dir);
    assert_eq!(
        missing,
        vec![
            "animated-types.windows.bundle".to_string(),
            "emote-types.windows.bundle".to_string(),
            "skinned-types.windows.bundle".to_string(),
        ]
    );

    for f in REQUIRED_TEMPLATES {
        std::fs::write(dir.join(f), b"stub").unwrap();
    }
    assert!(templates_missing_in(&dir).is_empty());

    std::fs::remove_file(dir.join("emote-types.windows.bundle")).unwrap();
    std::fs::create_dir_all(dir.join("emote-types.windows.bundle")).unwrap();
    assert_eq!(
        templates_missing_in(&dir),
        vec!["emote-types.windows.bundle".to_string()]
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bc7_slot_normal_with_alpha_needs_override_to_swizzle() {
    let img = RgbaImage::from_fn(8, 8, |x, _| {
        image::Rgba([128 + (x as u8 % 4) * 8, 128, 255, 200])
    });
    assert!(!looks_like_normal_map(img.as_raw()));

    let (heur, _) = encode_texture_bc7(&img, 1, false, None, bc7_pure::Bc7Profile::Basic);
    let (forced, _) = encode_texture_bc7(&img, 1, false, Some(true), bc7_pure::Bc7Profile::Basic);
    assert_ne!(heur, forced, "heuristic missed the slot-normal swizzle");

    let packed = RgbaImage::from_raw(8, 8, pack_normal_map(img.as_raw())).expect("packed image");
    let (manual, _) =
        encode_texture_bc7(&packed, 1, false, Some(false), bc7_pure::Bc7Profile::Basic);
    assert_eq!(forced, manual, "override must apply exactly the DXTnm pack");
}

#[test]
fn bc7_orm_false_positive_needs_override_to_stay_plain() {
    let img = RgbaImage::from_pixel(8, 8, image::Rgba([0, 137, 158, 255]));
    assert!(
        looks_like_normal_map(img.as_raw()),
        "precondition: heuristic misclassifies this ORM as a normal map"
    );

    let (heur, _) = encode_texture_bc7(&img, 1, false, None, bc7_pure::Bc7Profile::Basic);
    let (slot, _) = encode_texture_bc7(&img, 1, false, Some(false), bc7_pure::Bc7Profile::Basic);
    assert_ne!(
        heur, slot,
        "slot-driven encode must not apply the false-positive swizzle"
    );
}

fn decode_bc3_level(data: &[u8], offset: usize, w: usize, h: usize) -> Vec<u8> {
    let size = texpresso::Format::Bc3.compressed_size(w, h);
    let mut out = vec![0u8; w * h * 4];
    texpresso::Format::Bc3.decompress(&data[offset..offset + size], w, h, &mut out);
    out
}

#[test]
fn dxt5_mip1_of_srgb_gradient_is_linear_filtered() {
    let img = RgbaImage::from_fn(2, 2, |x, y| {
        let v = if (x + y) % 2 == 0 { 0 } else { 255 };
        image::Rgba([v, v, v, 255])
    });
    let (data, mips) = encode_dxt5_mip_chain_real(&img, 2, true);
    assert_eq!(mips, 2);

    let mip0_size = texpresso::Format::Bc3.compressed_size(2, 2);
    let px = decode_bc3_level(&data, mip0_size, 1, 1);
    for c in 0..3 {
        assert!(
            px[c] > 160,
            "mip1 channel {c} = {} — gamma-space halving (expected ~188, \
                 gamma would give ~128)",
            px[c]
        );
        assert!(
            (px[c] as i32 - 188).abs() <= 8,
            "mip1 channel {c} = {} not within BC3 quantization of the \
                 linear-filtered value 188",
            px[c]
        );
    }
    assert_eq!(px[3], 255, "alpha is linear data, stays 255");
}

#[test]
fn dxt5_mip1_of_linear_data_is_value_filtered() {
    let img = RgbaImage::from_fn(2, 2, |x, y| {
        let v = if (x + y) % 2 == 0 { 0 } else { 255 };
        image::Rgba([v, v, v, 255])
    });
    let (data, _) = encode_dxt5_mip_chain_real(&img, 2, false);
    let mip0_size = texpresso::Format::Bc3.compressed_size(2, 2);
    let px = decode_bc3_level(&data, mip0_size, 1, 1);
    for c in 0..3 {
        assert!(
            (px[c] as i32 - 128).abs() <= 8,
            "non-sRGB mip1 channel {c} = {} should be the value-space \
                 average ~128",
            px[c]
        );
    }
}

#[test]
fn dxt5_mip0_survives_the_linear_roundtrip() {
    let img = RgbaImage::from_fn(4, 4, |x, y| {
        image::Rgba([(x * 60) as u8, (y * 60) as u8, 200, 100 + (x * 30) as u8])
    });
    let (w, h) = (4usize, 4usize);
    let src = img.as_raw();
    let mut flipped = vec![0u8; w * h * 4];
    for y in 0..h {
        flipped[y * w * 4..(y + 1) * w * 4]
            .copy_from_slice(&src[(h - 1 - y) * w * 4..(h - y) * w * 4]);
    }
    let params = texpresso::Params {
        algorithm: texpresso::Algorithm::IterativeClusterFit,
        weights: texpresso::COLOUR_WEIGHTS_PERCEPTUAL,
        weigh_colour_by_alpha: false,
    };
    let size = texpresso::Format::Bc3.compressed_size(w, h);
    let mut direct = vec![0u8; size];
    texpresso::Format::Bc3.compress(&flipped, w, h, params, &mut direct);

    for srgb in [true, false] {
        let (data, _) = encode_dxt5_mip_chain_real(&img, 1, srgb);
        assert_eq!(
            data, direct,
            "mip0 changed by the linear round trip (srgb={srgb})"
        );
    }
}

fn webgl_standalone_profile(usage_normal: Option<bool>) -> texprofile::Profile {
    let src = texprofile::SourceImage {
        width: 8,
        height: 8,
        container: "PNG".to_string(),
        has_real_alpha: false,
    };
    let mut prof = texprofile::standalone_texture_profile_named(&src, 1024, usage_normal);
    prof.texture_format = texprofile::TF_DXT5;
    prof
}

#[test]
fn standalone_dxt5_normal_map_gets_dxtnm_swizzle() {
    let img = RgbaImage::from_pixel(8, 8, image::Rgba([128, 128, 255, 255]));
    let prof = webgl_standalone_profile(Some(true));
    assert_eq!(prof.color_space, 0);
    assert_eq!(prof.lightmap_format, 3);

    let (data, _) = encode_standalone_dxt5(&img, &prof, Some(true));
    let px = decode_bc3_level(&data, 0, 8, 8);
    assert!(
        px[0] >= 250,
        "R must be 1.0 after the swizzle, got {}",
        px[0]
    );
    assert!(
        (px[1] as i32 - 128).abs() <= 8 && (px[2] as i32 - 128).abs() <= 8,
        "G/B must carry g=128, got {}/{}",
        px[1],
        px[2]
    );
    assert!(
        (px[3] as i32 - 128).abs() <= 8,
        "A must carry r=128, got {}",
        px[3]
    );

    let packed = RgbaImage::from_raw(8, 8, pack_normal_map(img.as_raw())).expect("packed");
    let (manual, _) = encode_dxt5_mip_chain_real(&packed, prof.mip_count, false);
    assert_eq!(data, manual, "swizzle must be exactly pack_normal_map");
}

#[test]
fn standalone_dxt5_non_normal_stays_plain() {
    let img = RgbaImage::from_pixel(8, 8, image::Rgba([128, 128, 255, 255]));
    let prof = webgl_standalone_profile(None);
    assert_eq!(prof.color_space, 1);

    let (data, _) = encode_standalone_dxt5(&img, &prof, None);
    let px = decode_bc3_level(&data, 0, 8, 8);
    assert!(
        px[2] >= 250,
        "plain base-color encode must keep blue=255, got {}",
        px[2]
    );
    assert_eq!(px[3], 255, "plain encode keeps source alpha");
}

#[test]
fn standalone_dxt5_unknown_usage_mirrors_bc7_fallback() {
    let img = RgbaImage::from_pixel(8, 8, image::Rgba([128, 128, 255, 255]));
    assert!(looks_like_normal_map(img.as_raw()));
    let mut prof = webgl_standalone_profile(Some(true));
    prof.color_space = 0;

    let (fallback, _) = encode_standalone_dxt5(&img, &prof, None);
    let (forced, _) = encode_standalone_dxt5(&img, &prof, Some(true));
    assert_eq!(
        fallback, forced,
        "heuristic must fire on linear normal-looking data"
    );

    let mut srgb_prof = webgl_standalone_profile(Some(true));
    srgb_prof.color_space = 1;
    let (srgb_none, _) = encode_standalone_dxt5(&img, &srgb_prof, None);
    let (srgb_plain, _) = encode_standalone_dxt5(&img, &srgb_prof, Some(false));
    assert_eq!(
        srgb_none, srgb_plain,
        "the fallback heuristic must be gated off for sRGB textures"
    );
}

#[test]
fn standalone_readable_is_decoupled_from_streaming() {
    assert!(!standalone_texture_readable(true, true));
    assert!(standalone_texture_readable(false, true));
    assert!(standalone_texture_readable(true, false));
    assert!(standalone_texture_readable(false, false));

    for target in ["windows", "mac", "linux"] {
        for model_referenced in [false, true] {
            for (fmt, compressed) in [
                (texprofile::TF_BC7, true),
                (texprofile::TF_RGBA32_UNITY, false),
            ] {
                let do_stream = target != "webgl" && model_referenced && fmt == 25;
                assert_eq!(
                    standalone_texture_readable(model_referenced, compressed),
                    !do_stream,
                    "desktop readable must match the pre-D7 rule \
                         (target={target}, model_referenced={model_referenced}, fmt={fmt})"
                );
            }
        }
    }
}

fn png_with_chunks(extra: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut push = |typ: &[u8; 4], body: &[u8]| {
        v.extend_from_slice(&(body.len() as u32).to_be_bytes());
        v.extend_from_slice(typ);
        v.extend_from_slice(body);
        v.extend_from_slice(&[0, 0, 0, 0]);
    };

    push(b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0]);
    for (t, b) in extra {
        push(t, b);
    }
    push(b"IDAT", &[]);
    push(b"IEND", &[]);
    v
}

#[test]
fn png_gamma_gate_fires_only_on_nontrivial_gama_without_srgb() {
    let nontrivial = 55531u32.to_be_bytes().to_vec();
    let trivial = 45455u32.to_be_bytes().to_vec();

    let png = png_with_chunks(&[(b"gAMA", nontrivial.clone())]);
    assert_eq!(png_gamma_to_apply(&png), Some(55531));

    let png = png_with_chunks(&[(b"gAMA", trivial)]);
    assert_eq!(png_gamma_to_apply(&png), None);

    let png = png_with_chunks(&[(b"sRGB", vec![0]), (b"gAMA", nontrivial)]);
    assert_eq!(png_gamma_to_apply(&png), None);

    let png = png_with_chunks(&[
        (b"iCCP", b"Adobe RGB (1998)\0\0".to_vec()),
        (b"cHRM", vec![0; 32]),
    ]);
    assert_eq!(png_gamma_to_apply(&png), None);
}

#[test]
fn png_gamma_lut_matches_freeimage_curve() {
    let mut img = RgbaImage::from_pixel(4, 1, image::Rgba([0, 0, 0, 255]));
    let vals = [28u8, 64, 128, 192, 255];

    let expect = [42u8, 82, 145, 202, 255];
    for (&v, &e) in vals.iter().zip(expect.iter()) {
        let mut one = RgbaImage::from_pixel(1, 1, image::Rgba([v, v, v, 200]));
        apply_png_gamma(&mut one, 55531);
        let p = one.get_pixel(0, 0);
        assert_eq!(p[0], e, "in={v}");
        assert_eq!(p[3], 200, "alpha untouched");
    }

    apply_png_gamma(&mut img, 55531);
    assert_eq!(img.get_pixel(0, 0)[0], 0);
}

#[test]
fn natural_bundle_cmp_orders_digit_runs_numerically() {
    use std::cmp::Ordering;

    assert_eq!(
        natural_bundle_cmp(
            "bafkreig7pqew5umjh46onc3zowyub2pkjoikltldxtxi26rnists3k3rdm_windows",
            "bafkreig42hknvr5derr24elh4l3uxwnsef6ddvzcfv7x2ys64goj4ov6vy_windows",
        ),
        Ordering::Less
    );

    assert_eq!(natural_bundle_cmp("abc", "abd"), Ordering::Less);
    assert_eq!(natural_bundle_cmp("abc", "abc"), Ordering::Equal);
    assert_eq!(natural_bundle_cmp("ab", "abc"), Ordering::Less);

    assert_eq!(natural_bundle_cmp("a4x", "abx"), Ordering::Less);

    assert_eq!(natural_bundle_cmp("a7b", "a07b"), Ordering::Less);

    assert_eq!(natural_bundle_cmp("a42b", "a42c"), Ordering::Less);
    assert_eq!(natural_bundle_cmp("a42b", "a43a"), Ordering::Less);
}

fn tiny_gltf_with_buffer(n_materials: usize, buffer_json: &str) -> Vec<u8> {
    let mats: Vec<String> = (0..n_materials)
        .map(|i| format!("{{\"name\":\"mat_{i}\",\"pbrMetallicRoughness\":{{}}}}"))
        .collect();
    let mat_block = if mats.is_empty() {
        String::new()
    } else {
        format!(",\"materials\":[{}]", mats.join(","))
    };
    let mat_ref = if n_materials > 0 {
        ",\"material\":0"
    } else {
        ""
    };
    format!(
            "{{\"asset\":{{\"version\":\"2.0\"}},\
             \"scene\":0,\"scenes\":[{{\"nodes\":[0]}}],\
             \"nodes\":[{{\"mesh\":0,\"name\":\"tri\"}}],\
             \"meshes\":[{{\"primitives\":[{{\"attributes\":{{\"POSITION\":0}},\"indices\":1{mat_ref}}}]}}]\
             {mat_block},\
             \"accessors\":[\
               {{\"bufferView\":0,\"componentType\":5126,\"count\":3,\"type\":\"VEC3\",\
                 \"min\":[0,0,0],\"max\":[1,1,0]}},\
               {{\"bufferView\":1,\"componentType\":5123,\"count\":3,\"type\":\"SCALAR\"}}],\
             \"bufferViews\":[\
               {{\"buffer\":0,\"byteOffset\":0,\"byteLength\":36}},\
               {{\"buffer\":0,\"byteOffset\":36,\"byteLength\":6}}],\
             \"buffers\":[{buffer_json}]}}"
        )
        .into_bytes()
}

fn tiny_gltf(n_materials: usize) -> Vec<u8> {
    const BUF_B64: &str = "AAAAAAAAAAAAAAAAAACAPwAAAAAAAAAAAAAAAAAAgD8AAAAAAAABAAIA";
    tiny_gltf_with_buffer(
        n_materials,
        &format!(
            "{{\"byteLength\":42,\"uri\":\"data:application/octet-stream;base64,{BUF_B64}\"}}"
        ),
    )
}

struct BundleProbe {
    dcl_scene_materials: usize,
    material_names: Vec<String>,

    dcl_scene_container: Option<usize>,

    renderer_mat_pids: Vec<i64>,
    dcl_scene_pid: Option<i64>,
    keywords_empty: bool,
    mesh_index_nonzero: bool,
}

fn probe(data: &[u8]) -> BundleProbe {
    let b = ReadBundle::load_bytes(data).expect("bundle parses");
    let mut p = BundleProbe {
        dcl_scene_materials: 0,
        material_names: Vec::new(),
        dcl_scene_container: None,
        renderer_mat_pids: Vec::new(),
        dcl_scene_pid: None,
        keywords_empty: false,
        mesh_index_nonzero: false,
    };
    for f in &b.files {
        let FileContent::Serialized(sf) = &f.content else {
            continue;
        };
        for o in &sf.objects {
            match o.class_id {
                21 => {
                    let v = sf.read_typetree(o).unwrap();
                    let name = v
                        .get("m_Name")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string();
                    if name == "DCL_Scene" {
                        p.dcl_scene_materials += 1;
                        p.dcl_scene_pid = Some(o.path_id);
                        let empty =
                            |k: &str| matches!(v.get(k), Some(Value::Array(a)) if a.is_empty());
                        p.keywords_empty = empty("m_ValidKeywords") && empty("m_InvalidKeywords");
                    }
                    p.material_names.push(name);
                }
                23 | 137 => {
                    let v = sf.read_typetree(o).unwrap();
                    if let Some(Value::Array(mats)) = v.get("m_Materials") {
                        for m in mats {
                            let fid = m.get("m_FileID").and_then(|x| x.as_i64()).unwrap_or(0);
                            let pid = m.get("m_PathID").and_then(|x| x.as_i64()).unwrap_or(0);
                            if fid == 0 {
                                p.renderer_mat_pids.push(pid);
                            }
                        }
                    }
                }
                43 => {
                    let v = sf.read_typetree(o).unwrap();
                    match v.get("m_IndexBuffer") {
                        Some(Value::Bytes(b)) if b.iter().any(|&x| x != 0) => {
                            p.mesh_index_nonzero = true;
                        }
                        Some(Value::Array(a)) if a.iter().any(|x| x.as_i64().unwrap_or(0) != 0) => {
                            p.mesh_index_nonzero = true;
                        }
                        _ => {}
                    }
                }
                142 => {
                    let v = sf.read_typetree(o).unwrap();
                    if let Some(Value::Array(cont)) = v.get("m_Container") {
                        for e in cont {
                            let Value::Array(pair) = e else { continue };
                            if pair.len() == 2 && pair[0].as_str() == Some("DCL_Scene.mat") {
                                let sz = pair[1]
                                    .get("preloadSize")
                                    .and_then(|x| x.as_i64())
                                    .unwrap_or(-1);
                                p.dcl_scene_container = Some(sz as usize);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    p
}

fn build_tiny(n_materials: usize) -> BundleProbe {
    build_tiny_toggles(n_materials, false)
}

fn build_tiny_toggles(n_materials: usize, v38_compat: bool) -> BundleProbe {
    let gltf = tiny_gltf(n_materials);
    let opts = BuildOpts {
        source_file: Some("test.gltf"),
        v38_compat,
        v38_timestamp: 0,
        ..BuildOpts::default()
    };
    let art =
        build_bundle(&gltf, "QmTestTinyTri_windows", "QmTestTinyTri", &opts).expect("build_bundle");
    probe(&art.data)
}

#[test]
fn v38_compat_dcl_scene_default_material() {
    if !template_path().exists() {
        eprintln!(
            "skipping: template bundle not found at {}",
            template_path().display()
        );
        return;
    }
    let off = build_tiny(1);
    assert_eq!(off.dcl_scene_materials, 0);
    assert_eq!(off.material_names, vec!["material_0".to_string()]);
    assert_eq!(off.dcl_scene_container, None);
    let off_renderer_mats = off.renderer_mat_pids.clone();

    let off_zero = build_tiny(0);
    assert_eq!(off_zero.dcl_scene_materials, 1);
    assert_eq!(off_zero.material_names, vec!["DCL_Scene".to_string()]);
    let off_dz = off_zero.dcl_scene_pid.unwrap();
    assert!(!off_zero.renderer_mat_pids.is_empty());
    assert!(off_zero.renderer_mat_pids.iter().all(|&p| p == off_dz));

    let on = build_tiny_toggles(1, true);
    let on_zero = build_tiny_toggles(0, true);

    assert_eq!(on.dcl_scene_materials, 1);
    assert_eq!(on.material_names.len(), 2);
    assert!(on.keywords_empty);
    assert_eq!(on.dcl_scene_container, Some(2));

    assert_eq!(on.renderer_mat_pids.len(), off_renderer_mats.len());
    let ds = on.dcl_scene_pid.unwrap();
    assert!(on.renderer_mat_pids.iter().all(|&p| p != ds));

    assert_eq!(on_zero.dcl_scene_materials, 1);
    assert_eq!(on_zero.material_names, vec!["DCL_Scene".to_string()]);
    assert_eq!(on_zero.dcl_scene_container, Some(2));
    let dz = on_zero.dcl_scene_pid.unwrap();
    assert!(!on_zero.renderer_mat_pids.is_empty());
    assert!(
        on_zero.renderer_mat_pids.iter().all(|&p| p == dz),
        "zero-material renderers must reference the DCL_Scene default material, \
             not a null PPtr (L2 magenta InternalErrorShader)"
    );
}

fn build_tiny_force_dcl_scene(n_materials: usize) -> BundleProbe {
    let gltf = tiny_gltf(n_materials);
    let opts = BuildOpts {
        source_file: Some("test.gltf"),
        force_default_material: true,
        ..BuildOpts::default()
    };
    let art = build_bundle(&gltf, "QmTestForceDcl_windows", "QmTestForceDcl", &opts)
        .expect("build_bundle");
    probe(&art.data)
}

#[test]
fn force_default_material_emits_dcl_scene_without_v38() {
    if !template_path().exists() {
        eprintln!(
            "skipping: template bundle not found at {}",
            template_path().display()
        );
        return;
    }

    let on = build_tiny_force_dcl_scene(1);
    assert_eq!(on.dcl_scene_materials, 1);
    assert_eq!(on.material_names.len(), 2);
    assert_eq!(on.dcl_scene_container, Some(2));
    let ds = on.dcl_scene_pid.unwrap();
    assert!(on.renderer_mat_pids.iter().all(|&p| p != ds));

    let on_zero = build_tiny_force_dcl_scene(0);
    assert_eq!(on_zero.dcl_scene_materials, 1);
    assert_eq!(on_zero.material_names, vec!["DCL_Scene".to_string()]);
    assert_eq!(on_zero.dcl_scene_container, Some(2));
    let dz = on_zero.dcl_scene_pid.unwrap();
    assert!(!on_zero.renderer_mat_pids.is_empty());
    assert!(on_zero.renderer_mat_pids.iter().all(|&p| p == dz));
}

#[test]
fn external_bin_buffer_builds_real_geometry_or_fails() {
    if !template_path().exists() {
        eprintln!(
            "skipping: template bundle not found at {}",
            template_path().display()
        );
        return;
    }

    let gltf = tiny_gltf_with_buffer(0, "{\"byteLength\":42,\"uri\":\"tri.bin\"}");
    let mut bin: Vec<u8> = Vec::new();
    for f in [0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
        bin.extend_from_slice(&f.to_le_bytes());
    }
    for i in [0u16, 1, 2] {
        bin.extend_from_slice(&i.to_le_bytes());
    }

    let resolver = move |uri: &str| -> Option<Vec<u8>> { (uri == "tri.bin").then(|| bin.clone()) };
    let opts = BuildOpts {
        source_file: Some("test.gltf"),
        resolve: Some(&resolver),
        ..BuildOpts::default()
    };
    let art = build_bundle(&gltf, "QmTestExtBin_windows", "QmTestExtBin", &opts)
        .expect("build with resolved .bin");
    let p = probe(&art.data);
    assert!(
        p.mesh_index_nonzero,
        "mesh index buffer must carry the real indices, not zero-fill"
    );

    let opts = BuildOpts {
        source_file: Some("test.gltf"),
        ..BuildOpts::default()
    };
    let err = build_bundle(&gltf, "QmTestExtBin_windows", "QmTestExtBin", &opts)
        .expect_err("missing .bin must fail the build");
    assert!(format!("{err:#}").contains("tri.bin"), "{err:#}");
}

fn count_anim_classes(data: &[u8]) -> (usize, usize, usize) {
    let b = ReadBundle::load_bytes(data).expect("bundle parses");
    let (mut animators, mut controllers, mut clips) = (0usize, 0usize, 0usize);
    for f in &b.files {
        let FileContent::Serialized(sf) = &f.content else {
            continue;
        };
        for o in &sf.objects {
            match o.class_id {
                95 => animators += 1,
                91 => controllers += 1,
                74 => clips += 1,
                _ => {}
            }
        }
    }
    (animators, controllers, clips)
}

#[test]
fn text_gltf_emote_emits_animator_and_controller() {
    if !template_path().exists() {
        eprintln!(
            "skipping: template bundle not found at {}",
            template_path().display()
        );
        return;
    }

    let gltf = br#"{
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [{"nodes": [0]}],
            "nodes": [{"name": "Armature", "children": [1]}, {"name": "Bone"}],
            "animations": [{
                "name": "TestEmote",
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
            "buffers": [{"byteLength": 32, "uri": "data:application/octet-stream;base64,AAAAAAAAgD8AAAAAAAAAAAAAAAAAAIA/AAAAQAAAQEA="}]
        }"#;

    let opts = BuildOpts {
        source_file: Some("male/test.gltf"),
        entity_type: Some("emote"),
        ..BuildOpts::default()
    };
    let art = build_bundle(gltf, "QmTestGltfEmote_windows", "QmTestGltfEmote", &opts)
        .expect("text .gltf emote builds");
    let (animators, controllers, clips) = count_anim_classes(&art.data);
    assert_eq!(animators, 1, "text .gltf emote must carry an Animator");
    assert_eq!(controllers, 1, "…and an AnimatorController");
    assert!(clips >= 1, "…and at least one AnimationClip");
}

#[test]
fn build_bundle_multi_pair_matches_single_platform_builds() {
    if !template_path().exists() {
        eprintln!(
            "skipping: template bundle not found at {}",
            template_path().display()
        );
        return;
    }
    let gltf = tiny_gltf(1);
    let opts = BuildOpts {
        source_file: Some("test.gltf"),
        v38_compat: true,
        v38_timestamp: 638_000_000_000_000_000,
        ..BuildOpts::default()
    };
    for names in [
        ["QmTestMulti_windows", "QmTestMulti_mac"],
        ["QmTestMulti_mac", "QmTestMulti_windows"],
    ] {
        let names: Vec<String> = names.iter().map(|s| s.to_string()).collect();
        let multi = build_bundle_multi(&gltf, &names, "QmTestMulti", &opts).expect("multi");
        assert_eq!(multi.len(), 2);
        assert_ne!(multi[0].data, multi[1].data);
        for (art, name) in multi.iter().zip(names.iter()) {
            let single = build_bundle(&gltf, name, "QmTestMulti", &opts).expect("single");
            assert_eq!(
                art.data, single.data,
                "{name}: encode-once serialize must be byte-identical to a fresh build"
            );
        }
    }
}

#[test]
fn build_bundle_multi_non_shareable_targets_fall_back_to_full_builds() {
    if !template_path().exists() {
        eprintln!(
            "skipping: template bundle not found at {}",
            template_path().display()
        );
        return;
    }
    let gltf = tiny_gltf(1);
    let opts = BuildOpts {
        source_file: Some("test.gltf"),
        ..BuildOpts::default()
    };
    let names = vec![
        "QmTestMultiFb_windows".to_string(),
        "QmTestMultiFb_linux".to_string(),
    ];
    let multi = build_bundle_multi(&gltf, &names, "QmTestMultiFb", &opts).expect("multi");
    for (art, name) in multi.iter().zip(names.iter()) {
        let single = build_bundle(&gltf, name, "QmTestMultiFb", &opts).expect("single");
        assert_eq!(art.data, single.data);
    }
}

#[test]
fn build_bundle_multi_pair_standalone_texture_matches_singles() {
    if !template_path().exists() {
        eprintln!(
            "skipping: template bundle not found at {}",
            template_path().display()
        );
        return;
    }
    let img = RgbaImage::from_fn(16, 16, |x, y| {
        image::Rgba([(x * 16) as u8, (y * 16) as u8, 128, 255])
    });
    let mut png: Vec<u8> = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .expect("encode test png");
    let opts = BuildOpts {
        source_file: Some("tex.png"),
        standalone_color_space: Some(1),
        real_textures: true,
        v38_compat: true,
        v38_timestamp: 638_000_000_000_000_000,
        ..BuildOpts::default()
    };
    let names = vec![
        "bafkreitestmultitex_mac".to_string(),
        "bafkreitestmultitex_windows".to_string(),
    ];
    let multi = build_bundle_multi(&png, &names, "bafkreitestmultitex", &opts).expect("multi");
    assert_ne!(multi[0].data, multi[1].data);
    for (art, name) in multi.iter().zip(names.iter()) {
        let single = build_bundle(&png, name, "bafkreitestmultitex", &opts).expect("single");
        assert_eq!(
            art.data, single.data,
            "{name}: standalone encode-once serialize must match a fresh build"
        );
    }
}
