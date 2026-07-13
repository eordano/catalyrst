use super::templates::cab_node_name;
use super::*;

pub(super) enum ExternalsPolicy<'a> {
    ShaderRef {
        ext_bundle_files: &'a [String],
        shader_cab: String,
    },
    Clear,
}

fn collect_pptr_first_use(
    value: &Value,
    node: &crate::unity::typetree_node::TypeTreeNode,
    order: &mut Vec<i64>,
) {
    if node.m_Type.starts_with("PPtr<") {
        if let Some(m) = value.as_map() {
            if let Some(fid) = m.get("m_FileID").and_then(|v| v.as_i64()) {
                if fid >= 1 && !order.contains(&fid) {
                    order.push(fid);
                }
            }
        }
        return;
    }
    match value {
        Value::Map(m) => {
            for child in &node.m_Children {
                if let Some(cv) = m.get(&child.m_Name) {
                    collect_pptr_first_use(cv, child, order);
                }
            }
        }
        Value::Array(a) => {
            if node.m_Type == "pair" {
                for (v, child) in a.iter().zip(node.m_Children.iter()) {
                    collect_pptr_first_use(v, child, order);
                }
            } else if !node.m_Children.is_empty() && node.m_Children[0].m_Type == "Array" {
                let subtype = &node.m_Children[0].m_Children[1];
                for v in a {
                    collect_pptr_first_use(v, subtype, order);
                }
            }
        }
        _ => {}
    }
}

fn remap_pptr_fids(value: &mut Value, remap: &HashMap<i64, i64>) {
    match value {
        Value::Map(m) => {
            if m.len() == 2 && m.get("m_PathID").is_some() {
                if let Some(old) = m.get("m_FileID").and_then(|v| v.as_i64()) {
                    if let Some(&new) = remap.get(&old) {
                        if new != old {
                            m.insert("m_FileID", Value::Int(new));
                        }
                    }
                }
                return;
            }
            for i in 0..m.len() {
                remap_pptr_fids(&mut m.0[i].1, remap);
            }
        }
        Value::Array(a) => {
            for v in a {
                remap_pptr_fids(v, remap);
            }
        }
        _ => {}
    }
}

pub(super) fn commit_objects(
    bundle: &mut Bundle,
    bundle_name: &str,
    target: &str,
    proto: &HashMap<String, SerializedType>,
    objects: &BTreeMap<i64, (String, Value)>,
    blobs: &[ress::TextureBlob],
    inline_pids: &HashSet<i64>,
    externals: ExternalsPolicy<'_>,
) -> Result<()> {
    let big_endian = bundle.serialized().map(|sf| sf.big_endian).unwrap_or(false);

    let new_cab = cabname::cab_name(bundle_name);
    let old_cab = cab_node_name(bundle)?;
    if new_cab != old_cab {
        for e in bundle.files.iter_mut() {
            if e.name.starts_with(&old_cab) {
                e.name = format!("{new_cab}{}", &e.name[old_cab.len()..]);
            }
        }
    }
    let cab = cab_node_name(bundle)?;

    let webgl_inline_all = target == "webgl";
    let built = if blobs.is_empty() {
        None
    } else {
        let pred = |t: &ress::TextureBlob| webgl_inline_all || inline_pids.contains(&t.path_id);
        let predicate: Option<&dyn Fn(&ress::TextureBlob) -> bool> =
            if !webgl_inline_all && inline_pids.is_empty() {
                None
            } else {
                Some(&pred)
            };
        Some(ress::build_ress(blobs, &cab, predicate))
    };

    let mut overrides: HashMap<i64, Value> = HashMap::new();
    if let Some(b) = &built {
        for (pid, sd) in &b.stream_data {
            if let Some((tn, tree)) = objects.get(pid) {
                if tn == "Texture2D" {
                    let mut tree = tree.clone();
                    tree.insert(
                        "m_StreamData",
                        map! {"offset" => sd.offset as i64, "size" => sd.size as i64, "path" => sd.path.clone()},
                    );
                    if !sd.path.is_empty() {
                        tree.insert("image data", Value::Bytes(Vec::new()));
                    }
                    overrides.insert(*pid, tree);
                }
            }
        }
    }

    let mut ext_perm: Option<HashMap<i64, i64>> = None;
    if let ExternalsPolicy::ShaderRef {
        ext_bundle_files, ..
    } = &externals
    {
        let n_ext = 1 + ext_bundle_files.len() as i64;
        if n_ext > 1 {
            let mut first_use: Vec<i64> = Vec::new();
            for (type_name, tree) in objects.values() {
                if let Some(node) = proto.get(type_name).and_then(|st| st.node.as_ref()) {
                    collect_pptr_first_use(tree, node, &mut first_use);
                }
            }

            for fid in 1..=n_ext {
                if !first_use.contains(&fid) {
                    first_use.push(fid);
                }
            }
            if first_use
                .iter()
                .enumerate()
                .any(|(i, &f)| f != i as i64 + 1)
            {
                let remap: HashMap<i64, i64> = first_use
                    .iter()
                    .enumerate()
                    .map(|(i, &old)| (old, i as i64 + 1))
                    .collect();
                for (pid, (_tn, tree)) in objects.iter() {
                    let mut t = overrides.remove(pid).unwrap_or_else(|| tree.clone());
                    remap_pptr_fids(&mut t, &remap);
                    overrides.insert(*pid, t);
                }
                ext_perm = Some(remap);
            }
        }
    }

    let mut used_types: Vec<SerializedType> = Vec::new();
    let mut type_index: HashMap<String, i32> = HashMap::new();
    let mut out_objects: Vec<unity::Object> = Vec::new();
    for (pid, (type_name, tree)) in objects.iter() {
        let tree = overrides.get(pid).unwrap_or(tree);
        if !type_index.contains_key(type_name) {
            let st = proto
                .get(type_name)
                .ok_or_else(|| anyhow!("no proto type for {type_name}"))?
                .clone();
            type_index.insert(type_name.clone(), used_types.len() as i32);
            used_types.push(st);
        }
        let tid = type_index[type_name];
        let node = used_types[tid as usize]
            .node
            .as_ref()
            .ok_or_else(|| anyhow!("type {type_name} has no node"))?;
        let data = unity::write_typetree(tree, node, big_endian);
        out_objects.push(unity::Object {
            path_id: *pid,
            type_id: tid,
            class_id: used_types[tid as usize].class_id,
            type_name: type_name.clone(),
            data,
        });
    }

    let shcab = match &externals {
        ExternalsPolicy::ShaderRef { shader_cab, .. } => shader_cab.clone(),
        ExternalsPolicy::Clear => cabname::shader_bundle_cab(target).to_string(),
    };
    {
        let sf = bundle
            .serialized_mut()
            .ok_or_else(|| anyhow!("bundle has no serialized file"))?;
        sf.types = used_types;
        sf.objects = out_objects;
        match externals {
            ExternalsPolicy::ShaderRef {
                ext_bundle_files, ..
            } => {
                if !sf.externals.is_empty() {
                    let mut shader_ext = sf.externals[0].clone();
                    shader_ext.path = format!("archive:/{shcab}/{shcab}");
                    let mut new_ext = vec![shader_ext.clone()];
                    for bf in ext_bundle_files {
                        let cab = cabname::cab_name(bf);
                        let mut fi = shader_ext.clone();
                        fi.path = format!("archive:/{cab}/{cab}");
                        fi.guid = [0u8; 16];
                        fi.r#type = 0;
                        new_ext.push(fi);
                    }

                    if let Some(remap) = &ext_perm {
                        let mut permuted = new_ext.clone();
                        for (i, fi) in new_ext.into_iter().enumerate() {
                            let new_fid = remap[&(i as i64 + 1)];
                            permuted[(new_fid - 1) as usize] = fi;
                        }
                        new_ext = permuted;
                    }
                    sf.externals = new_ext;
                }
            }
            ExternalsPolicy::Clear => {
                sf.externals = Vec::new();
            }
        }
        if let Some(tp) = target_platform_for(target) {
            sf.target_platform = tp;
        }
    }

    if let Some(b) = built.as_ref().filter(|b| !b.payload.is_empty()) {
        let target_name = ress::ress_node_name(&cab);

        bundle.files.retain(|e| {
            let lower = e.name.to_lowercase();
            !(lower.ends_with(".ress") && e.name != target_name)
        });
        if let Some(e) = bundle.files.iter_mut().find(|e| e.name == target_name) {
            e.content = unity::bundle_file::FileContent::Raw(b.payload.clone());
            e.flags = ress::RESS_NODE_FLAGS;
        } else {
            bundle.files.push(unity::bundle_file::BundleEntry {
                name: target_name,
                content: unity::bundle_file::FileContent::Raw(b.payload.clone()),
                flags: ress::RESS_NODE_FLAGS,
            });
        }
    } else {
        let ress_nodes: Vec<String> = bundle
            .files
            .iter()
            .filter(|e| e.name.to_lowercase().ends_with(".ress"))
            .map(|e| e.name.clone())
            .collect();
        for name in ress_nodes {
            bundle.remove_file(&name);
        }
    }
    Ok(())
}

impl<'a> Builder<'a> {
    pub(super) fn build_assetbundle(&mut self) {
        let mut ab = self.base_clone("AssetBundle");

        let lower = self.bundle_name.to_ascii_lowercase();
        ab.insert("m_Name", lower.clone());
        ab.insert("m_AssetBundleName", lower);
        self.ab_pid = self.npid();
        self.set_obj(self.ab_pid, "AssetBundle", ab, Role::Bundle);
    }

    pub(super) fn finalize_pathids(&mut self) -> Result<()> {
        let mut old2new: HashMap<i64, i64> = HashMap::new();
        for (&old_pid, role) in self.roles.iter() {
            old2new.insert(old_pid, self.resolve_pathid(role));
        }

        let mut seen: HashMap<i64, i64> = HashMap::new();
        for (&old, &new) in old2new.iter() {
            if let Some(&prev) = seen.get(&new) {
                if prev != old {
                    return Err(anyhow!(
                        "PathID collision {new}: roles {:?} and {:?}",
                        self.roles.get(&prev),
                        self.roles.get(&old)
                    ));
                }
            }
            seen.insert(new, old);
        }

        fn remap(node: &mut Value, m: &HashMap<i64, i64>) {
            match node {
                Value::Map(map) => {
                    let is_pptr = map.len() == 2
                        && map.contains_key("m_FileID")
                        && map.contains_key("m_PathID");
                    if is_pptr {
                        let fid = map.get("m_FileID").and_then(|v| v.as_i64()).unwrap_or(0);
                        let pid = map.get("m_PathID").and_then(|v| v.as_i64()).unwrap_or(0);
                        if fid == 0 {
                            if let Some(&np) = m.get(&pid) {
                                map.insert("m_PathID", np);
                            }
                        }
                        return;
                    }
                    for (_, v) in map.0.iter_mut() {
                        remap(v, m);
                    }
                }
                Value::Array(a) => {
                    for v in a.iter_mut() {
                        remap(v, m);
                    }
                }
                _ => {}
            }
        }

        let mut new_objects: BTreeMap<i64, (String, Value)> = BTreeMap::new();
        for (old_pid, (tn, mut tree)) in std::mem::take(&mut self.objects) {
            remap(&mut tree, &old2new);
            let np = *old2new.get(&old_pid).unwrap();
            new_objects.insert(np, (tn, tree));
        }
        self.order = self
            .order
            .iter()
            .map(|p| *old2new.get(p).unwrap())
            .collect();
        self.objects = new_objects;

        let map_pid = |p: &i64| *old2new.get(p).unwrap();
        self.scene_object_pids = self.scene_object_pids.iter().map(map_pid).collect();
        self.root_go_pid = old2new[&self.root_go_pid];
        self.material_entries = self
            .material_entries
            .iter()
            .map(|(k, p, deps)| (k.clone(), old2new[p], deps.iter().map(map_pid).collect()))
            .collect();

        self.mat_external_pptrs = self
            .mat_external_pptrs
            .iter()
            .map(|(p, v)| (old2new[p], v.clone()))
            .collect();
        self.texture_entries = self
            .texture_entries
            .iter()
            .map(|(k, p)| (k.clone(), old2new[p]))
            .collect();
        self.lod_mesh_entries = self
            .lod_mesh_entries
            .iter()
            .map(|(k, p)| (k.clone(), old2new[p]))
            .collect();
        self.glb_referenced_mats = self.glb_referenced_mats.iter().map(map_pid).collect();
        self.animator_controller_entry = self
            .animator_controller_entry
            .take()
            .map(|(c, clips)| (old2new[&c], clips.iter().map(map_pid).collect()));
        if self.meta_pid != 0 {
            self.meta_pid = old2new[&self.meta_pid];
        }
        self.ab_pid = old2new[&self.ab_pid];

        self.force_inline_tex = self.force_inline_tex.iter().map(map_pid).collect();

        let ab_tree = self.fill_assetbundle();
        if let Some(slot) = self.objects.get_mut(&self.ab_pid) {
            slot.1 = ab_tree;
        }
        Ok(())
    }

    pub(super) fn metadata_script_json(&self) -> String {
        let mut deps: Vec<String> = if self.lod.is_some() {
            if self.material_entries.is_empty() {
                Vec::new()
            } else {
                vec![crate::shader::texarray_bundle_name(self.target)]
            }
        } else {
            self.metadata_dependencies.to_vec()
        };
        if self.lod.is_none() {
            for f in &self.ext_bundle_files {
                if !deps.contains(f) {
                    deps.push(f.clone());
                }
            }
            if self.toggles.v38_compat {
                for d in &mut deps {
                    *d = d.to_ascii_lowercase();
                }
                if !self.material_entries.is_empty() {
                    deps.push(format!("dcl/scene_ignore_{}", self.target));
                }
            }
        }
        deps.sort_unstable_by(|x, y| natural_bundle_cmp(x, y));
        if self.toggles.v38_compat {
            deps.dedup();
        }
        let deps_json: String = {
            let parts: Vec<String> = deps
                .iter()
                .map(|d| serde_json::to_string(d).expect("serialize metadata dep"))
                .collect();
            format!("[{}]", parts.join(","))
        };
        let (version, ts, main_asset): (&str, i64, String) = match &self.lod {
            Some(l) => (
                "1.0",
                l.timestamp
                    .unwrap_or_else(|| metadata_timestamp(self.toggles)),
                l.main_asset.clone(),
            ),
            None => (
                metadata_version_for_target(self.target, self.toggles.v38_compat),
                metadata_timestamp(self.toggles),
                String::new(),
            ),
        };
        format!(
            "{{\"timestamp\":{ts},\"version\":\"{version}\",\"dependencies\":{deps_json},\"mainAsset\":\"{main_asset}\"}}"
        )
    }

    pub(super) fn retarget(&mut self, bundle_name: &str) {
        let old_suffix = format!("_{}", self.target);
        self.bundle_name = bundle_name.to_string();
        self.target = target_from_bundle_name(bundle_name);
        let new_suffix = format!("_{}", self.target);
        let swap = |s: &str| -> String {
            s.strip_suffix(old_suffix.as_str())
                .map(|stem| format!("{stem}{new_suffix}"))
                .unwrap_or_else(|| s.to_string())
        };
        for f in &mut self.ext_bundle_files {
            *f = swap(f);
        }
        for d in &mut self.metadata_dependencies {
            *d = swap(d);
        }
        self.ext_bundle_fileid = self
            .ext_bundle_fileid
            .drain()
            .map(|(k, v)| (swap(&k), v))
            .collect();
        if self.meta_pid != 0 {
            let script = self.metadata_script_json();
            if let Some((_, tree)) = self.objects.get_mut(&self.meta_pid) {
                tree.insert("m_Script", script);
            }
        }
        let ab_tree = self.fill_assetbundle();
        if let Some(slot) = self.objects.get_mut(&self.ab_pid) {
            slot.1 = ab_tree;
        }
    }

    fn fill_assetbundle(&self) -> Value {
        let mut ab = self.base_clone("AssetBundle");
        let lower = self.bundle_name.to_ascii_lowercase();
        ab.insert("m_Name", lower.clone());
        ab.insert("m_AssetBundleName", lower);

        let mut entries: Vec<sbp_order::Entry> = Vec::new();

        let mut glb_objs: Vec<sbp_order::Obj> = vec![sbp_order::Obj::new(0, self.root_go_pid)];
        let mut seen: HashSet<(i64, i64)> = HashSet::new();
        seen.insert((0, self.root_go_pid));
        let glb_set: Vec<i64> = self
            .scene_object_pids
            .iter()
            .chain(self.glb_referenced_mats.iter())
            .copied()
            .filter(|p| *p != self.root_go_pid)
            .collect();
        for p in glb_set {
            if seen.insert((0, p)) {
                glb_objs.push(sbp_order::Obj::new(0, p));
            }
        }

        let pos_default = self
            .externals_position
            .unwrap_or_else(|| ExternalsPosition::for_target(self.target));
        let cb_default = self
            .cross_bundle_position
            .unwrap_or_else(|| CrossBundlePosition::for_target(self.target));
        let mut entry_pos: Vec<(ExternalsPosition, CrossBundlePosition)> = Vec::new();

        let glb_ext = if self.is_gltf { "gltf" } else { "glb" };
        let glb_key = if self.lod.is_some() {
            format!("{}.prefab", self.root_hash)
        } else {
            format!("{}.{}", self.root_hash, glb_ext)
        };
        entries.push(sbp_order::Entry {
            guid: self.glb_guid.clone(),
            key: glb_key,
            objects: glb_objs,
            asset: Some(sbp_order::Obj::new(0, self.root_go_pid)),
        });
        entry_pos.push((pos_default, cb_default));

        if emits_metadata_textasset(&self.root_hash, self.toggles.v38_compat) {
            entries.push(sbp_order::Entry {
                guid: pathids::asset_guid(&format!("{}/metadata", self.root_hash)),
                key: "metadata.json".into(),
                objects: vec![sbp_order::Obj::new(0, self.meta_pid)],
                asset: Some(sbp_order::Obj::new(0, self.meta_pid)),
            });
            entry_pos.push((pos_default, cb_default));
        }

        for (mi, (key, mat_pid, tex_pids)) in self.material_entries.iter().enumerate() {
            let mut objs: Vec<sbp_order::Obj> = vec![sbp_order::Obj::new(0, *mat_pid)];
            let mut seen: HashSet<(i64, i64)> = HashSet::new();
            seen.insert((0, *mat_pid));
            let mut deps: Vec<sbp_order::Obj> = vec![sbp_order::Obj::new(
                SHADER_FILE_ID,
                self.shader_dep_path_id(),
            )];
            deps.extend(tex_pids.iter().map(|p| sbp_order::Obj::new(0, *p)));
            if let Some(ext) = self.mat_external_pptrs.get(mat_pid) {
                deps.extend(ext.iter().map(|&(f, p)| sbp_order::Obj::new(f, p)));
            }
            for d in deps {
                if seen.insert((d.file_id, d.path_id)) {
                    objs.push(d);
                }
            }
            let stem = key.strip_suffix(".mat").unwrap_or(key);
            entries.push(sbp_order::Entry {
                guid: pathids::asset_guid(&format!("{}/material/{}", self.root_hash, stem)),
                key: key.clone(),
                objects: objs,
                asset: Some(sbp_order::Obj::new(0, *mat_pid)),
            });
            let p = self
                .material_externals_overrides
                .as_ref()
                .and_then(|v| v.get(mi).copied())
                .unwrap_or((pos_default, cb_default));
            entry_pos.push(p);
        }

        for (key, tex_pid) in &self.texture_entries {
            let stem = key.strip_suffix(".png").unwrap_or(key);
            entries.push(sbp_order::Entry {
                guid: pathids::asset_guid(&format!("{}/texture/{}", self.root_hash, stem)),
                key: key.clone(),
                objects: vec![sbp_order::Obj::new(0, *tex_pid)],
                asset: Some(sbp_order::Obj::new(0, *tex_pid)),
            });
            entry_pos.push((pos_default, cb_default));
        }

        for (key, mesh_pid) in &self.lod_mesh_entries {
            let stem = key.strip_suffix(".asset").unwrap_or(key);
            entries.push(sbp_order::Entry {
                guid: pathids::asset_guid(&format!("{}/mesh/{}", self.root_hash, stem)),
                key: key.clone(),
                objects: vec![sbp_order::Obj::new(0, *mesh_pid)],
                asset: Some(sbp_order::Obj::new(0, *mesh_pid)),
            });
            entry_pos.push((pos_default, cb_default));
        }

        if let Some((ctrl_pid, clip_pids)) = &self.animator_controller_entry {
            let ctrl_pid = *ctrl_pid;

            let mut objs: Vec<sbp_order::Obj> = clip_pids
                .iter()
                .map(|p| sbp_order::Obj::new(0, *p))
                .collect();
            objs.push(sbp_order::Obj::new(0, ctrl_pid));
            entries.push(sbp_order::Entry {
                guid: pathids::asset_guid(&format!("{}/animatorController", self.root_hash)),
                key: "animatorController.controller".into(),
                objects: objs,
                asset: Some(sbp_order::Obj::new(0, ctrl_pid)),
            });
            entry_pos.push((pos_default, cb_default));
        }

        let pos_by_guid: HashMap<String, (ExternalsPosition, CrossBundlePosition)> = entries
            .iter()
            .zip(entry_pos.iter())
            .map(|(e, p)| (e.guid.clone(), *p))
            .collect();
        let shader_cab = self.shader_cab_name().to_lowercase();
        let ext_cabs: Vec<String> = self
            .ext_bundle_files
            .iter()
            .map(|bf| cabname::cab_name(bf).to_lowercase())
            .collect();
        let use_cab_merge = matches!(self.target, "windows" | "mac" | "linux" | "webgl")
            && self.externals_position.is_none()
            && self.cross_bundle_position.is_none()
            && self.material_externals_overrides.is_none();

        let own_cab = cabname::cab_name(&self.bundle_name).to_lowercase();
        let cab_for = |fid: i64| -> String {
            if fid == 1 {
                shader_cab.clone()
            } else if fid >= 2 {
                ext_cabs
                    .get((fid - 2) as usize)
                    .cloned()
                    .unwrap_or_default()
            } else {
                own_cab.clone()
            }
        };
        let (preload, container) = if use_cab_merge {
            let mut order_idx: Vec<usize> = (0..entries.len()).collect();
            order_idx.sort_by_key(|&i| sbp_order::guid_sort_key(&entries[i].guid));
            let mut pl: Vec<sbp_order::Obj> = Vec::new();
            let mut by_key: Vec<(String, sbp_order::ContainerSlot)> = Vec::new();
            for &orig_idx in &order_idx {
                let e = &entries[orig_idx];
                let run = sbp_order::order_run_cab_merge(&e.objects, cab_for);
                let start = pl.len();
                let size = run.len();
                let asset = e
                    .asset
                    .or_else(|| run.first().copied())
                    .unwrap_or(sbp_order::Obj {
                        file_id: 0,
                        path_id: 0,
                    });
                pl.extend(run);
                let slot = sbp_order::ContainerSlot {
                    preload_index: start,
                    preload_size: size,
                    asset,
                };
                match by_key.iter_mut().find(|(k, _)| *k == e.key) {
                    Some((_, existing)) => *existing = slot,
                    None => by_key.push((e.key.clone(), slot)),
                }
            }
            by_key.sort_by(|a, b| a.0.cmp(&b.0));
            (pl, by_key)
        } else {
            sbp_order::build_preload_and_container_per_entry(&entries, |e| {
                pos_by_guid
                    .get(&e.guid)
                    .copied()
                    .unwrap_or((pos_default, cb_default))
            })
        };
        let (preload_v, container_v) = sbp_order::to_values(&preload, &container);
        ab.insert("m_PreloadTable", preload_v);
        ab.insert("m_Container", container_v);
        ab.insert("m_MainAsset", sbp_order::empty_main_asset());

        let mut dep_set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

        if !self.material_entries.is_empty() {
            dep_set.insert(self.shader_cab_name().to_lowercase());
        }
        for bf in &self.ext_bundle_files {
            dep_set.insert(cabname::cab_name(bf).to_lowercase());
        }
        for bf in &self.metadata_dependencies {
            dep_set.insert(cabname::cab_name(bf).to_lowercase());
        }
        ab.insert(
            "m_Dependencies",
            Value::Array(dep_set.into_iter().map(Value::Str).collect()),
        );
        ab
    }

    pub(super) fn commit(&self, bundle: &mut Bundle) -> Result<()> {
        let mut blobs: Vec<ress::TextureBlob> = Vec::new();
        for (&pid, (tn, tree)) in self.objects.iter() {
            if tn != "Texture2D" {
                continue;
            }

            if matches!(tree.get("m_IsReadable"), Some(Value::Bool(true))) {
                continue;
            }
            if let Some(Value::Bytes(b)) = tree.get("image data") {
                if !b.is_empty() {
                    blobs.push(ress::TextureBlob::new(pid, b.clone(), ""));
                }
            }
        }

        let shader_cab = self.shader_cab_name();
        let externals = if self.material_entries.is_empty() && self.ext_bundle_files.is_empty() {
            ExternalsPolicy::Clear
        } else {
            ExternalsPolicy::ShaderRef {
                ext_bundle_files: &self.ext_bundle_files,
                shader_cab,
            }
        };
        commit_objects(
            bundle,
            &self.bundle_name,
            self.target,
            self.proto,
            &self.objects,
            &blobs,
            &self.force_inline_tex,
            externals,
        )
    }
}
