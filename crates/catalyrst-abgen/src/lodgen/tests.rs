use super::*;
use crate::lodgen::emit::{emit_empty_glb, emit_glb};
use crate::lodgen::model::{AlphaClass, LodImage, LodMaterial, LodModel, LodPrimitive};

#[test]
fn level_path_formatting() {
    assert_eq!(staged_glb_name("BafkReiX", 1), "bafkreix_1.glb");
    assert_eq!(
        staged_glb_name("qmccggwqvb7v3b3vqxajzcjimmzhzrrvmk3ulkt6qxsesd", 1),
        "qmccggwqvb7v3b3vqxajzcjimmzhzrrvmk3ulkt6qxsesd_1.glb"
    );
    assert_eq!(
        expected_rel_path("BafkReiX", 1, "windows"),
        "LOD/1/bafkreix_1_windows"
    );
    assert_eq!(expected_rel_path("scene", 0, "mac"), "LOD/0/scene_0_mac");
}

#[test]
fn choose_lane_level_0_is_always_passthrough() {
    for tri_cap in [None, Some(100u64), Some(1_000_000u64)] {
        for tri_cap_auto in [false, true] {
            for source_tris in [0usize, 400, 500, 501, 5_000_000] {
                for ratio in [0.1, 1.0] {
                    assert_eq!(
                        choose_lane(0, tri_cap, tri_cap_auto, ratio, source_tris, 500),
                        SimplifyLane::Passthrough,
                        "tri_cap={tri_cap:?} auto={tri_cap_auto} tris={source_tris} ratio={ratio}"
                    );
                }
            }
        }
    }
}

#[test]
fn choose_lane_level_1_matrix() {
    assert_eq!(
        choose_lane(1, None, false, 0.1, 400, 500),
        SimplifyLane::Passthrough
    );
    assert_eq!(
        choose_lane(1, None, false, 0.1, 500, 500),
        SimplifyLane::Passthrough
    );
    assert_eq!(
        choose_lane(1, None, false, 0.1, 501, 500),
        SimplifyLane::Uncapped { ratio: 0.1 }
    );
    assert_eq!(
        choose_lane(1, None, false, 0.25, 5_000_000, 1500),
        SimplifyLane::Uncapped { ratio: 0.25 }
    );
    assert_eq!(
        choose_lane(1, Some(250), false, 0.25, 400, 500),
        SimplifyLane::Capped {
            ratio: 0.25,
            cap: 250
        }
    );
    assert_eq!(
        choose_lane(1, Some(250), false, 0.1, 5_000_000, 500),
        SimplifyLane::Capped {
            ratio: 0.1,
            cap: 250
        }
    );
    assert_eq!(
        choose_lane(1, None, true, 0.1, 5_000_000, 1500),
        SimplifyLane::Capped {
            ratio: 0.1,
            cap: 1500
        }
    );
    assert_eq!(
        choose_lane(1, None, true, 0.1, 400, 500),
        SimplifyLane::Passthrough
    );
    assert_eq!(
        choose_lane(1, None, true, 0.1, 500, 500),
        SimplifyLane::Passthrough
    );
    assert_eq!(
        choose_lane(1, None, true, 0.1, 501, 500),
        SimplifyLane::Capped {
            ratio: 0.1,
            cap: 500
        }
    );
    assert_eq!(
        choose_lane(1, Some(500), false, 0.1, 400, 9999),
        SimplifyLane::Passthrough
    );
    assert_eq!(
        choose_lane(1, Some(9), true, 0.1, 400, 1500),
        SimplifyLane::Passthrough
    );
    assert_eq!(
        choose_lane(1, Some(9), true, 0.1, 1501, 1500),
        SimplifyLane::Capped {
            ratio: 0.1,
            cap: 1500
        }
    );
}

#[test]
fn default_params_cap_tris_to_auto_budget() {
    let p = GenerateParams::default();
    assert!(p.tri_cap_auto);
    assert_eq!(p.tri_cap, None);
    assert_eq!(p.levels, vec![0, 1]);
}

#[test]
fn simplifier_backend_parse_and_names() {
    use super::simplify::SimplifierBackend;
    assert_eq!(
        SimplifierBackend::parse("meshopt").unwrap(),
        SimplifierBackend::Meshopt
    );
    assert_eq!(
        SimplifierBackend::parse(" GLTFPACK ").unwrap(),
        SimplifierBackend::Gltfpack
    );
    let msg = format!("{:#}", SimplifierBackend::parse("pixyz").unwrap_err());
    assert!(msg.contains("meshopt|gltfpack"), "{msg}");
    assert_eq!(SimplifierBackend::Meshopt.name(), "meshopt");
    assert_eq!(SimplifierBackend::Gltfpack.name(), "gltfpack");
}

#[test]
fn normalize_levels_dedupes_and_refuses() {
    assert_eq!(normalize_levels(&[0, 1]).unwrap(), vec![0, 1]);
    assert_eq!(normalize_levels(&[1, 0, 1, 0]).unwrap(), vec![1, 0]);
    assert_eq!(normalize_levels(&[1]).unwrap(), vec![1]);
    let msg = format!("{:#}", normalize_levels(&[]).unwrap_err());
    assert!(msg.contains("at least one"), "{msg}");
    let msg = format!("{:#}", normalize_levels(&[0, 1, 2]).unwrap_err());
    assert!(msg.contains("level 2"), "{msg}");
    let msg = format!("{:#}", normalize_levels(&[7]).unwrap_err());
    assert!(msg.contains("level 7"), "{msg}");
}

#[test]
fn effective_tri_cap_table() {
    assert_eq!(effective_tri_cap(0, Some(100), true, 500), None);
    assert_eq!(effective_tri_cap(0, None, true, 500), None);
    assert_eq!(effective_tri_cap(1, None, true, 500), Some(500));
    assert_eq!(effective_tri_cap(1, Some(100), true, 500), Some(500));
    assert_eq!(effective_tri_cap(1, Some(100), false, 500), Some(100));
    assert_eq!(effective_tri_cap(1, None, false, 500), None);
}

#[test]
fn crop_union_and_orphan_stats_feed_the_gate() {
    let base = (0, 0);
    let parcels = vec![(0, 0), (1, 0), (0, 1)];
    let rects = crop::crop_rects_rh(base, &parcels);
    assert_eq!(rects.len(), 2);
    let mut model = LodModel {
        root_name: "gate-stats".to_string(),
        primitives: vec![LodPrimitive {
            positions: vec![
                [-2.0, 0.0, 2.0],
                [-4.0, 0.0, 2.0],
                [-2.0, 1.0, 4.0],
                [-99.0, 0.0, 99.0],
            ],
            normals: vec![[0.0, 1.0, 0.0]; 4],
            uvs: vec![[0.0, 0.0]; 4],
            indices: vec![0, 1, 2],
            material: 0,
            ..Default::default()
        }],
        materials: Vec::new(),
        images: Vec::new(),
        log: Vec::new(),
    };
    let report = crop::crop(&mut model, &rects);
    assert_eq!(report.rects, 2);
    assert_eq!(report.tris_out, 1);
    assert_eq!(report.verts_dropped, 1);
    let stats = crop::union_stats(&model, &rects, 1e-3);
    assert_eq!(stats.rects, 2);
    assert_eq!(stats.buffer_verts, 3);
    assert_eq!(stats.referenced_verts, 3);
    assert_eq!(stats.outside, 0);
    assert_eq!(stats.outside_fraction(), 0.0);
}

#[test]
fn tri_cap_gate_check_pass_fail_waiver() {
    let pass = gate::tri_cap_check(500, 500, false);
    assert!(pass.ok);
    assert_eq!(pass.label, "tri-cap");
    assert!(
        pass.detail.contains("500 tris <= cap 500"),
        "{}",
        pass.detail
    );

    let fail = gate::tri_cap_check(500, 501, false);
    assert!(!fail.ok);
    assert_eq!(gate_failures(&[pass, fail]), 1);

    let waived = gate::tri_cap_check(500, 3519, true);
    assert!(waived.ok);
    assert!(waived.detail.contains("WAIVED"), "{}", waived.detail);
    assert!(
        waived.detail.contains("--allow-unsimplified"),
        "{}",
        waived.detail
    );
}

#[test]
fn generate_refuses_level_2() {
    let params = GenerateParams {
        scene: "0,0".to_string(),
        levels: vec![2],
        ..Default::default()
    };
    let err = generate(&params).unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.contains("level 2"), "{msg}");

    let params = GenerateParams {
        scene: "0,0".to_string(),
        levels: Vec::new(),
        ..Default::default()
    };
    let msg = format!("{:#}", generate(&params).unwrap_err());
    assert!(msg.contains("at least one"), "{msg}");
}

#[test]
fn generate_refuses_webgl_platform() {
    let params = GenerateParams {
        scene: "0,0".to_string(),
        platform: "webgl".to_string(),
        ..Default::default()
    };
    let msg = format!("{:#}", generate(&params).unwrap_err());
    assert!(msg.contains("webgl"), "{msg}");

    let params = GenerateParams {
        scene: "0,0".to_string(),
        platforms: vec!["windows".to_string(), "webgl".to_string()],
        ..Default::default()
    };
    let msg = format!("{:#}", generate(&params).unwrap_err());
    assert!(msg.contains("webgl"), "{msg}");

    let params = GenerateParams {
        scene: "0,0".to_string(),
        platforms: vec!["amiga".to_string()],
        ..Default::default()
    };
    let msg = format!("{:#}", generate(&params).unwrap_err());
    assert!(msg.contains("amiga"), "{msg}");
}

fn tiny_png() -> Vec<u8> {
    let mut img = image::RgbaImage::new(4, 4);
    for (i, p) in img.pixels_mut().enumerate() {
        *p = image::Rgba([(i * 16) as u8, 128, 200, 255]);
    }
    let mut cur = std::io::Cursor::new(Vec::new());
    img.write_to(&mut cur, image::ImageFormat::Png).unwrap();
    cur.into_inner()
}

fn synthetic_glb() -> Vec<u8> {
    emit_glb(&LodModel {
        root_name: "synthetic".to_string(),
        primitives: vec![LodPrimitive {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            indices: vec![0, 1, 2],
            material: 0,
            ..Default::default()
        }],
        materials: vec![LodMaterial {
            name: "TextureBakeResult-mat".to_string(),
            class: AlphaClass::Opaque,
            base_color: [1.0, 1.0, 1.0, 1.0],
            cutoff: 0.5,
            image: Some(0),
            double_sided: false,
        }],
        images: vec![LodImage {
            bytes: tiny_png(),
            mime: "image/png".to_string(),
        }],
        log: Vec::new(),
    })
    .unwrap()
}

#[test]
fn empty_scene_bundle_passes_empty_gate_and_fails_content_gate() {
    std::env::set_var("ABGEN_ROOT", env!("CARGO_MANIFEST_DIR"));
    let dir = std::env::temp_dir().join(format!("abgen-lod-emptygate-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let sid = "bafkreiemptyscene";
    let src = dir.join(format!("{sid}_1.glb"));
    std::fs::write(&src, emit_empty_glb(&format!("{sid}_1")).unwrap()).unwrap();

    let client = CatalystClient::new("http://127.0.0.1:9");
    let opts = lods::LodOptions {
        platform: "windows".to_string(),
        lod: Some(lods::LodGenMeta {
            parcels: vec![(55, -76)],
            base: (55, -76),
            timestamp: None,
            vertical_override: None,
        }),
        ..Default::default()
    };
    let out = dir.join("out");
    let conv = lods::convert_lods(
        &client,
        &[src.to_string_lossy().into_owned()],
        out.to_str().unwrap(),
        &opts,
    )
    .unwrap();
    assert_eq!(conv.results.len(), 1);
    let bundle_path = out.join(sid).join(&conv.results[0].rel_path);
    let data = std::fs::read(&bundle_path).unwrap();

    let checks = self_gate_bundle_with(&data, sid, 1, "windows", false).unwrap();
    for c in &checks {
        assert!(c.ok, "unexpected FAIL {}: {}", c.label, c.detail);
    }
    let as_content = self_gate_bundle_with(&data, sid, 1, "windows", true).unwrap();
    let failed: Vec<&str> = as_content
        .iter()
        .filter(|c| !c.ok)
        .map(|c| c.label.as_str())
        .collect();
    assert!(failed.contains(&"material-count"), "{failed:?}");
    assert!(failed.contains(&"texture-count"), "{failed:?}");
    assert!(failed.contains(&"metadata-deps"), "{failed:?}");

    let (iss_path, iss_assets, iss_skipped) =
        write_iss_descriptor(&out, sid, &[], &HashMap::new()).unwrap();
    assert_eq!((iss_assets, iss_skipped), (0, 0));
    assert_eq!(
        iss_path,
        out.join(sid)
            .join(format!("{sid}{}", placements::ISS_SUFFIX))
    );
    let bytes = std::fs::read(&iss_path).unwrap();
    let doc: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(doc["assets"], serde_json::json!([]));
    assert_eq!(doc["sceneId"], serde_json::json!(sid));
    assert_eq!(doc["version"], serde_json::json!(1));
    assert!(placements::parse_iss(&bytes).unwrap().is_empty());
    let mut br = iss_path.as_os_str().to_owned();
    br.push(".br");
    assert!(PathBuf::from(br).is_file());

    let _ = std::fs::remove_dir_all(&dir);
}

fn first_target_platform(data: &[u8]) -> i32 {
    let bundle = Bundle::load_bytes(data).unwrap();
    for file in &bundle.files {
        if let FileContent::Serialized(sf) = &file.content {
            return sf.target_platform;
        }
    }
    panic!("no serialized file in bundle");
}

#[test]
fn multi_platform_bundles_union_manifest_and_target_platform_gate() {
    std::env::set_var("ABGEN_ROOT", env!("CARGO_MANIFEST_DIR"));
    let dir = std::env::temp_dir().join(format!("abgen-lod-multiplat-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let sid = "bafkreimultiplatsynthetic";
    let src = dir.join(format!("{sid}_1.glb"));
    std::fs::write(&src, synthetic_glb()).unwrap();

    let client = CatalystClient::new("http://127.0.0.1:9");
    let opts = lods::LodOptions {
        platform: "windows".to_string(),
        lod: Some(lods::LodGenMeta {
            parcels: vec![(8, -83)],
            base: (8, -83),
            timestamp: None,
            vertical_override: None,
        }),
        ..Default::default()
    };
    let platforms: Vec<String> = ["windows", "mac", "linux"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let out = dir.join("out");
    let conv = lods::convert_lods_platforms(
        &client,
        &[src.to_string_lossy().into_owned()],
        out.to_str().unwrap(),
        &opts,
        &platforms,
    )
    .unwrap();
    assert!(conv.skipped.is_empty(), "{:?}", conv.skipped);
    assert_eq!(conv.results.len(), 3);

    let mut datas: HashMap<&str, Vec<u8>> = HashMap::new();
    for (plat, want_tp) in [("windows", 19), ("mac", 2), ("linux", 24)] {
        let rel = expected_rel_path(sid, 1, plat);
        assert!(
            conv.results.iter().any(|r| r.rel_path == rel),
            "{rel} missing from results"
        );
        let path = out.join(sid).join(&rel);
        let data = std::fs::read(&path).unwrap();
        assert_eq!(first_target_platform(&data), want_tp, "{plat}");
        let mut br = path.as_os_str().to_owned();
        br.push(".br");
        assert!(PathBuf::from(br).is_file(), "{plat} .br sidecar missing");
        let checks = self_gate_bundle(&data, sid, 1, plat).unwrap();
        for c in &checks {
            assert!(c.ok, "{plat} unexpected FAIL {}: {}", c.label, c.detail);
        }
        let tp = checks
            .iter()
            .find(|c| c.label == "target-platform")
            .unwrap();
        assert!(
            tp.detail.contains(&format!("Some({want_tp})")),
            "{plat}: {}",
            tp.detail
        );
        let dep = checks
            .iter()
            .find(|c| c.label == "assetbundle-dep")
            .unwrap();
        let want_cab =
            crate::cabname::cab_name(&crate::shader::texarray_bundle_name(plat)).to_lowercase();
        assert!(dep.detail.contains(&want_cab), "{plat}: {}", dep.detail);
        if plat == "mac" {
            assert!(
                dep.detail.contains("cab-2f95afafeab990fc349e5ab530941444"),
                "{}",
                dep.detail
            );
        }
        datas.insert(plat, data);
    }
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join(sid).join("LOD.manifest.json")).unwrap())
            .unwrap();
    let files: Vec<String> = manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        files,
        vec![
            format!("LOD/1/{sid}_1_linux"),
            format!("LOD/1/{sid}_1_mac"),
            format!("LOD/1/{sid}_1_windows"),
        ]
    );
    assert_eq!(manifest["levels"], serde_json::json!([1]));
    assert_eq!(manifest["sceneId"], serde_json::json!(sid));
    assert_eq!(manifest["exitCode"], serde_json::json!(0));

    let out_single = dir.join("out-single");
    let conv_single = lods::convert_lods(
        &client,
        &[src.to_string_lossy().into_owned()],
        out_single.to_str().unwrap(),
        &opts,
    )
    .unwrap();
    assert_eq!(conv_single.results.len(), 1);
    let single_bundle = std::fs::read(
        out_single
            .join(sid)
            .join(expected_rel_path(sid, 1, "windows")),
    )
    .unwrap();
    assert_eq!(
        &single_bundle, &datas["windows"],
        "windows bundle bytes differ between convert_lods and convert_lods_platforms"
    );
    let single_manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(out_single.join(sid).join("LOD.manifest.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        single_manifest["files"],
        serde_json::json!([format!("LOD/1/{sid}_1_windows")])
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn multi_level_sources_build_both_levels_from_one_bake() {
    std::env::set_var("ABGEN_ROOT", env!("CARGO_MANIFEST_DIR"));
    let dir =
        std::env::temp_dir().join(format!("abgen-lod-multilevel-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let sid = "bafkreimultilevelsynthetic";
    let glb = synthetic_glb();
    let mut sources = Vec::new();
    for level in [0u32, 1] {
        let src = dir.join(staged_glb_name(sid, level));
        std::fs::write(&src, &glb).unwrap();
        sources.push(src.to_string_lossy().into_owned());
    }

    let client = CatalystClient::new("http://127.0.0.1:9");
    let opts = lods::LodOptions {
        platform: "windows".to_string(),
        lod: Some(lods::LodGenMeta {
            parcels: vec![(8, -83)],
            base: (8, -83),
            timestamp: None,
            vertical_override: None,
        }),
        ..Default::default()
    };
    let out = dir.join("out");
    let conv = lods::convert_lods_platforms(
        &client,
        &sources,
        out.to_str().unwrap(),
        &opts,
        &["windows".to_string()],
    )
    .unwrap();
    assert!(conv.skipped.is_empty(), "{:?}", conv.skipped);
    assert_eq!(conv.results.len(), 2);
    assert_eq!(conv.scene_id, sid);
    for level in [0u32, 1] {
        let rel = expected_rel_path(sid, level, "windows");
        assert!(
            conv.results
                .iter()
                .any(|r| r.rel_path == rel && r.level == level),
            "{rel} missing"
        );
        let path = out.join(sid).join(&rel);
        let data = std::fs::read(&path).unwrap();
        let checks = self_gate_bundle(&data, sid, level, "windows").unwrap();
        for c in &checks {
            assert!(c.ok, "L{level} unexpected FAIL {}: {}", c.label, c.detail);
        }
        let mut br = path.as_os_str().to_owned();
        br.push(".br");
        assert!(PathBuf::from(br).is_file(), "L{level} .br sidecar missing");
    }
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join(sid).join("LOD.manifest.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["levels"], serde_json::json!([0, 1]));
    assert_eq!(
        manifest["files"],
        serde_json::json!([
            format!("LOD/0/{sid}_0_windows"),
            format!("LOD/1/{sid}_1_windows"),
        ])
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn self_gate_passes_on_synthetic_lod_bundle_and_catches_mismatches() {
    std::env::set_var("ABGEN_ROOT", env!("CARGO_MANIFEST_DIR"));
    let dir = std::env::temp_dir().join(format!("abgen-lod-selfgate-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let sid = "bafkreiselfgatesynthetic";
    let src = dir.join(format!("{sid}_1.glb"));
    std::fs::write(&src, synthetic_glb()).unwrap();

    let client = CatalystClient::new("http://127.0.0.1:9");
    let opts = lods::LodOptions {
        platform: "windows".to_string(),
        lod: Some(lods::LodGenMeta {
            parcels: vec![(8, -83)],
            base: (8, -83),
            timestamp: None,
            vertical_override: None,
        }),
        ..Default::default()
    };
    let out = dir.join("out");
    let conv = lods::convert_lods(
        &client,
        &[src.to_string_lossy().into_owned()],
        out.to_str().unwrap(),
        &opts,
    )
    .unwrap();
    assert_eq!(conv.results.len(), 1);
    assert_eq!(
        conv.results[0].rel_path,
        expected_rel_path(sid, 1, "windows")
    );
    let bundle_path = out.join(sid).join(&conv.results[0].rel_path);
    let data = std::fs::read(&bundle_path).unwrap();

    let checks = self_gate_bundle(&data, sid, 1, "windows").unwrap();
    assert!(checks.len() >= 9, "{}", checks.len());
    for c in &checks {
        assert!(c.ok, "unexpected FAIL {}: {}", c.label, c.detail);
    }
    assert_eq!(gate_failures(&checks), 0);

    let wrong_id = self_gate_bundle(&data, "bafkreiwrongid", 1, "windows").unwrap();
    let failed: Vec<&str> = wrong_id
        .iter()
        .filter(|c| !c.ok)
        .map(|c| c.label.as_str())
        .collect();
    assert!(failed.contains(&"root-name"), "{failed:?}");
    assert!(failed.contains(&"metadata-main-asset"), "{failed:?}");

    let wrong_platform = self_gate_bundle(&data, sid, 1, "mac").unwrap();
    let failed: Vec<&str> = wrong_platform
        .iter()
        .filter(|c| !c.ok)
        .map(|c| c.label.as_str())
        .collect();
    assert!(failed.contains(&"assetbundle-dep"), "{failed:?}");
    assert!(failed.contains(&"metadata-deps"), "{failed:?}");

    let wrong_level = self_gate_bundle(&data, sid, 0, "windows").unwrap();
    assert!(gate_failures(&wrong_level) > 0);

    let _ = std::fs::remove_dir_all(&dir);
}
