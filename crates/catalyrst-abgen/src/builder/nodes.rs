use super::*;

fn merge_into(dst: &mut Value, src: Value) {
    if let (Value::Map(d), Value::Map(mut s)) = (dst, src) {
        for (k, v) in std::mem::take(&mut s.0) {
            d.insert(k, v);
        }
    }
}

impl<'a> Builder<'a> {
    pub(super) fn build(&mut self, scene: &Scene) -> Result<()> {
        self.collect_collidable_mesh_keys(scene);
        self.assign_lod_mesh_names(scene);
        self.colorspaces = materials::classify_texture_colorspaces(scene);
        self.dxt1_images = materials::classify_dxt1_images(scene);
        self.bc5_normal_images = materials::classify_bc5_normal_images(scene);
        self.spec_color_only_images = materials::classify_spec_color_only_images(scene);
        self.unbound_images = materials::classify_unbound_images(scene);

        self.build_sampler_canon(scene);
        for tr in &scene.texture_refs {
            self.image_distinct_samplers
                .entry(tr.image)
                .or_default()
                .insert(tr.sampler);
        }

        for tr in &scene.texture_refs {
            let img = tr.image;
            let is_external = img < scene.image_uri.len() && scene.image_uri[img].is_some();
            if !is_external {
                self.texture(scene, Some(*tr));
            }
        }

        if self.toggles.v38_compat || self.toggles.collection_mode || self.force_default_material {
            let _ = self.default_material();
        }

        let mut has_anim = !self.is_wearable
            && ((self.is_emote && self.proto.contains_key("AnimatorController"))
                || (!self.is_emote && self.proto.contains_key("AnimationClip")));

        let mut prebuilt_clips: Option<Vec<Value>> = None;
        if has_anim {
            let clips = if self.is_emote {
                let base_clip = self.base_clone("AnimationClip_mecanim");
                animation_mecanim::build_mecanim_clips(
                    &self.gltf_json,
                    &self.gltf_buffers,
                    &base_clip,
                )
            } else {
                animation::build_animation_clips_from_gltf(&self.gltf_json, &self.gltf_buffers)
            };
            has_anim = !clips.is_empty();
            prebuilt_clips = Some(clips);
        }

        let scene_name: &str = scene.name.as_deref().unwrap_or("");
        let scene_path = format!("scenes/{scene_name}");

        let wrap = scene.root_nodes.len() != 1 || has_anim;

        self.count_scene_visits(scene, &scene.root_nodes);

        let root_go;
        if !wrap {
            let (go, _tr) = self.build_node(scene, scene.root_nodes[0], 0, &scene_path);
            root_go = go;
        } else {
            let wrap_is_importer_parent = scene.root_nodes.len() <= 1 && has_anim;
            let wrap_inner: &str = if !scene_name.is_empty() {
                scene_name
            } else if wrap_is_importer_parent {
                "New Game Object"
            } else {
                "Scene"
            };

            let inner_name: &str = if scene_name.is_empty() {
                "Scene"
            } else {
                scene_name
            };
            let wrap_path = format!("{scene_path}/{wrap_inner}");
            root_go = self.npid();
            let root_tr = self.npid();

            let has_anim_component = self.proto.contains_key("AnimatorController")
                || self.proto.contains_key("AnimatorOverrideController")
                || self.proto.contains_key("Animation");
            let inline_root: Option<usize> = if scene.root_nodes.len() == 1 {
                let ri = scene.root_nodes[0];
                let r = &scene.nodes[ri];
                let identity_trs = r.translation == [0.0, 0.0, 0.0]
                    && r.rotation == [0.0, 0.0, 0.0, 1.0]
                    && r.scale == [1.0, 1.0, 1.0];
                if r.primitives.is_empty()
                    && !r.children.is_empty()
                    && identity_trs
                    && !has_anim_component
                {
                    Some(ri)
                } else {
                    None
                }
            } else {
                None
            };

            let empty_anim_scene = scene.root_nodes.is_empty() && has_anim;
            let (wrap_t, wrap_r, wrap_s, child_transforms) = if empty_anim_scene {
                let inner_path = format!("{wrap_path}/{inner_name}");
                let inner_go = self.npid();
                let inner_tr = self.npid();
                let inner_tr_tree = self.transform_tree(
                    inner_go,
                    [0.0, 0.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                    [1.0, 1.0, 1.0],
                    &[],
                    root_tr,
                );
                self.set_obj(
                    inner_tr,
                    "Transform",
                    inner_tr_tree,
                    Role::Glb("Transform".into(), format!("{inner_path}/Transform")),
                );
                let inner_go_tree = self.go_tree(inner_name, &[inner_tr]);
                self.set_obj(
                    inner_go,
                    "GameObject",
                    inner_go_tree,
                    Role::Glb("GameObject".into(), inner_path.clone()),
                );
                self.scene_object_pids.push(inner_tr);
                self.scene_object_pids.push(inner_go);
                self.anim_target_go = inner_go;
                self.anim_target_recycle = inner_path;
                self.emit_orphan_node_assets(scene);
                (
                    [0.0, 0.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                    [1.0, 1.0, 1.0],
                    vec![inner_tr],
                )
            } else if let Some(ri) = inline_root {
                let r = scene.nodes[ri].clone();

                let child_trs: Vec<i64> = r
                    .children
                    .iter()
                    .map(|ci| self.build_node(scene, *ci, root_tr, &wrap_path).1)
                    .collect();
                (r.translation, r.rotation, r.scale, child_trs)
            } else {
                let roots = scene.root_nodes.clone();
                let child_trs: Vec<i64> = roots
                    .iter()
                    .map(|ni| self.build_node(scene, *ni, root_tr, &wrap_path).1)
                    .collect();
                (
                    [0.0, 0.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                    [1.0, 1.0, 1.0],
                    child_trs,
                )
            };
            let (wrap_t, wrap_r, wrap_s) = match &self.lod {
                Some(l) => (l.root_position, [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0]),
                None => (wrap_t, wrap_r, wrap_s),
            };
            let tr_tree =
                self.transform_tree(root_go, wrap_t, wrap_r, wrap_s, &child_transforms, 0);
            self.set_obj(
                root_tr,
                "Transform",
                tr_tree,
                Role::Glb("Transform".into(), format!("{wrap_path}/Transform")),
            );
            let go_tree = self.go_tree(&self.root_hash.clone(), &[root_tr]);
            self.set_obj(
                root_go,
                "GameObject",
                go_tree,
                Role::Glb("GameObject".into(), wrap_path),
            );
            self.scene_object_pids.push(root_tr);
            self.scene_object_pids.push(root_go);
            self.bundle_root_assigned = true;
        }
        self.root_go_pid = root_go;

        if !self.orphan_assets_emitted {
            self.emit_orphan_node_assets(scene);
        }

        let root_recycle = match self.roles.get(&root_go) {
            Some(Role::Glb(_, r)) => r.clone(),
            _ => String::new(),
        };
        if self.anim_target_go == 0 {
            self.anim_target_go = root_go;
            self.anim_target_recycle = root_recycle.clone();
        }
        let anim_go = self.anim_target_go;
        let anim_recycle = self.anim_target_recycle.clone();

        let mut anim_component_pid: Option<i64> = None;
        if self.is_emote && self.proto.contains_key("AnimatorController") {
            let clips = prebuilt_clips.take().unwrap_or_else(|| {
                let base_clip = self.base_clone("AnimationClip_mecanim");
                animation_mecanim::build_mecanim_clips(
                    &self.gltf_json,
                    &self.gltf_buffers,
                    &base_clip,
                )
            });
            let mut clip_specs: Vec<(String, i64)> = Vec::new();

            let clip_names: Vec<String> = clips
                .iter()
                .map(|c| {
                    c.get("m_Name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string()
                })
                .collect();
            let mut order: Vec<usize> = (0..clip_names.len()).collect();
            order.sort_by(|&a, &b| clip_names[a].cmp(&clip_names[b]).then(a.cmp(&b)));
            let mut rank = vec![0usize; order.len()];
            for (r, &orig) in order.iter().enumerate() {
                rank[orig] = r;
            }
            for (idx, mut clip) in clips.into_iter().enumerate() {
                self.set_muscle_clip_size(&mut clip);
                let name = clip_names[idx].clone();
                let pid = self.add(
                    "AnimationClip",
                    clip,
                    Role::AnimControllerSubClip(rank[idx]),
                );

                clip_specs.push((name, pid));
            }
            if !clip_specs.is_empty() {
                let base_ctrl = self.base_clone("AnimatorController");
                let ctrl = animation_mecanim::build_animator_controller(&clip_specs, &base_ctrl);
                let ctrl_pid = self.add("AnimatorController", ctrl, Role::AnimController);
                self.scene_object_pids.push(ctrl_pid);
                let clip_pids: Vec<i64> = clip_specs.iter().map(|(_, p)| *p).collect();
                self.animator_controller_entry = Some((ctrl_pid, clip_pids));
                let animator = animation_mecanim::build_animator_component(root_go, ctrl_pid);
                anim_component_pid = Some(self.add(
                    "Animator",
                    animator,
                    Role::Glb("Animator".into(), format!("{root_recycle}/Animator")),
                ));
            }
        } else if !self.is_emote && !self.is_wearable && self.proto.contains_key("AnimationClip") {
            let clips = prebuilt_clips.take().unwrap_or_else(|| {
                animation::build_animation_clips_from_gltf(&self.gltf_json, &self.gltf_buffers)
            });
            let mut clip_name_pids: Vec<(String, i64)> = Vec::new();
            for clip in clips {
                let name = clip
                    .get("m_Name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let mut tree = self.base_clone("AnimationClip");
                merge_into(&mut tree, clip);
                let recycle = self.unique_recycle("animations", &name);
                let pid = self.add(
                    "AnimationClip",
                    tree,
                    Role::Glb("AnimationClip".into(), recycle),
                );
                clip_name_pids.push((name, pid));
                self.scene_object_pids.push(pid);
            }
            if !clip_name_pids.is_empty() && self.proto.contains_key("Animation") {
                let comp = animation::build_animation_component(anim_go, &clip_name_pids);
                anim_component_pid = Some(self.add(
                    "Animation",
                    comp,
                    Role::Glb("Animation".into(), format!("{anim_recycle}/Animation")),
                ));
                self.anim_clip_name_pids = clip_name_pids;
            }
        }

        if let Some(acp) = anim_component_pid {
            self.scene_object_pids.push(acp);
            if let Some((_, tree)) = self.objects.get_mut(&anim_go) {
                if let Some(comp) = tree.get_mut("m_Component").and_then(|v| v.as_array_mut()) {
                    comp.push(map! {"component" => crate::value::pptr(0, acp)});
                }
            }
        }

        let pending = std::mem::take(&mut self.pending_smr);
        let node_children: Vec<Vec<usize>> = if pending.is_empty() {
            Vec::new()
        } else {
            scene.nodes.iter().map(|n| n.children.clone()).collect()
        };
        for (smr_pid, go_pid, mesh_pid, mat_pid, skin_idx, blend_shape_weights) in pending {
            let (bones, root_bone) = match skin_idx {
                Some(si) => {
                    let skin = &scene.skins[si];
                    let bs: Vec<Value> = skin
                        .joints
                        .iter()
                        .map(|j| crate::value::pptr(0, *self.node_tr.get(j).unwrap_or(&0)))
                        .collect();
                    let view = skeleton::SkinView {
                        node_children: &node_children,
                        joints: &skin.joints,
                        skeleton: skin.skeleton,
                    };
                    let rb = match skeleton::resolve_root_joint(&view) {
                        Some(rj) if self.node_tr.contains_key(&rj) => {
                            crate::value::pptr(0, self.node_tr[&rj])
                        }
                        _ => crate::value::pptr(0, 0),
                    };
                    (bs, rb)
                }
                None => (Vec::new(), crate::value::pptr(0, 0)),
            };
            let base = self.base_clone("SkinnedMeshRenderer");
            let mats: Vec<Value> = mat_pid
                .into_iter()
                .map(|p| crate::value::pptr(0, p))
                .collect();
            let smr = mesh_layout::skinned_mesh_renderer_tree(
                &base,
                go_pid,
                mesh_pid,
                mats,
                bones,
                root_bone,
                &blend_shape_weights,
            );

            if let Some(slot) = self.objects.get_mut(&smr_pid) {
                slot.1 = smr;
            }
        }

        for (name, roots) in scene.extra_scenes.clone() {
            self.build_extra_scene(scene, name.as_deref(), &roots);
        }

        for mat_idx in 0..scene.materials.len() {
            if !self.mat_pid.contains_key(&mat_idx) {
                let _ = self.material_orphan(scene, Some(mat_idx));
            }
        }

        if emits_metadata_textasset(&self.root_hash, self.toggles.v38_compat) {
            let mut meta = self.base_clone("TextAsset");
            meta.insert("m_Name", "metadata");
            meta.insert("m_Script", self.metadata_script_json());
            self.meta_pid = self.add("TextAsset", meta, Role::Meta);
        }

        self.build_assetbundle();
        Ok(())
    }

    fn count_scene_visits(&mut self, scene: &Scene, roots: &[usize]) {
        fn dfs(
            scene: &Scene,
            idx: usize,
            on_path: &mut HashSet<usize>,
            counts: &mut HashMap<usize, usize>,
        ) {
            if idx >= scene.nodes.len() || !on_path.insert(idx) {
                return;
            }
            *counts.entry(idx).or_insert(0) += 1;
            for ci in &scene.nodes[idx].children {
                dfs(scene, *ci, on_path, counts);
            }
            on_path.remove(&idx);
        }
        self.visits_left.clear();
        let mut on_path = HashSet::new();
        for r in roots {
            dfs(scene, *r, &mut on_path, &mut self.visits_left);
        }
    }

    fn build_bare_node(
        &mut self,
        scene: &Scene,
        node_idx: usize,
        parent_tr: i64,
        parent_path: &str,
    ) -> i64 {
        if let Some(c) = self.visits_left.get_mut(&node_idx) {
            *c = c.saturating_sub(1);
        }
        let node = &scene.nodes[node_idx];
        let (t, r, s, children) = (
            node.translation,
            node.rotation,
            node.scale,
            node.children.clone(),
        );
        let node_path = format!("{parent_path}/New Game Object");
        let go = self.npid();
        let tr = self.npid();
        let mut child_trs: Vec<i64> = Vec::new();
        for ci in &children {
            if *ci >= scene.nodes.len() || self.visits_left.get(ci).copied().unwrap_or(0) == 0 {
                continue;
            }
            child_trs.push(self.build_bare_node(scene, *ci, tr, &node_path));
        }
        let go_tree = self.go_tree("New Game Object", &[tr]);
        self.set_obj(
            go,
            "GameObject",
            go_tree,
            Role::Glb("GameObject".into(), node_path.clone()),
        );
        let tr_tree = self.transform_tree(go, t, r, s, &child_trs, parent_tr);
        self.set_obj(
            tr,
            "Transform",
            tr_tree,
            Role::Glb("Transform".into(), format!("{node_path}/Transform")),
        );
        self.scene_object_pids.push(go);
        self.scene_object_pids.push(tr);
        tr
    }

    fn build_node(
        &mut self,
        scene: &Scene,
        node_idx: usize,
        parent_tr: i64,
        parent_path: &str,
    ) -> (i64, i64) {
        if let Some(c) = self.visits_left.get_mut(&node_idx) {
            *c = c.saturating_sub(1);
        }
        let node = scene.nodes[node_idx].clone();

        let node_name = match scene.unique_node_names.get(node_idx) {
            Some(n) => n.clone(),
            None => {
                if !node.name.is_empty() {
                    node.name.clone()
                } else if let Some(p) = node.primitives.first() {
                    p.name.clone()
                } else {
                    format!("Node-{node_idx}")
                }
            }
        };
        let node_path = format!("{parent_path}/{node_name}");
        let go = self.npid();
        let tr = self.npid();
        let mut components = vec![tr];
        let mut comp_roles: Vec<(i64, Role)> = vec![(
            tr,
            Role::Glb("Transform".into(), format!("{node_path}/Transform")),
        )];

        let mut extra_child_transforms: Vec<i64> = Vec::new();
        if !node.primitives.is_empty() {
            let mesh_base = node.primitives[0].name.clone();

            let merged =
                self.try_attach_primitives_merged(scene, go, tr, &node, &node_path, &mesh_base);
            if let Some(extra) = merged {
                components.extend(self.component_pids.clone());
                comp_roles.extend(std::mem::take(&mut self.component_roles));
                extra_child_transforms.extend(extra);
            } else {
                self.attach_primitive(
                    scene,
                    go,
                    &node.primitives[0],
                    node.is_collider,
                    true,
                    &node_path,
                    &mesh_base,
                    node.extra_colliders,
                );
                components.extend(self.component_pids.clone());
                comp_roles.extend(std::mem::take(&mut self.component_roles));
                let child_base = if mesh_base.is_empty() {
                    "Primitive"
                } else {
                    mesh_base.as_str()
                };
                for pi in 1..node.primitives.len() {
                    let prim = &node.primitives[pi].clone();
                    let child_name = format!("{child_base}_{pi}");
                    let child_path = format!("{node_path}/{child_name}");
                    let cgo = self.npid();
                    let ctr = self.npid();
                    self.attach_primitive(
                        scene,
                        cgo,
                        prim,
                        node.is_collider,
                        false,
                        &child_path,
                        &mesh_base,
                        0,
                    );
                    let mut ccomp = vec![ctr];
                    ccomp.extend(self.component_pids.clone());
                    let go_tree = self.go_tree(&child_name, &ccomp);
                    self.set_obj(
                        cgo,
                        "GameObject",
                        go_tree,
                        Role::Glb("GameObject".into(), child_path.clone()),
                    );
                    let tr_tree = self.transform_tree(
                        cgo,
                        [0.0, 0.0, 0.0],
                        [0.0, 0.0, 0.0, 1.0],
                        [1.0, 1.0, 1.0],
                        &[],
                        tr,
                    );
                    self.set_obj(
                        ctr,
                        "Transform",
                        tr_tree,
                        Role::Glb("Transform".into(), format!("{child_path}/Transform")),
                    );
                    for (cp, cr) in std::mem::take(&mut self.component_roles) {
                        self.insert_role(cp, cr);
                    }
                    self.scene_object_pids.push(cgo);
                    self.scene_object_pids.push(ctr);
                    extra_child_transforms.push(ctr);
                }
            }
        }

        self.node_tr.insert(node_idx, tr);

        let mut child_transforms: Vec<i64> = Vec::new();
        for ci in &node.children {
            if *ci >= scene.nodes.len() {
                continue;
            }

            if self.visits_left.get(ci).copied().unwrap_or(1) > 1 {
                child_transforms.push(self.build_bare_node(scene, *ci, tr, &node_path));
                continue;
            }
            if self.node_tr.contains_key(ci) {
                continue;
            }
            child_transforms.push(self.build_node(scene, *ci, tr, &node_path).1);
        }
        child_transforms.extend(extra_child_transforms);

        let is_root = parent_tr == 0;
        let assigning_root = is_root && !self.bundle_root_assigned;
        let go_name = if assigning_root {
            self.bundle_root_assigned = true;
            self.root_hash.clone()
        } else {
            node_name
        };
        let go_tree = self.go_tree(&go_name, &components);
        self.set_obj(
            go,
            "GameObject",
            go_tree,
            Role::Glb("GameObject".into(), node_path.clone()),
        );
        for (p, r) in comp_roles {
            self.insert_role(p, r);
        }
        let (root_t, root_r, root_s) = match &self.lod {
            Some(l) if assigning_root => (l.root_position, [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0]),
            _ => (node.translation, node.rotation, node.scale),
        };
        let tr_tree = self.transform_tree(go, root_t, root_r, root_s, &child_transforms, parent_tr);

        self.set_obj(
            tr,
            "Transform",
            tr_tree,
            Role::Glb("Transform".into(), format!("{node_path}/Transform")),
        );
        self.scene_object_pids.push(go);
        self.scene_object_pids.push(tr);
        (go, tr)
    }

    fn build_extra_scene(&mut self, scene: &Scene, name: Option<&str>, roots: &[usize]) {
        self.count_scene_visits(scene, roots);
        let scene_name = name.unwrap_or("");
        let scene_path = format!("scenes/{scene_name}");
        let has_anim_component = self.proto.contains_key("AnimatorController")
            || self.proto.contains_key("AnimatorOverrideController")
            || self.proto.contains_key("Animation");
        let use_first_child = !has_anim_component && roots.len() == 1;
        if use_first_child {
            self.build_node(scene, roots[0], 0, &scene_path);
            return;
        }
        let wrap_inner = if scene_name.is_empty() {
            "Scene"
        } else {
            scene_name
        };
        let wrap_path = format!("{scene_path}/{wrap_inner}");
        let wrap_go = self.npid();
        let wrap_tr = self.npid();
        let mut child_trs: Vec<i64> = roots
            .iter()
            .map(|ri| self.build_node(scene, *ri, wrap_tr, &wrap_path).1)
            .collect();
        let has_animation_clips =
            !self.anim_clip_name_pids.is_empty() && self.proto.contains_key("Animation");
        if roots.is_empty() && has_animation_clips {
            let inner_name = if scene_name.is_empty() {
                "Scene"
            } else {
                scene_name
            };
            let inner_path = format!("{wrap_path}/{inner_name}");
            let inner_go = self.npid();
            let inner_tr = self.npid();
            let inner_tr_tree = self.transform_tree(
                inner_go,
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
                [1.0, 1.0, 1.0],
                &[],
                wrap_tr,
            );
            self.set_obj(
                inner_tr,
                "Transform",
                inner_tr_tree,
                Role::Glb("Transform".into(), format!("{inner_path}/Transform")),
            );
            let clips = self.anim_clip_name_pids.clone();
            let comp = animation::build_animation_component(inner_go, &clips);
            let acp = self.add(
                "Animation",
                comp,
                Role::Glb("Animation".into(), format!("{inner_path}/Animation")),
            );
            self.scene_object_pids.push(acp);
            let inner_go_tree = self.go_tree(inner_name, &[inner_tr, acp]);
            self.set_obj(
                inner_go,
                "GameObject",
                inner_go_tree,
                Role::Glb("GameObject".into(), inner_path),
            );
            self.scene_object_pids.push(inner_tr);
            self.scene_object_pids.push(inner_go);
            child_trs.push(inner_tr);
        }
        let tr_tree = self.transform_tree(
            wrap_go,
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            &child_trs,
            0,
        );
        self.set_obj(
            wrap_tr,
            "Transform",
            tr_tree,
            Role::Glb("Transform".into(), format!("{wrap_path}/Transform")),
        );
        let mut wrap_components = vec![wrap_tr];
        if !roots.is_empty() && has_animation_clips {
            let clips = self.anim_clip_name_pids.clone();
            let comp = animation::build_animation_component(wrap_go, &clips);
            let acp = self.add(
                "Animation",
                comp,
                Role::Glb("Animation".into(), format!("{wrap_path}/Animation")),
            );
            self.scene_object_pids.push(acp);
            wrap_components.push(acp);
        }
        let go_tree = self.go_tree(wrap_inner, &wrap_components);
        self.set_obj(
            wrap_go,
            "GameObject",
            go_tree,
            Role::Glb("GameObject".into(), wrap_path),
        );
        self.scene_object_pids.push(wrap_tr);
        self.scene_object_pids.push(wrap_go);
    }

    fn emit_orphan_node_assets(&mut self, scene: &Scene) {
        self.orphan_assets_emitted = true;
        let mut reachable: HashSet<usize> = HashSet::new();
        let mut stack: Vec<usize> = scene
            .root_nodes
            .iter()
            .chain(scene.extra_scenes.iter().flat_map(|(_, r)| r.iter()))
            .copied()
            .collect();
        while let Some(n) = stack.pop() {
            if n >= scene.nodes.len() || !reachable.insert(n) {
                continue;
            }
            stack.extend(scene.nodes[n].children.iter().copied());
        }
        for (idx, node) in scene.nodes.iter().enumerate() {
            if reachable.contains(&idx) {
                continue;
            }
            for prim in &node.primitives {
                let mesh_base = prim.name.clone();
                let mesh_pid = self.add_mesh(prim, 0, None, &mesh_base);
                self.scene_object_pids.push(mesh_pid);

                if prim.material_index.is_some() {
                    let _ = self.material_orphan(scene, prim.material_index);
                }
            }
        }
    }

    fn attach_primitive(
        &mut self,
        scene: &Scene,
        go_pid: i64,
        prim: &Primitive,
        is_collider: bool,
        is_parent_prim: bool,
        node_path: &str,
        mesh_base: &str,
        extra_colliders: usize,
    ) {
        let has_smr_proto = self.proto.contains_key("SkinnedMeshRenderer");
        let is_skinned_node = prim.skin_index.is_some() && prim.weights.is_some() && has_smr_proto;

        let has_morph_targets = !prim.morph_targets.is_empty() && has_smr_proto;
        let becomes_smr = is_skinned_node || has_morph_targets;

        let suppress_emission = is_parent_prim && is_collider && becomes_smr;
        let becomes_collider = is_collider && !becomes_smr;

        let usage: i64 = if becomes_smr {
            1
        } else if becomes_collider || self.mesh_collidable(prim) {
            16
        } else {
            0
        };
        let bind_poses: Option<Vec<[f64; 16]>> = if is_skinned_node {
            prim.skin_index
                .filter(|&si| si < scene.skins.len())
                .map(|si| scene.skins[si].bind_poses.clone())
        } else {
            None
        };

        let mesh_usage = if suppress_emission { 0 } else { usage };
        let mesh_pid = self.add_mesh(prim, mesh_usage, bind_poses.as_deref(), mesh_base);
        self.scene_object_pids.push(mesh_pid);

        if suppress_emission {
            if prim.material_index.is_some() {
                let _ = self.material_orphan(scene, prim.material_index);
            }
            self.component_pids = Vec::new();
            self.component_roles = Vec::new();
            return;
        }

        if becomes_collider {
            if prim.material_index.is_some() {
                let _ = self.material_orphan(scene, prim.material_index);
            }
            let mf = self.add(
                "MeshFilter",
                map! {"m_GameObject" => crate::value::pptr(0, go_pid), "m_Mesh" => crate::value::pptr(0, mesh_pid)},
                Role::Glb("MeshFilter".into(), format!("{node_path}/MeshFilter")),
            );
            let mut mc = self.base_clone("MeshCollider");
            mc.insert("m_GameObject", crate::value::pptr(0, go_pid));
            mc.insert("m_Mesh", crate::value::pptr(0, mesh_pid));
            let mc_pid = self.add(
                "MeshCollider",
                mc,
                Role::Glb("MeshCollider".into(), format!("{node_path}/MeshCollider")),
            );
            self.scene_object_pids.push(mf);
            self.scene_object_pids.push(mc_pid);
            self.component_pids = vec![mf, mc_pid];
            self.component_roles = Vec::new();

            let go_name = node_path.rsplit('/').next().unwrap_or("");

            let n_extra = extra_colliders
                + usize::from(!is_parent_prim && go_name.to_lowercase().contains("_collider"));
            for idx in 1..=n_extra {
                let mut mc2 = self.base_clone("MeshCollider");
                mc2.insert("m_GameObject", crate::value::pptr(0, go_pid));
                mc2.insert("m_Mesh", crate::value::pptr(0, mesh_pid));
                let mc2_pid = self.add(
                    "MeshCollider",
                    mc2,
                    Role::GlbIdx(
                        "MeshCollider".into(),
                        format!("{node_path}/MeshCollider"),
                        idx as u32,
                    ),
                );
                self.scene_object_pids.push(mc2_pid);
                self.component_pids.push(mc2_pid);
            }
            return;
        }

        if becomes_smr {
            let mat_pid = self.material(scene, prim.material_index);
            let smr_pid = self.npid();
            self.set_obj(
                smr_pid,
                "SkinnedMeshRenderer",
                Value::map(),
                Role::Glb(
                    "SkinnedMeshRenderer".into(),
                    format!("{node_path}/SkinnedMeshRenderer"),
                ),
            );
            self.pending_smr.push((
                smr_pid,
                go_pid,
                mesh_pid,
                mat_pid.into_iter().collect(),
                prim.skin_index,
                prim.morph_weights.clone(),
            ));
            self.scene_object_pids.push(smr_pid);
            self.component_pids = vec![smr_pid];
            self.component_roles = Vec::new();
            return;
        }

        let mf = self.add(
            "MeshFilter",
            map! {"m_GameObject" => crate::value::pptr(0, go_pid), "m_Mesh" => crate::value::pptr(0, mesh_pid)},
            Role::Glb("MeshFilter".into(), format!("{node_path}/MeshFilter")),
        );
        self.scene_object_pids.push(mf);
        let mat_pids: Vec<Value> = self
            .material(scene, prim.material_index)
            .into_iter()
            .map(|p| crate::value::pptr(0, p))
            .collect();
        let mut mr = self.base_clone("MeshRenderer");
        mr.insert("m_GameObject", crate::value::pptr(0, go_pid));
        mr.insert("m_Materials", Value::Array(mat_pids));
        let mr_pid = self.add(
            "MeshRenderer",
            mr,
            Role::Glb("MeshRenderer".into(), format!("{node_path}/MeshRenderer")),
        );
        self.scene_object_pids.push(mr_pid);
        self.component_pids = vec![mf, mr_pid];
        self.component_roles = Vec::new();
    }

    pub(super) fn go_tree(&self, name: &str, component_pids: &[i64]) -> Value {
        let comps: Vec<Value> = component_pids
            .iter()
            .map(|p| map! {"component" => crate::value::pptr(0, *p)})
            .collect();
        map! {
            "m_Component" => Value::Array(comps),
            "m_Layer" => 0,
            "m_Name" => name,
            "m_Tag" => 0,
            "m_IsActive" => true,
        }
    }

    pub(super) fn transform_tree(
        &self,
        go_pid: i64,
        t: [f64; 3],
        r: [f64; 4],
        s: [f64; 3],
        children: &[i64],
        father: i64,
    ) -> Value {
        let kids: Vec<Value> = children.iter().map(|c| crate::value::pptr(0, *c)).collect();
        map! {
            "m_GameObject" => crate::value::pptr(0, go_pid),
            "m_LocalRotation" => map!{"x" => r[0], "y" => r[1], "z" => r[2], "w" => r[3]},
            "m_LocalPosition" => map!{"x" => t[0], "y" => t[1], "z" => t[2]},
            "m_LocalScale" => map!{"x" => s[0], "y" => s[1], "z" => s[2]},
            "m_Children" => Value::Array(kids),
            "m_Father" => crate::value::pptr(0, father),
        }
    }
}
