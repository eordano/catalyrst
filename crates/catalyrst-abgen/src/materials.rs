use crate::scene::{Material, Scene, TexRef};
use crate::value::Value;
use std::collections::HashMap;

const KW_OCCLUSION: &str = "_OCCLUSION";
const KW_EMISSION: &str = "_EMISSION";
const KW_ALPHA_TEST: &str = "_ALPHATEST_ON";
const KW_NORMALMAP: &str = "_NORMALMAP";
const KW_METALLICSPECGLOSSMAP: &str = "_METALLICSPECGLOSSMAP";
const KW_ALPHA_PREMULTIPLY: &str = "_ALPHAPREMULTIPLY_ON";
const KW_TEXTURE_TRANSFORM: &str = "_TEXTURE_TRANSFORM";
const KW_UV_CHANNEL_SELECT: &str = "_UV_CHANNEL_SELECT";
const KW_SPECGLOSSMAP: &str = "_SPECGLOSSMAP";

const FW_PLUS: &str = "_FORWARD_PLUS";
const FW_PLUS_LIGHT_SHADOWS: &str = "_ADDITIONAL_LIGHT_SHADOWS";
const FW_PLUS_SHADOWS_CASCADE: &str = "_MAIN_LIGHT_SHADOWS_CASCADE";
const FW_PLUS_SHADOWS_SOFT: &str = "_SHADOWS_SOFT";

pub const EMISSIVE_HDR_INTENSITY: f64 = 5.0;

pub const MATERIAL_TEXTURE_SLOTS: [(&str, fn(&Material) -> Option<TexRef>); 6] = [
    ("_BaseMap", |m| m.base_color_image),
    ("_BumpMap", |m| m.normal_image),
    ("_MetallicGlossMap", |m| m.metallic_roughness_image),
    ("_OcclusionMap", |m| m.occlusion_image),
    ("_EmissionMap", |m| m.emissive_image),
    ("_SpecGlossMap", |m| m.spec_gloss_image),
];

fn lin2srgb(c: f64) -> f64 {
    const GAMMA_LO: f32 = 0.0031308;
    const GAMMA_ONE: f32 = 1.0;
    const GAMMA_12_92: f32 = 12.92;
    const GAMMA_1_055: f32 = 1.055;
    const GAMMA_0_055: f32 = 0.055;
    const GAMMA_EXP_LO: f32 = 0.4166667;
    const GAMMA_EXP_HI: f32 = 0.45454545;
    let c = c as f32;
    let out: f32 = if c <= 0.0 {
        0.0
    } else if c <= GAMMA_LO {
        GAMMA_12_92 * c
    } else if c < GAMMA_ONE {
        GAMMA_1_055 * c.powf(GAMMA_EXP_LO) - GAMMA_0_055
    } else {
        c.powf(GAMMA_EXP_HI)
    };
    out as f64
}

pub fn base_color_gamma(rgba: [f64; 4]) -> [f64; 4] {
    [
        lin2srgb(rgba[0]),
        lin2srgb(rgba[1]),
        lin2srgb(rgba[2]),
        rgba[3],
    ]
}

const LINEAR_SLOTS: [&str; 4] = [
    "_BumpMap",
    "_MetallicGlossMap",
    "_OcclusionMap",
    "_ParallaxMap",
];

const SRGB_SLOTS: [&str; 4] = ["_BaseMap", "_MainTex", "_EmissionMap", "_SpecGlossMap"];

fn material_slot_images(m: &Material) -> Vec<(&'static str, Option<TexRef>)> {
    vec![
        ("_BaseMap", m.base_color_image),
        ("_BaseMap", m.base_color_emit_image),
        ("_EmissionMap", m.emissive_image),
        ("_BumpMap", m.normal_image),
        ("_MetallicGlossMap", m.metal_rough_emit_image),
        ("_OcclusionMap", m.occlusion_image),
        ("_SpecGlossMap", m.spec_gloss_image),
    ]
}

pub fn classify_bc5_normal_images(scene: &Scene) -> std::collections::HashSet<usize> {
    let mut normal: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let mut base_or_emissive: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for m in &scene.materials {
        if let Some(tr) = m.normal_image {
            normal.insert(tr.image);
        }
        if let Some(tr) = m.base_color_image {
            base_or_emissive.insert(tr.image);
        }
        if let Some(tr) = m.emissive_image {
            base_or_emissive.insert(tr.image);
        }
    }
    normal.intersection(&base_or_emissive).copied().collect()
}

pub fn classify_dxt1_images(_scene: &Scene) -> std::collections::HashSet<usize> {
    std::collections::HashSet::new()
}

pub fn classify_spec_color_only_images(scene: &Scene) -> std::collections::HashSet<usize> {
    let mut spec: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let mut other: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for m in &scene.materials {
        if let Some(tr) = m.specular_color_image {
            spec.insert(tr.image);
        }
        for (_slot, tr) in material_slot_images(m) {
            if let Some(t) = tr {
                other.insert(t.image);
            }
        }
    }
    spec.difference(&other).copied().collect()
}

pub fn classify_unbound_images(scene: &Scene) -> std::collections::HashSet<usize> {
    let mut bound: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for m in &scene.materials {
        for (_slot, accessor) in MATERIAL_TEXTURE_SLOTS.iter() {
            if let Some(tr) = accessor(m) {
                bound.insert(tr.image);
            }
        }
    }
    let spec_only = classify_spec_color_only_images(scene);
    let mut out: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for tr in &scene.texture_refs {
        let img = tr.image;
        if img < scene.image_uri.len() && scene.image_uri[img].is_some() {
            continue;
        }
        if !bound.contains(&img) && !spec_only.contains(&img) {
            out.insert(img);
        }
    }
    out
}

pub fn classify_texture_colorspaces(scene: &Scene) -> HashMap<usize, i64> {
    let mut srgb: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let mut linear: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for m in &scene.materials {
        for (slot, tr) in material_slot_images(m) {
            let img = match tr {
                Some(t) => t.image,
                None => continue,
            };
            if SRGB_SLOTS.contains(&slot) {
                srgb.insert(img);
            } else if LINEAR_SLOTS.contains(&slot) {
                linear.insert(img);
            }
        }
    }
    let mut out: HashMap<usize, i64> = HashMap::new();
    for img in srgb.iter().chain(linear.iter()) {
        let v = if linear.contains(img) && !srgb.contains(img) {
            0
        } else {
            1
        };
        out.insert(*img, v);
    }
    out
}

fn material_keywords(
    m: &Material,
    keep_forward_plus: bool,
    has_normal: bool,
    has_metallic: bool,
    has_occlusion: bool,
    emissive_on: bool,
    has_emission_map: bool,
    has_tex_transform: bool,
    has_spec_gloss_map: bool,
) -> (Vec<String>, Vec<String>) {
    let mut valid: Vec<String> = vec![
        FW_PLUS_LIGHT_SHADOWS.to_string(),
        FW_PLUS_SHADOWS_CASCADE.to_string(),
        FW_PLUS_SHADOWS_SOFT.to_string(),
    ];
    if keep_forward_plus {
        valid.push(FW_PLUS.to_string());
    }

    let transparent = m.alpha_mode == "BLEND";
    let masked = m.alpha_mode == "MASK";
    if transparent {
        valid.push(KW_ALPHA_PREMULTIPLY.to_string());
    }
    if masked {
        valid.push(KW_ALPHA_TEST.to_string());
    }
    if has_metallic {
        valid.push(KW_METALLICSPECGLOSSMAP.to_string());
    }

    let mut invalid: Vec<String> = Vec::new();

    if emissive_on || has_emission_map {
        invalid.push(KW_EMISSION.to_string());
    }
    if has_normal {
        invalid.push(KW_NORMALMAP.to_string());
    }

    if has_occlusion {
        invalid.push(KW_OCCLUSION.to_string());
    }

    if has_tex_transform {
        invalid.push(KW_TEXTURE_TRANSFORM.to_string());
    }

    if m.uses_uv_channel_select {
        invalid.push(KW_UV_CHANNEL_SELECT.to_string());
    }

    if has_spec_gloss_map {
        invalid.push(KW_SPECGLOSSMAP.to_string());
    }

    valid.sort();
    invalid.sort();
    (valid, invalid)
}

fn saved_array<'a>(tree: &'a mut Value, key: &str) -> Option<&'a mut Vec<Value>> {
    tree.get_mut("m_SavedProperties")
        .and_then(|sp| sp.get_mut(key))
        .and_then(|a| a.as_array_mut())
}

fn set_tex(
    tree: &mut Value,
    slot: &str,
    pptr: (i64, i64),
    scale: (f64, f64),
    offset: (f64, f64),
) -> bool {
    let arr = match saved_array(tree, "m_TexEnvs") {
        Some(a) => a,
        None => return false,
    };
    let (fid, pid) = pptr;
    for entry in arr.iter_mut() {
        let matches = entry
            .as_array()
            .and_then(|e| e.first())
            .and_then(|n| n.as_str())
            .map(|n| n == slot)
            .unwrap_or(false);
        if matches {
            let env = map! {
                "m_Texture" => map!{ "m_FileID" => fid, "m_PathID" => pid },
                "m_Scale" => map!{ "x" => scale.0, "y" => scale.1 },
                "m_Offset" => map!{ "x" => offset.0, "y" => offset.1 },
            };
            *entry = arr![slot, env];
            return true;
        }
    }
    false
}

fn set_float(tree: &mut Value, name: &str, val: f64) -> bool {
    let arr = match saved_array(tree, "m_Floats") {
        Some(a) => a,
        None => return false,
    };
    for entry in arr.iter_mut() {
        let matches = entry
            .as_array()
            .and_then(|e| e.first())
            .and_then(|n| n.as_str())
            .map(|n| n == name)
            .unwrap_or(false);
        if matches {
            *entry = arr![name, val];
            return true;
        }
    }
    false
}

fn set_color(tree: &mut Value, name: &str, rgba: [f64; 4]) -> bool {
    let arr = match saved_array(tree, "m_Colors") {
        Some(a) => a,
        None => return false,
    };
    for entry in arr.iter_mut() {
        let matches = entry
            .as_array()
            .and_then(|e| e.first())
            .and_then(|n| n.as_str())
            .map(|n| n == name)
            .unwrap_or(false);
        if matches {
            let color = map! {
                "r" => rgba[0],
                "g" => rgba[1],
                "b" => rgba[2],
                "a" => rgba[3],
            };
            *entry = arr![name, color];
            return true;
        }
    }
    false
}

pub fn material_name(m: &Material, index: usize) -> String {
    let mut name = String::from("material");
    let orig = m.name.to_lowercase();
    if orig.contains("skin") {
        name.push_str("_skin");
    }
    if orig.contains("hair") {
        name.push_str("_hair");
    }
    format!("{name}_{index}")
}

pub fn build_material_tree(
    base_template: &Value,
    m: &Material,
    index: usize,
    shader_pptr: &Value,
    keep_forward_plus: bool,
    tex_pid: &HashMap<String, (i64, i64)>,
) -> Value {
    let mut t = base_template.clone();
    t.insert("m_Name", material_name(m, index));
    t.insert("m_Shader", shader_pptr.clone());

    let transparent = m.alpha_mode == "BLEND";
    let masked = m.alpha_mode == "MASK";

    let base_pid = tex_pid.get("_BaseMap").copied();
    let norm_pid = tex_pid.get("_BumpMap").copied();
    let metal_pid = tex_pid.get("_MetallicGlossMap").copied();
    let occ_pid = tex_pid.get("_OcclusionMap").copied();
    let emis_pid = tex_pid.get("_EmissionMap").copied();
    let spec_gloss_pid = tex_pid.get("_SpecGlossMap").copied();

    let truthy = |p: Option<(i64, i64)>| p.map(|(f, v)| f != 0 || v != 0).unwrap_or(false);

    let (er, eg, eb) = (m.emissive[0], m.emissive[1], m.emissive[2]);

    const COLOR_EQ_EPSILON: f32 = 9.999_999_4e-11;
    let (erf, egf, ebf) = (er as f32, eg as f32, eb as f32);
    let emissive_sqr_mag = erf * erf + egf * egf + ebf * ebf;
    let emissive_on = emissive_sqr_mag >= COLOR_EQ_EPSILON;

    let has_tex_transform = !m.tex_transforms.is_empty();
    let (valid, invalid) = material_keywords(
        m,
        keep_forward_plus,
        truthy(norm_pid),
        truthy(metal_pid),
        truthy(occ_pid),
        emissive_on,
        truthy(emis_pid),
        has_tex_transform,
        truthy(spec_gloss_pid),
    );
    t.insert(
        "m_ValidKeywords",
        Value::Array(valid.into_iter().map(Value::Str).collect()),
    );
    t.insert(
        "m_InvalidKeywords",
        Value::Array(invalid.into_iter().map(Value::Str).collect()),
    );
    t.insert("m_DoubleSidedGI", m.double_sided);

    t.insert("m_LightmapFlags", if emissive_on { 1i64 } else { 4i64 });

    let or0 = |p: Option<(i64, i64)>| p.unwrap_or((0, 0));

    let xform = |slot: &str| -> ((f64, f64), (f64, f64)) {
        match m.tex_transforms.get(slot) {
            Some(t) => ((t.scale[0], t.scale[1]), (t.offset[0], t.offset[1])),
            None => ((1.0, 1.0), (0.0, 0.0)),
        }
    };
    let (s, o) = xform("_BaseMap");
    set_tex(&mut t, "_BaseMap", or0(base_pid), s, o);
    let (s, o) = xform("_BumpMap");
    set_tex(&mut t, "_BumpMap", or0(norm_pid), s, o);
    let (s, o) = xform("_MetallicGlossMap");
    set_tex(&mut t, "_MetallicGlossMap", or0(metal_pid), s, o);
    let (s, o) = xform("_OcclusionMap");
    set_tex(&mut t, "_OcclusionMap", or0(occ_pid), s, o);
    let (s, o) = xform("_EmissionMap");
    set_tex(&mut t, "_EmissionMap", or0(emis_pid), s, o);
    let (s, o) = xform("_SpecGlossMap");
    set_tex(&mut t, "_SpecGlossMap", or0(spec_gloss_pid), s, o);

    set_color(&mut t, "_BaseColor", base_color_gamma(m.base_color));
    set_color(&mut t, "_Color", [1.0, 1.0, 1.0, 1.0]);
    if emissive_on {
        let hdr32 = EMISSIVE_HDR_INTENSITY as f32;
        set_color(
            &mut t,
            "_EmissionColor",
            [
                ((er as f32) * hdr32) as f64,
                ((eg as f32) * hdr32) as f64,
                ((eb as f32) * hdr32) as f64,
                EMISSIVE_HDR_INTENSITY,
            ],
        );
    } else {
        set_color(&mut t, "_EmissionColor", [0.0, 0.0, 0.0, 1.0]);
    }

    if m.uses_spec_gloss {
        set_float(&mut t, "_Metallic", 0.0);
        set_float(&mut t, "_Smoothness", 0.5);
        let g = m.glossiness_factor as f32 as f64;
        set_float(&mut t, "_Glossiness", g);
        set_float(&mut t, "_GlossMapScale", g);
        let sf = m.specular_factor;
        set_color(&mut t, "_SpecColor", [sf[0], sf[1], sf[2], 1.0]);
    } else {
        set_float(&mut t, "_Metallic", m.metallic);
        if truthy(metal_pid) {
            set_float(&mut t, "_Smoothness", 1.0);
            set_float(&mut t, "_SmoothnessTextureChannel", 0.0);
        } else {
            set_float(&mut t, "_Smoothness", (1.0_f32 - m.roughness as f32) as f64);
        }
    }

    if truthy(norm_pid) {
        set_float(&mut t, "_BumpScale", m.normal_scale);
    }
    if truthy(occ_pid) {
        set_float(&mut t, "_OcclusionStrength", m.occlusion_strength);
    }

    set_float(&mut t, "_Cull", if m.double_sided { 0.0 } else { 2.0 });
    if masked {
        t.insert(
            "stringTagMap",
            arr![arr!["RenderType", "TransparentCutout"]],
        );
        set_float(&mut t, "_SrcBlend", 1.0);
        set_float(&mut t, "_DstBlend", 0.0);
        set_float(&mut t, "_ZWrite", 1.0);
        set_float(&mut t, "_Surface", 1.0);
        set_float(&mut t, "_AlphaClip", 1.0);
        set_float(&mut t, "_Cutoff", m.alpha_cutoff);
        t.insert("m_CustomRenderQueue", 2450i64);
    } else if transparent {
        t.insert("stringTagMap", arr![arr!["RenderType", "Transparent"]]);
        set_float(&mut t, "_SrcBlend", 5.0);
        set_float(&mut t, "_DstBlend", 10.0);
        set_float(&mut t, "_ZWrite", 0.0);
        set_float(&mut t, "_Surface", 1.0);
        set_float(&mut t, "_Cutoff", 0.0);
        t.insert("m_CustomRenderQueue", 3000i64);
    } else {
        t.insert("stringTagMap", arr![arr!["RenderType", "Opaque"]]);
        set_float(&mut t, "_SrcBlend", 1.0);
        set_float(&mut t, "_DstBlend", 0.0);
        set_float(&mut t, "_ZWrite", 1.0);
        set_float(&mut t, "_Surface", 0.0);
        set_float(&mut t, "_Cutoff", 0.0);
        t.insert("m_CustomRenderQueue", 2000i64);
    }

    t
}

pub fn build_default_material_tree(base_template: &Value, shader_pptr: &Value) -> Value {
    let mut t = base_template.clone();
    t.insert("m_Name", "DCL_Scene");
    t.insert("m_Shader", shader_pptr.clone());
    t.insert("m_ValidKeywords", Value::Array(vec![]));
    t.insert("m_InvalidKeywords", Value::Array(vec![]));
    t.insert("m_CustomRenderQueue", -1i64);
    t.insert("m_LightmapFlags", 4i64);
    t.insert("m_DoubleSidedGI", false);
    t.insert("stringTagMap", Value::Array(vec![]));

    for slot in [
        "_BaseMap",
        "_BumpMap",
        "_MetallicGlossMap",
        "_OcclusionMap",
        "_EmissionMap",
        "_MainTex",
    ] {
        set_tex(&mut t, slot, (0, 0), (1.0, 1.0), (0.0, 0.0));
    }
    set_color(&mut t, "_BaseColor", [1.0, 1.0, 1.0, 1.0]);
    set_color(&mut t, "_Color", [1.0, 1.0, 1.0, 1.0]);
    set_color(&mut t, "_EmissionColor", [0.0, 0.0, 0.0, 1.0]);
    set_float(&mut t, "_Cutoff", 0.5);
    set_float(&mut t, "_Cull", 2.0);
    set_float(&mut t, "_Smoothness", 0.5);
    set_float(&mut t, "_Metallic", 0.0);
    set_float(&mut t, "_SrcBlend", 1.0);
    set_float(&mut t, "_DstBlend", 0.0);
    set_float(&mut t, "_ZWrite", 1.0);
    set_float(&mut t, "_Surface", 0.0);
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kw_lists(t: &Value) -> (Vec<String>, Vec<String>) {
        let collect = |key: &str| {
            t.get(key)
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default()
        };
        (collect("m_ValidKeywords"), collect("m_InvalidKeywords"))
    }

    #[test]
    fn unity_matched_keyword_and_lightmap_rules() {
        let shader = crate::value::pptr(0, 99);

        let m = Material {
            alpha_mode: "OPAQUE".into(),
            base_color: [1.0, 1.0, 1.0, 1.0],
            ..Material::default()
        };
        let tex: HashMap<String, (i64, i64)> = [
            ("_OcclusionMap", 22i64),
            ("_BumpMap", 11),
            ("_MetallicGlossMap", 33),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), (0, *v)))
        .collect();
        let t = build_material_tree(&template(), &m, 0, &shader, true, &tex);

        assert_eq!(t.get("m_LightmapFlags").unwrap().as_i64(), Some(4));

        let (valid, invalid) = kw_lists(&t);
        assert!(
            invalid.contains(&"_OCCLUSION".to_string()),
            "_OCCLUSION must be invalid"
        );
        assert!(
            invalid.contains(&"_NORMALMAP".to_string()),
            "_NORMALMAP must be invalid"
        );
        assert!(
            !valid.contains(&"_OCCLUSION".to_string()),
            "_OCCLUSION must NOT be valid"
        );
        assert!(
            valid.contains(&"_METALLICSPECGLOSSMAP".to_string()),
            "_METALLICSPECGLOSSMAP must be valid"
        );

        let me = Material {
            alpha_mode: "OPAQUE".into(),
            base_color: [1.0, 1.0, 1.0, 1.0],
            emissive: [0.5, 0.5, 0.5],
            ..Material::default()
        };
        let te = build_material_tree(&template(), &me, 1, &shader, true, &HashMap::new());
        assert_eq!(te.get("m_LightmapFlags").unwrap().as_i64(), Some(1));
        let (_v, iv) = kw_lists(&te);
        assert!(iv.contains(&"_EMISSION".to_string()));
    }

    fn tex_entry(name: &str) -> Value {
        arr![
            name,
            map! {
                "m_Texture" => map!{ "m_FileID" => 0, "m_PathID" => 0 },
                "m_Scale" => map!{ "x" => 1.0, "y" => 1.0 },
                "m_Offset" => map!{ "x" => 0.0, "y" => 0.0 },
            }
        ]
    }
    fn named_entry(name: &str) -> Value {
        arr![name, 0.0]
    }

    fn template() -> Value {
        let tex_envs = Value::Array(
            [
                "_BaseMap",
                "_BumpMap",
                "_MetallicGlossMap",
                "_OcclusionMap",
                "_EmissionMap",
                "_MainTex",
            ]
            .iter()
            .map(|s| tex_entry(s))
            .collect(),
        );
        let floats = Value::Array(
            [
                "_Metallic",
                "_Smoothness",
                "_SmoothnessTextureChannel",
                "_BumpScale",
                "_OcclusionStrength",
                "_Cull",
                "_SrcBlend",
                "_DstBlend",
                "_ZWrite",
                "_Surface",
                "_Cutoff",
                "_AlphaClip",
            ]
            .iter()
            .map(|s| named_entry(s))
            .collect(),
        );
        let colors = Value::Array(
            ["_BaseColor", "_Color", "_EmissionColor"]
                .iter()
                .map(|s| arr![*s, map! { "r"=>0.0,"g"=>0.0,"b"=>0.0,"a"=>0.0 }])
                .collect(),
        );
        map! {
            "m_Name" => "",
            "m_Shader" => map!{ "m_FileID" => 0, "m_PathID" => 0 },
            "m_SavedProperties" => map! {
                "m_TexEnvs" => tex_envs,
                "m_Floats" => floats,
                "m_Colors" => colors,
            },
        }
    }

    fn float_of(tree: &Value, name: &str) -> Option<f64> {
        tree.get("m_SavedProperties")?
            .get("m_Floats")?
            .as_array()?
            .iter()
            .find(|e| {
                e.as_array()
                    .and_then(|a| a.first())
                    .and_then(|n| n.as_str())
                    == Some(name)
            })
            .and_then(|e| e.as_array())
            .and_then(|a| a.get(1))
            .and_then(|v| v.as_f64())
    }

    #[test]
    fn spec_gloss_colorspaces_match_gltfast_gamma() {
        let tr = |image| {
            Some(TexRef {
                image,
                sampler: None,
            })
        };
        let m = Material {
            uses_spec_gloss: true,
            base_color_image: tr(0),
            base_color_emit_image: tr(2),
            spec_gloss_image: tr(1),
            metallic_roughness_image: None,
            metal_rough_emit_image: tr(3),
            ..Material::default()
        };
        let scene = Scene {
            materials: vec![m],
            ..Scene::default()
        };
        let cs = classify_texture_colorspaces(&scene);
        assert_eq!(cs.get(&0), Some(&1));
        assert_eq!(cs.get(&2), Some(&1));
        assert_eq!(cs.get(&1), Some(&1));
        assert_eq!(cs.get(&3), Some(&0));
    }

    #[test]
    fn base_color_gamma_known() {
        let g = base_color_gamma([0.0, 1.0, 0.5, 0.25]);
        assert_eq!(g[0], 0.0);
        assert!((g[1] - 1.0).abs() < 1e-9);
        assert_eq!(g[3], 0.25);
    }

    #[test]
    fn material_name_infixes() {
        let mut m = Material {
            name: "MySkinMat".into(),
            ..Material::default()
        };
        assert_eq!(material_name(&m, 3), "material_skin_3");
        m.name = "Hair_skin".into();
        assert_eq!(material_name(&m, 0), "material_skin_hair_0");
        m.name = "plain".into();
        assert_eq!(material_name(&m, 7), "material_7");
    }

    #[test]
    fn opaque_blend_state() {
        let m = Material {
            alpha_mode: "OPAQUE".into(),
            base_color: [1.0, 1.0, 1.0, 1.0],
            roughness: 0.4,
            ..Material::default()
        };
        let shader = crate::value::pptr(0, 99);
        let t = build_material_tree(&template(), &m, 0, &shader, true, &HashMap::new());
        assert_eq!(t.get("m_Name").unwrap().as_str(), Some("material_0"));
        assert_eq!(float_of(&t, "_SrcBlend"), Some(1.0));
        assert_eq!(float_of(&t, "_ZWrite"), Some(1.0));

        assert_eq!(
            float_of(&t, "_Smoothness"),
            Some((1.0_f32 - 0.4_f32) as f64)
        );
        assert_eq!(t.get("m_CustomRenderQueue").unwrap().as_i64(), Some(2000));
    }

    #[test]
    fn masked_blend_state() {
        let m = Material {
            alpha_mode: "MASK".into(),
            alpha_cutoff: 0.33,
            ..Material::default()
        };
        let shader = crate::value::pptr(0, 99);
        let t = build_material_tree(&template(), &m, 1, &shader, false, &HashMap::new());
        assert_eq!(float_of(&t, "_Cutoff"), Some(0.33));
        assert_eq!(float_of(&t, "_AlphaClip"), Some(1.0));
        assert_eq!(t.get("m_CustomRenderQueue").unwrap().as_i64(), Some(2450));

        let valid: Vec<&str> = t
            .get("m_ValidKeywords")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(valid.contains(&"_ALPHATEST_ON"));
    }

    #[test]
    fn default_material() {
        let shader = crate::value::pptr(0, 1);
        let t = build_default_material_tree(&template(), &shader);
        assert_eq!(t.get("m_Name").unwrap().as_str(), Some("DCL_Scene"));
        assert_eq!(t.get("m_CustomRenderQueue").unwrap().as_i64(), Some(-1));
        assert_eq!(t.get("m_LightmapFlags").unwrap().as_i64(), Some(4));
        assert_eq!(float_of(&t, "_Cutoff"), Some(0.5));
        assert_eq!(float_of(&t, "_Smoothness"), Some(0.5));
    }
}
