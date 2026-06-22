
use crate::unity::bundle_file::{Bundle, FileContent};
use crate::unity::serialized_file::{class_name, Object, SerializedFile};
use crate::value::Value;
use std::collections::{HashMap, HashSet};

const C_GAMEOBJECT: i32 = 1;
const C_TRANSFORM: i32 = 4;
const C_MATERIAL: i32 = 21;
const C_MESHRENDERER: i32 = 23;
const C_TEXTURE2D: i32 = 28;
const C_MESHFILTER: i32 = 33;
const C_MESH: i32 = 43;
const C_TEXTASSET: i32 = 49;
const C_SKINNEDMESHRENDERER: i32 = 137;
const C_ASSETBUNDLE: i32 = 142;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Severity {
    Error,
    Warn,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "ERR ",
            Severity::Warn => "WARN",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Finding {
    pub severity: Severity,

    pub code: &'static str,

    pub bundle: String,
    pub msg: String,
}

#[derive(Default)]
pub struct ValidateCtx {

    pub global_cabs: Option<HashSet<String>>,
}

impl ValidateCtx {
    pub fn single_file() -> Self {
        ValidateCtx { global_cabs: None }
    }
    pub fn with_global_cabs(cabs: HashSet<String>) -> Self {
        ValidateCtx {
            global_cabs: Some(cabs),
        }
    }
}

fn read_pptr(v: Option<&Value>) -> Option<(i64, i64)> {
    let m = v?.as_map()?;
    let fid = m.get("m_FileID").and_then(|x| x.as_i64())?;
    let pid = m.get("m_PathID").and_then(|x| x.as_i64())?;
    Some((fid, pid))
}

struct Bundles<'a> {
    label: String,
    sf: &'a SerializedFile,

    raw_files: HashSet<String>,

    self_cab: Option<String>,
}

pub fn validate_bundle(data: &[u8], label: &str, ctx: &ValidateCtx) -> Vec<Finding> {
    let mut out = Vec::new();
    let bundle = match Bundle::load_bytes(data) {
        Ok(b) => b,
        Err(e) => {
            out.push(Finding {
                severity: Severity::Error,
                code: "E0",
                bundle: label.to_string(),
                msg: format!("failed to parse bundle: {e:#}"),
            });
            return out;
        }
    };

    let mut sf: Option<&SerializedFile> = None;
    let mut self_cab: Option<String> = None;
    let mut raw_files: HashSet<String> = HashSet::new();
    for f in &bundle.files {
        match &f.content {
            FileContent::Serialized(s) => {
                if sf.is_none() {
                    sf = Some(s);
                    self_cab = Some(f.name.clone());
                }
            }
            FileContent::Raw(_) => {
                raw_files.insert(f.name.clone());
            }
        }
    }
    let Some(sf) = sf else {
        out.push(Finding {
            severity: Severity::Error,
            code: "E0",
            bundle: label.to_string(),
            msg: "bundle has no serialized file".into(),
        });
        return out;
    };

    let b = Bundles {
        label: label.to_string(),
        sf,
        raw_files,
        self_cab,
    };
    check(&b, ctx, &mut out);
    out
}

fn check(b: &Bundles, ctx: &ValidateCtx, out: &mut Vec<Finding>) {
    let sf = b.sf;

    let mut pid_class: HashMap<i64, i32> = HashMap::with_capacity(sf.objects.len());
    for o in &sf.objects {
        pid_class.insert(o.path_id, o.class_id);
    }
    let internal_exists = |pid: i64| pid_class.contains_key(&pid);

    let ext_names: Vec<String> = sf
        .externals
        .iter()
        .map(|e| {

            e.path
                .rsplit('/')
                .next()
                .unwrap_or(&e.path)
                .to_lowercase()
        })
        .collect();

    let ab_objs: Vec<&Object> = sf
        .objects
        .iter()
        .filter(|o| o.class_id == C_ASSETBUNDLE)
        .collect();

    let mut ab_deps: HashSet<String> = HashSet::new();
    let mut preload_len: usize = 0;
    match ab_objs.len() {
        1 => {
            let ab = ab_objs[0];
            if let Ok(v) = sf.read_typetree(ab) {
                if let Some(m) = v.as_map() {

                    if let Some(name) = m.get("m_AssetBundleName").and_then(|x| x.as_str()) {
                        if name.is_empty() {
                            out.push(finding(
                                Severity::Error,
                                "E8",
                                b,
                                "AssetBundle object has empty m_AssetBundleName".into(),
                            ));
                        }
                    } else {
                        out.push(finding(
                            Severity::Warn,
                            "E8",
                            b,
                            "AssetBundle object missing m_AssetBundleName".into(),
                        ));
                    }

                    if let Some(deps) = m.get("m_Dependencies").and_then(|x| x.as_array()) {
                        for d in deps {
                            if let Some(s) = d.as_str() {
                                ab_deps.insert(s.to_lowercase());
                            }
                        }
                    }

                    if let Some(pre) = m.get("m_PreloadTable").and_then(|x| x.as_array()) {
                        preload_len = pre.len();

                        check_pptr_list(
                            b,
                            "E1",
                            "m_PreloadTable entry",
                            pre.iter().filter_map(|pv| read_pptr(Some(pv))),
                            &internal_exists,
                            &ext_names,
                            &ab_deps,
                            ctx,
                            out,
                        );
                    }

                    if let Some(cont) = m.get("m_Container").and_then(|x| x.as_array()) {
                        for (i, e) in cont.iter().enumerate() {
                            let Some(pair) = e.as_array() else { continue };
                            let Some(slot) = pair.get(1).and_then(|x| x.as_map()) else {
                                continue;
                            };

                            if let Some((fid, pid)) = read_pptr(slot.get("asset")) {
                                resolve_one(
                                    b,
                                    "E1",
                                    &format!("m_Container[{i}].asset"),
                                    fid,
                                    pid,
                                    &internal_exists,
                                    &ext_names,
                                    &ab_deps,
                                    ctx,
                                    out,
                                );
                            }

                            let pidx = slot
                                .get("preloadIndex")
                                .and_then(|x| x.as_i64())
                                .unwrap_or(0);
                            let psz = slot
                                .get("preloadSize")
                                .and_then(|x| x.as_i64())
                                .unwrap_or(0);
                            if pidx < 0 || psz < 0 || (pidx + psz) as usize > preload_len {
                                out.push(finding(
                                    Severity::Error,
                                    "E7",
                                    b,
                                    format!(
                                        "m_Container[{i}] preload span [{pidx}..{}+{psz}] exceeds m_PreloadTable len {preload_len}",
                                        pidx
                                    ),
                                ));
                            }
                        }
                    }
                }
            }
        }
        0 => out.push(finding(
            Severity::Error,
            "E8",
            b,
            "no AssetBundle (class 142) object present".into(),
        )),
        n => out.push(finding(
            Severity::Error,
            "E8",
            b,
            format!("expected exactly one AssetBundle object, found {n}"),
        )),
    }

    for (i, name) in ext_names.iter().enumerate() {
        if name.is_empty() {
            continue;
        }
        if !ab_deps.contains(name) {
            out.push(finding(
                Severity::Error,
                "E3",
                b,
                format!("externals[{i}] '{name}' not listed in AssetBundle m_Dependencies"),
            ));
        }
        match &ctx.global_cabs {
            Some(cabs) if cabs.contains(name) => {}
            Some(_) => out.push(finding(
                Severity::Warn,
                "E3",
                b,
                format!(
                    "externals[{i}] '{name}' not found in the output index (may be a shared/external dependency)"
                ),
            )),
            None => out.push(finding(
                Severity::Warn,
                "E3",
                b,
                format!("externals[{i}] '{name}' not verifiable in single-file mode"),
            )),
        }
    }

    let mut texture_streams = false;
    let mut have_textasset = false;
    let mut have_glb_mesh = false;
    for o in &sf.objects {
        match o.class_id {
            C_MATERIAL => check_material(b, o, &pid_class, &ext_names, &ab_deps, ctx, out),
            C_MESHRENDERER | C_SKINNEDMESHRENDERER => {
                check_renderer(b, o, &internal_exists, &ext_names, &ab_deps, ctx, out)
            }
            C_MESHFILTER => check_meshfilter(b, o, &internal_exists, &ext_names, &ab_deps, ctx, out),
            C_GAMEOBJECT => check_gameobject(b, o, &internal_exists, out),
            C_TRANSFORM => check_transform(b, o, &internal_exists, out),
            C_MESH => {
                have_glb_mesh = true;
                check_mesh(b, o, out);
            }
            C_TEXTURE2D if check_texture_streams(b, o, out) => {
                texture_streams = true;
            }
            C_TEXTASSET => have_textasset = true,
            _ => {}
        }
    }

    if let Some(cab) = &b.self_cab {
        if !is_cab_name(cab) {
            out.push(finding(
                Severity::Warn,
                "E9",
                b,
                format!("serialized file name '{cab}' is not a well-formed CAB-<hex> name"),
            ));
        }
    }
    if texture_streams && b.raw_files.is_empty() {
        out.push(finding(
            Severity::Error,
            "E9",
            b,
            "Texture2D streams into a .resS but no raw sibling file is present".into(),
        ));
    }

    if have_glb_mesh && !have_textasset {
        out.push(finding(
            Severity::Warn,
            "W1",
            b,
            "glb bundle (has Mesh) but no TextAsset (metadata) object present".into(),
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn check_material(
    b: &Bundles,
    o: &Object,
    pid_class: &HashMap<i64, i32>,
    ext_names: &[String],
    ab_deps: &HashSet<String>,
    ctx: &ValidateCtx,
    out: &mut Vec<Finding>,
) {
    let Ok(v) = b.sf.read_typetree(o) else { return };
    let Some(envs) = v
        .get("m_SavedProperties")
        .and_then(|s| s.get("m_TexEnvs"))
        .and_then(|x| x.as_array())
    else {
        return;
    };
    for e in envs {
        let Some(pair) = e.as_array() else { continue };
        let slot = pair.first().and_then(|x| x.as_str()).unwrap_or("?");
        let Some((fid, pid)) = read_pptr(pair.get(1).and_then(|p| p.get("m_Texture"))) else {
            continue;
        };
        if pid == 0 {
            continue;
        }
        if fid == 0 {

            match pid_class.get(&pid) {
                Some(&C_TEXTURE2D) => {}
                Some(&other) => out.push(finding(
                    Severity::Error,
                    "E2",
                    b,
                    format!(
                        "Material pid={} slot '{slot}' binds pid={pid} which is a {} (expected Texture2D)",
                        o.path_id,
                        class_name(other)
                    ),
                )),
                None => out.push(finding(
                    Severity::Error,
                    "E2",
                    b,
                    format!(
                        "Material pid={} slot '{slot}' binds internal pid={pid} which is not present",
                        o.path_id
                    ),
                )),
            }
        } else {

            let _ = ctx;
            resolve_external(
                b,
                "E2",
                &format!("Material pid={} slot '{slot}'", o.path_id),
                fid,
                ext_names,
                ab_deps,
                out,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn check_renderer(
    b: &Bundles,
    o: &Object,
    internal_exists: &impl Fn(i64) -> bool,
    ext_names: &[String],
    ab_deps: &HashSet<String>,
    ctx: &ValidateCtx,
    out: &mut Vec<Finding>,
) {
    let Ok(v) = b.sf.read_typetree(o) else { return };

    if let Some((fid, pid)) = read_pptr(v.get("m_Mesh")) {
        if pid != 0 {
            resolve_one(
                b,
                "E4",
                &format!("{} pid={} m_Mesh", class_name(o.class_id), o.path_id),
                fid,
                pid,
                internal_exists,
                ext_names,
                ab_deps,
                ctx,
                out,
            );
        }
    }
    if let Some(mats) = v.get("m_Materials").and_then(|x| x.as_array()) {
        for (i, mv) in mats.iter().enumerate() {
            let Some((fid, pid)) = read_pptr(Some(mv)) else {
                continue;
            };
            if pid == 0 {
                continue;
            }
            resolve_one(
                b,
                "E4",
                &format!(
                    "{} pid={} m_Materials[{i}]",
                    class_name(o.class_id),
                    o.path_id
                ),
                fid,
                pid,
                internal_exists,
                ext_names,
                ab_deps,
                ctx,
                out,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn check_meshfilter(
    b: &Bundles,
    o: &Object,
    internal_exists: &impl Fn(i64) -> bool,
    ext_names: &[String],
    ab_deps: &HashSet<String>,
    ctx: &ValidateCtx,
    out: &mut Vec<Finding>,
) {
    let Ok(v) = b.sf.read_typetree(o) else { return };
    if let Some((fid, pid)) = read_pptr(v.get("m_Mesh")) {
        if pid != 0 {
            resolve_one(
                b,
                "E4",
                &format!("MeshFilter pid={} m_Mesh", o.path_id),
                fid,
                pid,
                internal_exists,
                ext_names,
                ab_deps,
                ctx,
                out,
            );
        }
    }
}

fn check_gameobject(
    b: &Bundles,
    o: &Object,
    internal_exists: &impl Fn(i64) -> bool,
    out: &mut Vec<Finding>,
) {
    let Ok(v) = b.sf.read_typetree(o) else { return };
    let Some(comps) = v.get("m_Component").and_then(|x| x.as_array()) else {
        return;
    };
    for (i, c) in comps.iter().enumerate() {

        let pptr = c.get("component").or_else(|| {
            c.as_array()
                .and_then(|a| a.get(1).or_else(|| a.first()))
        });
        let Some((fid, pid)) = read_pptr(pptr) else {
            continue;
        };

        if fid == 0 && pid != 0 && !internal_exists(pid) {
            out.push(finding(
                Severity::Error,
                "E5",
                b,
                format!(
                    "GameObject pid={} m_Component[{i}] points at missing pid={pid}",
                    o.path_id
                ),
            ));
        }
    }
}

fn check_transform(
    b: &Bundles,
    o: &Object,
    internal_exists: &impl Fn(i64) -> bool,
    out: &mut Vec<Finding>,
) {
    let Ok(v) = b.sf.read_typetree(o) else { return };
    if let Some((fid, pid)) = read_pptr(v.get("m_GameObject")) {
        if fid == 0 && pid != 0 && !internal_exists(pid) {
            out.push(finding(
                Severity::Error,
                "E5",
                b,
                format!(
                    "Transform pid={} m_GameObject points at missing pid={pid}",
                    o.path_id
                ),
            ));
        }
    }
}

fn check_mesh(b: &Bundles, o: &Object, out: &mut Vec<Finding>) {
    let Ok(v) = b.sf.read_typetree(o) else { return };
    let Some(m) = v.as_map() else { return };
    let name = m.get("m_Name").and_then(|x| x.as_str()).unwrap_or("");

    let total_verts = m
        .get("m_VertexData")
        .and_then(|x| x.get("m_VertexCount"))
        .and_then(|x| x.as_i64())
        .unwrap_or(0);

    let idx_bytes = match m.get("m_IndexBuffer") {
        Some(Value::Bytes(d)) => d.len() as i64,
        _ => 0,
    };

    let idx_width = match m.get("m_IndexFormat").and_then(|x| x.as_i64()).unwrap_or(0) {
        1 => 4,
        _ => 2,
    };

    let Some(subs) = m.get("m_SubMeshes").and_then(|x| x.as_array()) else {
        return;
    };
    for (i, s) in subs.iter().enumerate() {
        let g = |k: &str| s.get(k).and_then(|x| x.as_i64()).unwrap_or(0);
        let first_vertex = g("firstVertex");
        let vertex_count = g("vertexCount");
        let first_byte = g("firstByte");
        let index_count = g("indexCount");

        if total_verts > 0 && first_vertex + vertex_count > total_verts {
            out.push(finding(
                Severity::Error,
                "E6",
                b,
                format!(
                    "Mesh '{name}' submesh[{i}] vertex range [{first_vertex}+{vertex_count}] exceeds vertex count {total_verts}"
                ),
            ));
        }

        if idx_bytes > 0 && first_byte + index_count * idx_width > idx_bytes {
            out.push(finding(
                Severity::Error,
                "E6",
                b,
                format!(
                    "Mesh '{name}' submesh[{i}] index range [{first_byte}+{index_count}*{idx_width}B] exceeds index buffer {idx_bytes}B"
                ),
            ));
        }
    }
}

fn check_texture_streams(b: &Bundles, o: &Object, out: &mut Vec<Finding>) -> bool {
    let Ok(v) = b.sf.read_typetree(o) else {
        return false;
    };
    let Some(sd) = v.get("m_StreamData").and_then(|x| x.as_map()) else {
        return false;
    };
    let size = sd.get("size").and_then(|x| x.as_i64()).unwrap_or(0);
    let path = sd.get("path").and_then(|x| x.as_str()).unwrap_or("");
    if size > 0 && !path.is_empty() {

        let want = path.rsplit('/').next().unwrap_or(path);
        if !b.raw_files.contains(want) {
            out.push(finding(
                Severity::Error,
                "E9",
                b,
                format!(
                    "Texture2D pid={} streams into '{want}' which is not present in the bundle",
                    o.path_id
                ),
            ));
        }
        return true;
    }
    false
}

#[allow(clippy::too_many_arguments)]
fn check_pptr_list<I: Iterator<Item = (i64, i64)>>(
    b: &Bundles,
    code: &'static str,
    what: &str,
    it: I,
    internal_exists: &impl Fn(i64) -> bool,
    ext_names: &[String],
    ab_deps: &HashSet<String>,
    ctx: &ValidateCtx,
    out: &mut Vec<Finding>,
) {
    for (idx, (fid, pid)) in it.enumerate() {
        resolve_one(
            b,
            code,
            &format!("{what}[{idx}]"),
            fid,
            pid,
            internal_exists,
            ext_names,
            ab_deps,
            ctx,
            out,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn resolve_one(
    b: &Bundles,
    code: &'static str,
    what: &str,
    fid: i64,
    pid: i64,
    internal_exists: &impl Fn(i64) -> bool,
    ext_names: &[String],
    ab_deps: &HashSet<String>,
    ctx: &ValidateCtx,
    out: &mut Vec<Finding>,
) {
    if fid == 0 {
        if pid != 0 && !internal_exists(pid) {
            out.push(finding(
                Severity::Error,
                code,
                b,
                format!("{what}: internal PPtr pid={pid} does not resolve to any object"),
            ));
        }
    } else {
        let _ = ctx;
        resolve_external(b, code, what, fid, ext_names, ab_deps, out);
    }
}

fn resolve_external(
    b: &Bundles,
    _code: &'static str,
    what: &str,
    fid: i64,
    ext_names: &[String],
    ab_deps: &HashSet<String>,
    out: &mut Vec<Finding>,
) {

    let idx = (fid - 1) as usize;
    let Some(name) = ext_names.get(idx) else {
        out.push(finding(
            Severity::Error,
            "E3",
            b,
            format!("{what}: external fileID={fid} out of range (externals len {})", ext_names.len()),
        ));
        return;
    };

    if !name.is_empty() && !ab_deps.contains(name) {
        out.push(finding(
            Severity::Error,
            "E3",
            b,
            format!("{what}: external '{name}' (fileID={fid}) is not in m_Dependencies"),
        ));
    }
}

fn finding(severity: Severity, code: &'static str, b: &Bundles, msg: String) -> Finding {
    Finding {
        severity,
        code,
        bundle: b.label.clone(),
        msg,
    }
}

fn is_cab_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let Some(rest) = lower.strip_prefix("cab-") else {
        return false;
    };
    rest.len() == 32 && rest.bytes().all(|c| c.is_ascii_hexdigit())
}

pub fn bundle_cab_names(data: &[u8]) -> Vec<String> {
    let Ok(bundle) = Bundle::load_bytes(data) else {
        return Vec::new();
    };
    bundle
        .files
        .iter()
        .filter(|f| matches!(f.content, FileContent::Serialized(_)))
        .map(|f| f.name.to_lowercase())
        .collect()
}
