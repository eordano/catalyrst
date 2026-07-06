use super::*;

pub(super) fn shader_pptr() -> Value {
    crate::value::pptr(SHADER_FILE_ID, SHADER_PATH_ID)
}

const LOD_SPEC_COLOR: f64 = 0.199_999_958_276_748_66;

fn lod_tex_env(slot: &str, pptr: (i64, i64), scale: (f64, f64), offset: (f64, f64)) -> Value {
    arr![
        slot,
        map! {
            "m_Texture" => map!{"m_FileID" => pptr.0, "m_PathID" => pptr.1},
            "m_Scale" => map!{"x" => scale.0, "y" => scale.1},
            "m_Offset" => map!{"x" => offset.0, "y" => offset.1},
        }
    ]
}

fn lod_color(name: &str, rgba: [f64; 4]) -> Value {
    arr![
        name,
        map! {"r" => rgba[0], "g" => rgba[1], "b" => rgba[2], "a" => rgba[3]}
    ]
}

fn build_lod_material_tree(
    base_template: &Value,
    m: &crate::scene::Material,
    name: &str,
    shader: &Value,
    tex_pid: &HashMap<String, (i64, i64)>,
    lod: &LodBuildParams,
) -> Value {
    let mut t = base_template.clone();
    t.insert("m_Name", name);
    t.insert("m_Shader", shader.clone());

    let masked = m.alpha_mode == "MASK";
    let transparent = m.alpha_mode == "BLEND";

    let valid: Vec<Value> = if masked {
        vec![
            Value::Str("_ALPHATEST_ON".into()),
            Value::Str("_SURFACE_TYPE_TRANSPARENT".into()),
        ]
    } else if transparent {
        vec![
            Value::Str("_ALPHAPREMULTIPLY_ON".into()),
            Value::Str("_SURFACE_TYPE_TRANSPARENT".into()),
        ]
    } else {
        Vec::new()
    };
    t.insert("m_ValidKeywords", Value::Array(valid));
    t.insert("m_InvalidKeywords", Value::Array(Vec::new()));
    t.insert("m_LightmapFlags", 4i64);
    t.insert("m_EnableInstancingVariants", false);
    t.insert("m_DoubleSidedGI", m.double_sided);
    t.insert(
        "m_CustomRenderQueue",
        if masked {
            2450i64
        } else if transparent {
            3000i64
        } else {
            -1i64
        },
    );
    if masked {
        t.insert(
            "stringTagMap",
            arr![arr!["RenderType", "TransparentCutout"]],
        );
    } else if transparent {
        t.insert("stringTagMap", arr![arr!["RenderType", "Transparent"]]);
    } else {
        t.insert("stringTagMap", Value::Array(Vec::new()));
    }
    t.insert("disabledShaderPasses", arr!["MOTIONVECTORS"]);

    let base_pid = tex_pid.get("_BaseMap").copied().unwrap_or((0, 0));
    let (base_scale, base_offset) = match m.tex_transforms.get("_BaseMap") {
        Some(x) => ((x.scale[0], x.scale[1]), (x.offset[0], x.offset[1])),
        None => ((1.0, 1.0), (0.0, 0.0)),
    };
    let tex_envs: Vec<Value> = vec![
        lod_tex_env("_BaseMap", base_pid, base_scale, base_offset),
        lod_tex_env("_BaseMapArr", (0, 0), (1.0, 1.0), (0.0, 0.0)),
        lod_tex_env("_BumpMap", (0, 0), (1.0, 1.0), (0.0, 0.0)),
        lod_tex_env("_EmissionMap", (0, 0), (1.0, 1.0), (0.0, 0.0)),
        lod_tex_env("_MainTex", (0, 0), (1.0, 1.0), (0.0, 0.0)),
        lod_tex_env("_MetallicGlossMap", (0, 0), (1.0, 1.0), (0.0, 0.0)),
        lod_tex_env("_OcclusionMap", (0, 0), (1.0, 1.0), (0.0, 0.0)),
        lod_tex_env("_ParallaxMap", (0, 0), (1.0, 1.0), (0.0, 0.0)),
        lod_tex_env("_SpecGlossMap", (0, 0), (1.0, 1.0), (0.0, 0.0)),
    ];

    let alpha_clip = if masked { 1.0 } else { 0.0 };
    let alpha_to_mask = if masked { 1.0 } else { 0.0 };
    let cutoff = if masked { m.alpha_cutoff } else { 0.5 };
    let dst_blend = if transparent { 10.0 } else { 0.0 };
    let surface = if masked || transparent { 1.0 } else { 0.0 };
    let zwrite = if transparent { 0.0 } else { 1.0 };
    let floats: Vec<(&str, f64)> = vec![
        ("_AddPrecomputedVelocity", 0.0),
        ("_AlphaClip", alpha_clip),
        ("_AlphaToMask", alpha_to_mask),
        ("_Blend", 0.0),
        ("_BlendModePreserveSpecular", 1.0),
        ("_BumpScale", 1.0),
        ("_ClearCoatMask", 0.0),
        ("_ClearCoatSmoothness", 0.0),
        ("_Cull", if m.double_sided { 0.0 } else { 2.0 }),
        ("_Cutoff", cutoff),
        ("_DetailAlbedoMapScale", 1.0),
        ("_DetailNormalMapScale", 1.0),
        ("_DstBlend", dst_blend),
        ("_DstBlendAlpha", dst_blend),
        ("_EnvironmentReflections", 1.0),
        ("_GlossMapScale", 1.0),
        ("_Glossiness", 0.0),
        ("_GlossyReflections", 1.0),
        ("_Metallic", 0.0),
        ("_Mode", 0.0),
        ("_OcclusionStrength", 1.0),
        ("_Parallax", 0.02),
        ("_QueueOffset", 0.0),
        ("_ReceiveShadows", 1.0),
        ("_Smoothness", 0.0),
        ("_SmoothnessTextureChannel", 0.0),
        ("_SpecularHighlights", 1.0),
        ("_SrcBlend", 1.0),
        ("_SrcBlendAlpha", 1.0),
        ("_Surface", surface),
        ("_UVSec", 0.0),
        ("_WorkflowMode", 1.0),
        ("_XRMotionVectorsPass", 1.0),
        ("_ZWrite", zwrite),
    ];
    let floats_v: Vec<Value> = floats.into_iter().map(|(n, v)| arr![n, v]).collect();

    let colors: Vec<Value> = vec![
        lod_color("_BaseColor", materials::base_color_verbatim(m.base_color)),
        lod_color("_Color", [1.0, 1.0, 1.0, 1.0]),
        lod_color("_EmissionColor", [0.0, 0.0, 0.0, 1.0]),
        lod_color("_PlaneClipping", lod.plane_clipping),
        lod_color(
            "_SpecColor",
            [LOD_SPEC_COLOR, LOD_SPEC_COLOR, LOD_SPEC_COLOR, 1.0],
        ),
        lod_color("_VerticalClipping", lod.vertical_clipping),
    ];

    t.insert(
        "m_SavedProperties",
        map! {
            "m_TexEnvs" => Value::Array(tex_envs),
            "m_Ints" => Value::Array(vec![arr!["_BaseMapArr_ID", -1]]),
            "m_Floats" => Value::Array(floats_v),
            "m_Colors" => Value::Array(colors),
        },
    );
    t
}

impl<'a> Builder<'a> {
    pub(super) fn default_material(&mut self) -> i64 {
        if self.default_mat.is_none() {
            let base = self.base_clone("Material");
            let tree = materials::build_default_material_tree(&base, &self.active_shader_pptr());
            let pid = self.add("Material", tree, Role::Mat("DCL_Scene".into()));
            self.default_mat = Some(pid);
            self.material_entries
                .push(("DCL_Scene.mat".to_string(), pid, vec![]));
        }
        self.default_mat.unwrap()
    }

    pub(super) fn material(&mut self, scene: &Scene, mat_idx: Option<usize>) -> Option<i64> {
        Some(self.material_inner(scene, mat_idx, true))
    }

    pub(super) fn material_orphan(&mut self, scene: &Scene, mat_idx: Option<usize>) -> i64 {
        self.material_inner(scene, mat_idx, false)
    }

    fn material_inner(
        &mut self,
        scene: &Scene,
        mat_idx: Option<usize>,
        referenced_by_glb: bool,
    ) -> i64 {
        let mat_idx = match mat_idx {
            Some(m) if m < scene.materials.len() => m,

            _ => {
                let pid = self.default_material();
                if referenced_by_glb && !self.glb_referenced_mats.contains(&pid) {
                    self.glb_referenced_mats.push(pid);
                }
                return pid;
            }
        };
        if let Some(&pid) = self.mat_pid.get(&mat_idx) {
            if referenced_by_glb && !self.glb_referenced_mats.contains(&pid) {
                self.glb_referenced_mats.push(pid);
            }
            return pid;
        }
        let m = &scene.materials[mat_idx];

        let index = mat_idx;
        let name = if self.lod.is_some() && !m.name.is_empty() {
            m.name.clone()
        } else {
            materials::material_name(m, index)
        };
        let mut tex_pid: HashMap<String, (i64, i64)> = HashMap::new();
        let mut local_tex_pids: Vec<i64> = Vec::new();
        let mut ext_pptrs: Vec<(i64, i64)> = Vec::new();
        for (slot, accessor) in materials::MATERIAL_TEXTURE_SLOTS.iter() {
            let tex = accessor(m);

            if let Some(pp) = self.external_texture(scene, tex.as_ref().map(|t| t.image)) {
                tex_pid.insert(slot.to_string(), pp);
                ext_pptrs.push(pp);
                continue;
            }
            if let Some(pid) = self.texture(scene, tex) {
                tex_pid.insert(slot.to_string(), (0, pid));
                local_tex_pids.push(pid);
            }
        }
        let base = self.base_clone("Material");
        let tree = if let Some(lod) = &self.lod {
            build_lod_material_tree(&base, m, &name, &self.active_shader_pptr(), &tex_pid, lod)
        } else {
            materials::build_material_tree(
                &base,
                m,
                index,
                &shader_pptr(),
                self.keep_forward_plus,
                &tex_pid,
            )
        };
        let pid = self.add("Material", tree, Role::Mat(name.clone()));
        self.mat_pid.insert(mat_idx, pid);
        self.material_entries
            .push((format!("{name}.mat"), pid, local_tex_pids));
        if !ext_pptrs.is_empty() {
            self.mat_external_pptrs.insert(pid, ext_pptrs);
        }
        if referenced_by_glb && !self.glb_referenced_mats.contains(&pid) {
            self.glb_referenced_mats.push(pid);
        }
        pid
    }
}
