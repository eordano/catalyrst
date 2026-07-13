use super::*;

impl<'a> Builder<'a> {
    fn mesh_tree(
        &self,
        prim: &Primitive,
        usage_flags: i64,
        bind_poses: Option<&[[f64; 16]]>,
    ) -> Value {
        let mut t = self.base_clone("Mesh");
        let n = prim.positions.len();

        let use_u16 = prim.from_draco && n <= u16::MAX as usize + 1;
        let mut idx_bytes: Vec<u8> =
            Vec::with_capacity(prim.indices.len() * if use_u16 { 2 } else { 4 });
        if use_u16 {
            for i in &prim.indices {
                idx_bytes.extend_from_slice(&(*i as u16).to_le_bytes());
            }
        } else {
            for i in &prim.indices {
                idx_bytes.extend_from_slice(&i.to_le_bytes());
            }
        }
        let (data, channels) = gltf::vertex_buffer(prim);
        let (center, extent) = gltf::aabb(
            &prim.positions,
            prim.position_min_decl,
            prim.position_max_decl,
        );
        t.insert("m_Name", prim.name.clone());
        if let Some(bps) = bind_poses {
            let bp: Vec<Value> = bps
                .iter()
                .map(|bp| mesh_layout::bind_pose_tree(*bp))
                .collect();
            t.insert("m_BindPose", Value::Array(bp));
            if let (Some(weights), Some(joints)) = (&prim.weights, &prim.joints) {
                t.insert(
                    "m_BonesAABB",
                    mesh_layout::compute_bones_aabb(
                        &prim.positions,
                        weights,
                        joints,
                        bps,
                        &prim.morph_targets,
                    ),
                );
            }
        }
        let submesh = map! {
            "firstByte" => 0,
            "indexCount" => prim.indices.len() as i64,
            "topology" => 0,
            "baseVertex" => 0,
            "firstVertex" => 0,
            "vertexCount" => n as i64,
            "localAABB" => map!{"m_Center" => center.clone(), "m_Extent" => extent.clone()},
        };
        t.insert("m_SubMeshes", Value::Array(vec![submesh]));
        t.insert("m_IndexFormat", if use_u16 { 0i64 } else { 1i64 });
        t.insert("m_IndexBuffer", Value::Bytes(idx_bytes));
        t.insert(
            "m_VertexData",
            map! {
                "m_VertexCount" => n as i64,
                "m_Channels" => Value::Array(channels),
                "m_DataSize" => Value::Bytes(data),
            },
        );
        t.insert(
            "m_LocalAABB",
            map! {"m_Center" => center, "m_Extent" => extent},
        );

        let has_morph = !prim.morph_targets.is_empty();
        let (m_shapes, _has_shapes) =
            mesh_layout::build_m_shapes(&prim.morph_targets, &prim.morph_target_names);
        t.insert("m_Shapes", m_shapes);
        let final_usage = if has_morph { 1 } else { usage_flags };
        t.insert("m_MeshUsageFlags", final_usage);
        if has_morph {
            t.insert("m_KeepVertices", true);
        }
        if final_usage == 36 {
            t.insert("m_KeepIndices", true);
            t.insert("m_KeepVertices", true);
        }
        t.insert(
            "m_StreamData",
            map! {"offset" => 0, "size" => 0, "path" => ""},
        );
        t
    }

    pub(super) fn unique_recycle(&mut self, prefix: &str, name: &str) -> String {
        let key = format!("{prefix}/{name}");
        let n = *self.recycle_seen.get(&key).unwrap_or(&0);
        self.recycle_seen.insert(key.clone(), n + 1);
        if n == 0 {
            key
        } else {
            format!("{key}_{}", n - 1)
        }
    }

    fn register_lod_mesh(&mut self, pid: i64, prim: &Primitive) {
        if self.lod.is_none() {
            return;
        }
        let name = if !prim.name.is_empty() {
            prim.name.clone()
        } else {
            let assigned = prim.gltf_mesh_index.and_then(|mi| {
                self.lod_mesh_names
                    .get(&(mi, prim.gltf_prim_index))
                    .cloned()
            });
            match assigned {
                Some(n) => n,
                None => {
                    let k = self.lod_mesh_entries.len();
                    format!("mesh_{k}_{k}")
                }
            }
        };
        self.lod_mesh_entries.push((format!("{name}.asset"), pid));
        if let Some((_tn, tree)) = self.objects.get_mut(&pid) {
            tree.insert("m_Name", name);
        }
    }

    pub(super) fn add_mesh(
        &mut self,
        prim: &Primitive,
        usage: i64,
        bind_poses: Option<&[[f64; 16]]>,
        mesh_base: &str,
    ) -> i64 {
        if let Some(mi) = prim.gltf_mesh_index {
            let key = (mi, prim.gltf_prim_index, usage, prim.skin_index);
            if let Some(&pid) = self.mesh_pid_by_gltf.get(&key) {
                return pid;
            }
            let recycle = self.unique_recycle("meshes", mesh_base);
            let pid = self.add(
                "Mesh",
                self.mesh_tree(prim, usage, bind_poses),
                Role::Glb("Mesh".into(), recycle),
            );
            self.mesh_pid_by_gltf.insert(key, pid);
            self.register_lod_mesh(pid, prim);
            return pid;
        }
        let recycle = self.unique_recycle("meshes", mesh_base);
        let pid = self.add(
            "Mesh",
            self.mesh_tree(prim, usage, bind_poses),
            Role::Glb("Mesh".into(), recycle),
        );
        self.register_lod_mesh(pid, prim);
        pid
    }

    fn mesh_tree_merged(
        &self,
        prims: &[&Primitive],
        usage_flags: i64,
        bind_poses: Option<&[[f64; 16]]>,
    ) -> Value {
        let p0 = prims[0];
        let mut t = self.base_clone("Mesh");
        let n = p0.positions.len();
        let (data, channels) = gltf::vertex_buffer(p0);
        let (center, extent) =
            gltf::aabb(&p0.positions, p0.position_min_decl, p0.position_max_decl);

        let merged_from_draco = prims.iter().all(|p| p.from_draco);
        let use_u16 = merged_from_draco && n <= u16::MAX as usize + 1;
        let idx_width: i64 = if use_u16 { 2 } else { 4 };
        let mut idx_bytes: Vec<u8> = Vec::new();
        let mut submeshes: Vec<Value> = Vec::with_capacity(prims.len());
        let mut first_byte: i64 = 0;
        for p in prims {
            if use_u16 {
                for i in &p.indices {
                    idx_bytes.extend_from_slice(&(*i as u16).to_le_bytes());
                }
            } else {
                for i in &p.indices {
                    idx_bytes.extend_from_slice(&i.to_le_bytes());
                }
            }
            let count = p.indices.len() as i64;
            submeshes.push(map! {
                "firstByte" => first_byte,
                "indexCount" => count,
                "topology" => 0,
                "baseVertex" => 0,
                "firstVertex" => 0,
                "vertexCount" => n as i64,
                "localAABB" => map!{"m_Center" => center.clone(), "m_Extent" => extent.clone()},
            });
            first_byte += count * idx_width;
        }
        t.insert("m_Name", p0.name.clone());
        if let Some(bps) = bind_poses {
            let bp: Vec<Value> = bps
                .iter()
                .map(|bp| mesh_layout::bind_pose_tree(*bp))
                .collect();
            t.insert("m_BindPose", Value::Array(bp));
            if let (Some(weights), Some(joints)) = (&p0.weights, &p0.joints) {
                t.insert(
                    "m_BonesAABB",
                    mesh_layout::compute_bones_aabb(
                        &p0.positions,
                        weights,
                        joints,
                        bps,
                        &p0.morph_targets,
                    ),
                );
            }
        }
        t.insert("m_SubMeshes", Value::Array(submeshes));
        t.insert("m_IndexFormat", if use_u16 { 0i64 } else { 1i64 });
        t.insert("m_IndexBuffer", Value::Bytes(idx_bytes));
        t.insert(
            "m_VertexData",
            map! {
                "m_VertexCount" => n as i64,
                "m_Channels" => Value::Array(channels),
                "m_DataSize" => Value::Bytes(data),
            },
        );
        t.insert(
            "m_LocalAABB",
            map! {"m_Center" => center, "m_Extent" => extent},
        );

        let lod_subs: Vec<Value> = (0..prims.len())
            .map(|_| {
                map! {
                    "m_Levels" => Value::Array(vec![
                        map!{"m_IndexStart" => 0, "m_IndexCount" => 0}
                    ]),
                }
            })
            .collect();
        let mut lod_info = t.get("m_MeshLodInfo").cloned().unwrap_or_else(Value::map);
        lod_info.insert("m_SubMeshes", Value::Array(lod_subs));
        t.insert("m_MeshLodInfo", lod_info);

        let has_morph = !p0.morph_targets.is_empty();
        let (m_shapes, _has_shapes) =
            mesh_layout::build_m_shapes(&p0.morph_targets, &p0.morph_target_names);
        t.insert("m_Shapes", m_shapes);
        let final_usage = if has_morph { 1 } else { usage_flags };
        t.insert("m_MeshUsageFlags", final_usage);
        if has_morph {
            t.insert("m_KeepVertices", true);
        }
        if final_usage == 36 {
            t.insert("m_KeepIndices", true);
            t.insert("m_KeepVertices", true);
        }
        t.insert(
            "m_StreamData",
            map! {"offset" => 0, "size" => 0, "path" => ""},
        );
        t
    }

    fn add_mesh_merged(&mut self, prims: &[Primitive], mesh_base: &str, usage: i64) -> i64 {
        let recycle = self.unique_recycle("meshes", mesh_base);
        let refs: Vec<&Primitive> = prims.iter().collect();
        let tree = self.mesh_tree_merged(&refs, usage, None);
        let pid = self.add("Mesh", tree, Role::Glb("Mesh".into(), recycle));
        if let Some(mi) = prims[0].gltf_mesh_index {
            let key = (mi, prims[0].gltf_prim_index, usage, prims[0].skin_index);
            self.mesh_pid_by_gltf.entry(key).or_insert(pid);
        }
        self.register_lod_mesh(pid, &prims[0]);
        pid
    }

    fn add_mesh_merged_v38(
        &mut self,
        prims: &[&Primitive],
        mesh_base: &str,
        usage: i64,
        bind_poses: Option<&[[f64; 16]]>,
    ) -> i64 {
        let p0 = prims[0];
        if let Some(mi) = p0.gltf_mesh_index {
            let key = (mi, p0.gltf_prim_index, usage, p0.skin_index);
            if let Some(&pid) = self.mesh_pid_by_gltf.get(&key) {
                return pid;
            }
        }
        let recycle = self.unique_recycle("meshes", mesh_base);
        let tree = self.mesh_tree_merged(prims, usage, bind_poses);
        let pid = self.add("Mesh", tree, Role::Glb("Mesh".into(), recycle));
        if let Some(mi) = p0.gltf_mesh_index {
            let key = (mi, p0.gltf_prim_index, usage, p0.skin_index);
            self.mesh_pid_by_gltf.insert(key, pid);
        }
        self.register_lod_mesh(pid, p0);
        pid
    }

    pub(super) fn try_attach_primitives_merged(
        &mut self,
        scene: &Scene,
        go_pid: i64,
        parent_tr: i64,
        node: &crate::scene::Node,
        node_path: &str,
        mesh_base: &str,
    ) -> Option<Vec<i64>> {
        let prims = &node.primitives;
        if prims.len() < 2 {
            return None;
        }
        if self.toggles.v38_compat {
            return self
                .try_attach_clusters_v38(scene, go_pid, parent_tr, node, node_path, mesh_base);
        }
        let sig0 = prims[0].gltf_attr_sig.as_ref()?.clone();
        if prims[0].skin_index.is_some() || !prims[0].morph_targets.is_empty() {
            return None;
        }
        for p in &prims[1..] {
            match &p.gltf_attr_sig {
                Some(s) if *s == sig0 => {}
                _ => return None,
            }
            if p.skin_index.is_some() || !p.morph_targets.is_empty() {
                return None;
            }
        }

        if node.is_collider {
            let usage: i64 = 16;
            let mesh_pid = self.add_mesh_merged(prims, mesh_base, usage);
            self.scene_object_pids.push(mesh_pid);

            for p in prims {
                if p.material_index.is_some() {
                    let _ = self.material_orphan(scene, p.material_index);
                }
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
            return Some(Vec::new());
        }

        let usage: i64 = if self.mesh_collidable(&prims[0]) {
            16
        } else {
            0
        };
        let mesh_pid = self.add_mesh_merged(prims, mesh_base, usage);
        self.scene_object_pids.push(mesh_pid);
        let mf = self.add(
            "MeshFilter",
            map! {"m_GameObject" => crate::value::pptr(0, go_pid), "m_Mesh" => crate::value::pptr(0, mesh_pid)},
            Role::Glb("MeshFilter".into(), format!("{node_path}/MeshFilter")),
        );
        self.scene_object_pids.push(mf);
        let mat_pids: Vec<i64> = prims
            .iter()
            .filter_map(|p| self.material(scene, p.material_index))
            .collect();
        let mut mr = self.base_clone("MeshRenderer");
        mr.insert("m_GameObject", crate::value::pptr(0, go_pid));
        mr.insert(
            "m_Materials",
            Value::Array(mat_pids.iter().map(|p| crate::value::pptr(0, *p)).collect()),
        );
        let mr_pid = self.add(
            "MeshRenderer",
            mr,
            Role::Glb("MeshRenderer".into(), format!("{node_path}/MeshRenderer")),
        );
        self.scene_object_pids.push(mr_pid);
        self.component_pids = vec![mf, mr_pid];
        self.component_roles = Vec::new();
        Some(Vec::new())
    }

    fn try_attach_clusters_v38(
        &mut self,
        scene: &Scene,
        go_pid: i64,
        parent_tr: i64,
        node: &crate::scene::Node,
        node_path: &str,
        mesh_base: &str,
    ) -> Option<Vec<i64>> {
        type ClusterKey = (
            Option<i64>,
            Option<i64>,
            Option<i64>,
            Vec<i64>,
            Option<i64>,
            Vec<crate::scene::MorphSig>,
        );
        let prims = &node.primitives;
        let mut clusters: Vec<(ClusterKey, Vec<&Primitive>)> = Vec::new();
        for p in prims {
            let sig = p.gltf_attr_sig.as_ref()?;
            let key: ClusterKey = (
                sig.position,
                sig.normal,
                sig.tangent,
                sig.texcoords.clone(),
                sig.color,
                p.gltf_morph_sig.clone(),
            );
            match clusters.iter_mut().find(|(k, _)| *k == key) {
                Some((_, v)) => v.push(p),
                None => clusters.push((key, vec![p])),
            }
        }

        if clusters.iter().all(|(_, v)| v.len() == 1) {
            return None;
        }

        self.attach_cluster_v38(
            scene,
            go_pid,
            &clusters[0].1,
            node.is_collider,
            true,
            node_path,
            mesh_base,
            node.extra_colliders,
        );
        let node_components = self.component_pids.clone();
        let node_roles = std::mem::take(&mut self.component_roles);

        let mut extra_child_transforms: Vec<i64> = Vec::new();
        let child_base = if mesh_base.is_empty() {
            "Primitive"
        } else {
            mesh_base
        };
        for (ci, (_, cluster)) in clusters.iter().enumerate().skip(1) {
            let child_name = format!("{child_base}_{ci}");
            let child_path = format!("{node_path}/{child_name}");
            let cgo = self.npid();
            let ctr = self.npid();
            self.attach_cluster_v38(
                scene,
                cgo,
                cluster,
                node.is_collider,
                false,
                &child_path,
                mesh_base,
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
                parent_tr,
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

        self.component_pids = node_components;
        self.component_roles = node_roles;
        Some(extra_child_transforms)
    }

    fn attach_cluster_v38(
        &mut self,
        scene: &Scene,
        go_pid: i64,
        prims: &[&Primitive],
        is_collider: bool,
        is_parent_prim: bool,
        node_path: &str,
        mesh_base: &str,
        extra_colliders: usize,
    ) {
        let p0 = prims[0];
        let has_smr_proto = self.proto.contains_key("SkinnedMeshRenderer");
        let is_skinned_node = p0.skin_index.is_some() && p0.weights.is_some() && has_smr_proto;
        let has_morph_targets = !p0.morph_targets.is_empty() && has_smr_proto;
        let becomes_smr = is_skinned_node || has_morph_targets;

        let suppress_emission = is_parent_prim && is_collider && becomes_smr;
        let becomes_collider = is_collider && !becomes_smr;

        let usage: i64 = if becomes_smr {
            1
        } else if becomes_collider || self.mesh_collidable(p0) {
            16
        } else {
            0
        };
        let bind_poses: Option<Vec<[f64; 16]>> = if is_skinned_node {
            p0.skin_index
                .filter(|&si| si < scene.skins.len())
                .map(|si| scene.skins[si].bind_poses.clone())
        } else {
            None
        };

        let mesh_usage = if suppress_emission { 0 } else { usage };
        let mesh_pid =
            self.add_mesh_merged_v38(prims, mesh_base, mesh_usage, bind_poses.as_deref());
        self.scene_object_pids.push(mesh_pid);

        if suppress_emission {
            for p in prims {
                if p.material_index.is_some() {
                    let _ = self.material_orphan(scene, p.material_index);
                }
            }
            self.component_pids = Vec::new();
            self.component_roles = Vec::new();
            return;
        }

        if becomes_collider {
            for p in prims {
                if p.material_index.is_some() {
                    let _ = self.material_orphan(scene, p.material_index);
                }
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
            let mat_pids: Vec<i64> = prims
                .iter()
                .filter_map(|p| self.material(scene, p.material_index))
                .collect();
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
                mat_pids,
                p0.skin_index,
                p0.morph_weights.clone(),
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
        let mat_pids: Vec<i64> = prims
            .iter()
            .filter_map(|p| self.material(scene, p.material_index))
            .collect();
        let mut mr = self.base_clone("MeshRenderer");
        mr.insert("m_GameObject", crate::value::pptr(0, go_pid));
        mr.insert(
            "m_Materials",
            Value::Array(mat_pids.iter().map(|p| crate::value::pptr(0, *p)).collect()),
        );
        let mr_pid = self.add(
            "MeshRenderer",
            mr,
            Role::Glb("MeshRenderer".into(), format!("{node_path}/MeshRenderer")),
        );
        self.scene_object_pids.push(mr_pid);
        self.component_pids = vec![mf, mr_pid];
        self.component_roles = Vec::new();
    }

    pub(super) fn mesh_collidable(&self, prim: &Primitive) -> bool {
        match prim.gltf_mesh_index {
            Some(mi) => {
                self.collidable_mesh_keys
                    .contains(&(mi, prim.gltf_prim_index, prim.skin_index))
            }
            None => false,
        }
    }

    pub(super) fn collect_collidable_mesh_keys(&mut self, scene: &Scene) {
        self.collidable_mesh_keys.clear();
        for node in &scene.nodes {
            if !node.is_collider {
                continue;
            }
            for p in &node.primitives {
                if let Some(mi) = p.gltf_mesh_index {
                    self.collidable_mesh_keys
                        .insert((mi, p.gltf_prim_index, p.skin_index));
                }
            }
        }
    }
}
