use abgen::unity::bundle_file::{Bundle, FileContent};
use abgen::Result;
use rayon::prelude::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const C_GO: i32 = 1;
const C_TRANSFORM: i32 = 4;
const C_MATERIAL: i32 = 21;
const C_MESHRENDERER: i32 = 23;
const C_TEXTURE2D: i32 = 28;
const C_MESH: i32 = 43;
const C_TEXTASSET: i32 = 49;
const C_MESHCOLLIDER: i32 = 64;
const C_ANIMATIONCLIP: i32 = 74;
const C_ANIMATORCONTROLLER: i32 = 91;
const C_ANIMATOR: i32 = 95;
const C_ANIMATION: i32 = 111;
const C_SKINNEDMESHRENDERER: i32 = 137;
const C_ASSETBUNDLE: i32 = 142;

fn usage() -> ! {
    eprintln!(
        "usage: abgen-verify <ours-dir> <reference-dir> [-j JOBS] [--json PATH] [--tolerant]\n\
         \n\
         walks <reference-dir>/<entity>/<bundle-name> looking for a matching\n\
         <ours-dir>/<entity>/<bundle-name>; diffs bytes and classifies each\n\
         bundle by Unity-class set (standalone-texture / standalone-texture-legacy\n\
         / glb-scene / glb-emote / glb-wearable / glb-animated / glb-with-morph /\n\
         glb-scene-collider / glb-scene-empty / bundle-empty / other). Prints\n\
         per-kind ppm-bits and size-delta histogram. Writes machine-readable\n\
         JSON if --json given.\n\
         \n\
         The reference side may also be an ab-cdn-reference tree —\n\
         <ref>/[<prefix>/]<entity>/[<platform>/]<bundle-name> is matched\n\
         against <ours-dir>/<entity>/<bundle-name>.\n\
         \n\
         --tolerant (D6 validation loop): additionally parse every\n\
         non-byte-identical pair and separate KNOWN-BENIGN v38/live-mode\n\
         structural deltas from real diffs. Tolerance classes:\n\
           unref-fmt5-texture   absent unreferenced in-GLB fmt=5 (ARGB32)\n\
                                'original' duplicate textures\n\
           metadata-textasset   metadata TextAsset presence, or content\n\
                                differing only in timestamp/version (+\n\
                                lowercased/deduped deps, dcl/scene_ignore_*)\n\
           container-path-case  container paths equal after lowercase+dedup\n\
           unref-material       extra Material bound to 0 renderers\n\
                                (e.g. upstream DCL_Scene)\n\
         Pair categories: identical | payload-drift (bytes differ, structure\n\
         matches) | tolerated (only benign deltas) | structural (REAL diffs).\n\
         Exit code 3 when any structural pair is found (0 otherwise), so the\n\
         loop can gate on it."
    );
    std::process::exit(2);
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut positional: Vec<String> = Vec::new();
    let mut jobs: usize = num_cpus::get();
    let mut json_out: Option<String> = None;
    let mut tolerant = false;
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "-j" | "--jobs" => {
                i += 1;
                jobs = argv
                    .get(i)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_else(|| usage());
            }
            "--json" => {
                i += 1;
                json_out = Some(argv.get(i).cloned().unwrap_or_else(|| usage()));
            }
            "--tolerant" => {
                tolerant = true;
            }
            "-h" | "--help" => usage(),
            other if other.starts_with("--") => {
                eprintln!("unknown option: {other}");
                usage();
            }
            other => positional.push(other.to_string()),
        }
        i += 1;
    }
    if positional.len() != 2 {
        usage();
    }
    let ours_root = PathBuf::from(&positional[0]);
    let ref_root = PathBuf::from(&positional[1]);

    rayon::ThreadPoolBuilder::new()
        .num_threads(jobs)
        .build_global()
        .ok();

    let pairs: Vec<(PathBuf, PathBuf, String, String)> = enumerate_pairs(&ref_root, &ours_root);
    let n = pairs.len();
    eprintln!("found {n} reference bundles, comparing");

    let per_bundle: Mutex<Vec<BundleStat>> = Mutex::new(Vec::with_capacity(n));
    pairs
        .par_iter()
        .for_each(|(ref_path, ours_path, ent, name)| {
            match stat_pair(ref_path, ours_path, tolerant) {
                Ok(s) => per_bundle.lock().unwrap().push(BundleStat {
                    entity: ent.clone(),
                    bundle: name.clone(),
                    ..s
                }),
                Err(e) => eprintln!("err {ent}/{name}: {e}"),
            }
        });

    let mut stats = per_bundle.into_inner().unwrap();
    stats.sort_by(|a, b| (&a.entity, &a.bundle).cmp(&(&b.entity, &b.bundle)));
    let n_structural = print_and_serialize(&stats, json_out.as_deref(), tolerant)?;
    if tolerant && n_structural > 0 {
        std::process::exit(3);
    }
    Ok(())
}

#[derive(Default, Clone)]
struct BundleStat {
    entity: String,
    bundle: String,
    kind: &'static str,
    ours_bytes: usize,
    ref_bytes: usize,
    bits_diff: u64,
    byte_identical: bool,
    category: &'static str,
    tolerated: Vec<&'static str>,
    structural: Vec<String>,
}

const PLATFORM_DIRS: [&str; 4] = ["windows", "mac", "webgl", "linux"];

fn enumerate_pairs(ref_root: &Path, ours_root: &Path) -> Vec<(PathBuf, PathBuf, String, String)> {
    let mut out = Vec::new();
    walk_ref(ref_root, ours_root, &mut Vec::new(), &mut out);
    out
}

fn walk_ref(
    dir: &Path,
    ours_root: &Path,
    comps: &mut Vec<String>,
    out: &mut Vec<(PathBuf, PathBuf, String, String)>,
) {
    if comps.len() > 3 {
        return;
    }
    let Ok(iter) = std::fs::read_dir(dir) else {
        return;
    };
    for e in iter.flatten() {
        let ft = match e.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        let name = e.file_name().to_string_lossy().into_owned();
        if ft.is_dir() {
            comps.push(name);
            walk_ref(&e.path(), ours_root, comps, out);
            comps.pop();
            continue;
        }
        if !ft.is_file() {
            continue;
        }
        let entity = comps
            .iter()
            .rev()
            .find(|c| !PLATFORM_DIRS.contains(&c.as_str()))
            .cloned()
            .unwrap_or_default();
        let ours = if entity.is_empty() {
            ours_root.join(&name)
        } else {
            ours_root.join(&entity).join(&name)
        };
        if ours.is_file() {
            out.push((e.path(), ours, entity, name));
        }
    }
}

fn stat_pair(ref_path: &Path, ours_path: &Path, tolerant: bool) -> Result<BundleStat> {
    let r_bytes = std::fs::read(ref_path)?;
    let o_bytes = std::fs::read(ours_path)?;
    let kind = classify(&r_bytes).unwrap_or("other");
    let bits = bits_diff(&o_bytes, &r_bytes);
    let byte_identical = o_bytes == r_bytes;
    let (category, tolerated, structural) = if !tolerant {
        ("", Vec::new(), Vec::new())
    } else if byte_identical {
        ("identical", Vec::new(), Vec::new())
    } else {
        let delta = match (collect_inv(&r_bytes), collect_inv(&o_bytes)) {
            (Some(r), Some(o)) => analyze(&r, &o),
            (r, o) => {
                let mut d = Delta::default();
                if r.is_none() {
                    d.structural.push("bundle-parse-failed(ref)".to_string());
                }
                if o.is_none() {
                    d.structural.push("bundle-parse-failed(ours)".to_string());
                }
                d
            }
        };
        let cat = if !delta.structural.is_empty() {
            "structural"
        } else if !delta.tolerated.is_empty() {
            "tolerated"
        } else {
            "payload-drift"
        };
        (cat, delta.tolerated, delta.structural)
    };
    Ok(BundleStat {
        entity: String::new(),
        bundle: String::new(),
        kind,
        ours_bytes: o_bytes.len(),
        ref_bytes: r_bytes.len(),
        bits_diff: bits,
        byte_identical,
        category,
        tolerated,
        structural,
    })
}

fn bits_diff(a: &[u8], b: &[u8]) -> u64 {
    let common = a.len().min(b.len());
    let mut diff: u64 = a[..common]
        .iter()
        .zip(b[..common].iter())
        .map(|(x, y)| (x ^ y).count_ones() as u64)
        .sum();
    diff += (a.len().abs_diff(b.len()) as u64) * 8;
    diff
}

const TOL_UNREF_FMT5: &str = "unref-fmt5-texture";
const TOL_METADATA: &str = "metadata-textasset";
const TOL_CONTAINER_CASE: &str = "container-path-case";
const TOL_UNREF_MATERIAL: &str = "unref-material";

const TEX_FMT_ARGB32: i64 = 5;

type ObjKey = (usize, i64);

#[derive(Default)]
struct SideInv {
    textures: Vec<(String, i64, ObjKey)>,
    referenced_tex: HashSet<ObjKey>,
    materials: Vec<(String, ObjKey)>,
    bound_mats: HashSet<ObjKey>,
    textassets: Vec<(String, String, ObjKey)>,
    container: Vec<(String, Option<ObjKey>)>,
    other_classes: BTreeMap<i32, usize>,
}

fn collect_inv(bytes: &[u8]) -> Option<SideInv> {
    let bundle = Bundle::load_bytes(bytes).ok()?;
    let mut inv = SideInv::default();
    let mut fi = 0usize;
    for f in &bundle.files {
        let FileContent::Serialized(sf) = &f.content else {
            continue;
        };
        for obj in &sf.objects {
            let key: ObjKey = (fi, obj.path_id);
            match obj.class_id {
                C_TEXTURE2D => {
                    let Ok(v) = sf.read_typetree(obj) else {
                        continue;
                    };
                    let name = v
                        .get("m_Name")
                        .and_then(|x| x.as_str())
                        .unwrap_or("?")
                        .to_string();
                    let fmt = v
                        .get("m_TextureFormat")
                        .and_then(|x| x.as_i64())
                        .unwrap_or(-1);
                    inv.textures.push((name, fmt, key));
                }
                C_MATERIAL => {
                    let Ok(v) = sf.read_typetree(obj) else {
                        continue;
                    };
                    let name = v
                        .get("m_Name")
                        .and_then(|x| x.as_str())
                        .unwrap_or("?")
                        .to_string();
                    inv.materials.push((name, key));
                    if let Some(envs) = v
                        .get("m_SavedProperties")
                        .and_then(|sp| sp.get("m_TexEnvs"))
                        .and_then(|x| x.as_array())
                    {
                        for e in envs {
                            let Some(pair) = e.as_array() else { continue };
                            let Some(t) = pair.get(1).and_then(|p| p.get("m_Texture")) else {
                                continue;
                            };
                            let fid = t.get("m_FileID").and_then(|x| x.as_i64()).unwrap_or(0);
                            let pid = t.get("m_PathID").and_then(|x| x.as_i64()).unwrap_or(0);
                            if fid == 0 && pid != 0 {
                                inv.referenced_tex.insert((fi, pid));
                            }
                        }
                    }
                }
                C_MESHRENDERER | C_SKINNEDMESHRENDERER => {
                    *inv.other_classes.entry(obj.class_id).or_default() += 1;
                    let Ok(v) = sf.read_typetree(obj) else {
                        continue;
                    };
                    if let Some(mats) = v.get("m_Materials").and_then(|x| x.as_array()) {
                        for m in mats {
                            let fid = m.get("m_FileID").and_then(|x| x.as_i64()).unwrap_or(0);
                            let pid = m.get("m_PathID").and_then(|x| x.as_i64()).unwrap_or(0);
                            if fid == 0 && pid != 0 {
                                inv.bound_mats.insert((fi, pid));
                            }
                        }
                    }
                }
                C_TEXTASSET => {
                    let Ok(v) = sf.read_typetree(obj) else {
                        continue;
                    };
                    let name = v
                        .get("m_Name")
                        .and_then(|x| x.as_str())
                        .unwrap_or("?")
                        .to_string();
                    let script = match v.get("m_Script") {
                        Some(s) => match (s.as_str(), s.as_bytes()) {
                            (Some(st), _) => st.to_string(),
                            (None, Some(b)) => String::from_utf8_lossy(b).into_owned(),
                            _ => String::new(),
                        },
                        None => String::new(),
                    };
                    inv.textassets.push((name, script, key));
                }
                C_ASSETBUNDLE => {
                    let Ok(v) = sf.read_typetree(obj) else {
                        continue;
                    };
                    if let Some(cont) = v.get("m_Container").and_then(|x| x.as_array()) {
                        for e in cont {
                            let Some(pair) = e.as_array() else { continue };
                            let Some(path) = pair.first().and_then(|x| x.as_str()) else {
                                continue;
                            };
                            let asset_key = pair.get(1).and_then(|info| {
                                let a = info.get("asset")?;
                                let fid = a.get("m_FileID").and_then(|x| x.as_i64()).unwrap_or(0);
                                let pid = a.get("m_PathID").and_then(|x| x.as_i64()).unwrap_or(0);
                                if fid == 0 && pid != 0 {
                                    Some((fi, pid))
                                } else {
                                    None
                                }
                            });
                            inv.container.push((path.to_string(), asset_key));
                        }
                    }
                }
                other => {
                    *inv.other_classes.entry(other).or_default() += 1;
                }
            }
        }
        fi += 1;
    }
    Some(inv)
}

#[derive(Default)]
struct Delta {
    tolerated: Vec<&'static str>,
    structural: Vec<String>,
}

impl Delta {
    fn tolerate(&mut self, class: &'static str) {
        if !self.tolerated.contains(&class) {
            self.tolerated.push(class);
        }
    }
}

fn multiset_diff<T: Ord + Clone>(a: &[T], b: &[T]) -> (Vec<T>, Vec<T>) {
    let mut counts: BTreeMap<&T, i64> = BTreeMap::new();
    for x in a {
        *counts.entry(x).or_default() += 1;
    }
    for x in b {
        *counts.entry(x).or_default() -= 1;
    }
    let mut a_only = Vec::new();
    let mut b_only = Vec::new();
    for (x, c) in counts {
        for _ in 0..c.max(0) {
            a_only.push(x.clone());
        }
        for _ in 0..(-c).max(0) {
            b_only.push(x.clone());
        }
    }
    (a_only, b_only)
}

fn fmt_names<T: std::fmt::Debug>(items: &[T]) -> String {
    const CAP: usize = 6;
    let mut s = String::new();
    for (i, it) in items.iter().take(CAP).enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        s.push_str(&format!("{it:?}"));
    }
    if items.len() > CAP {
        s.push_str(&format!(", +{} more", items.len() - CAP));
    }
    s
}

fn normalize_metadata_script(script: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(script).ok()?;
    let obj = v.as_object()?;
    let mut deps: Vec<String> = obj
        .get("dependencies")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str())
                .map(|x| x.to_ascii_lowercase())
                .filter(|x| !x.starts_with("dcl/scene_ignore_"))
                .collect()
        })
        .unwrap_or_default();
    deps.sort();
    deps.dedup();
    let mut rest: BTreeMap<String, serde_json::Value> = obj
        .iter()
        .filter(|(k, _)| !matches!(k.as_str(), "timestamp" | "version" | "dependencies"))
        .map(|(k, val)| (k.clone(), val.clone()))
        .collect();
    rest.insert(
        "dependencies".to_string(),
        serde_json::Value::Array(deps.into_iter().map(serde_json::Value::String).collect()),
    );
    serde_json::to_string(&rest).ok()
}

fn analyze(r: &SideInv, o: &SideInv) -> Delta {
    let mut d = Delta::default();
    let mut r_skip: HashSet<ObjKey> = HashSet::new();
    let mut o_skip: HashSet<ObjKey> = HashSet::new();

    let tex_split = |inv: &SideInv, skip: &mut HashSet<ObjKey>| {
        let mut kept: Vec<(String, i64)> = Vec::new();
        let mut tol: Vec<(String, i64)> = Vec::new();
        for (name, fmt, key) in &inv.textures {
            if *fmt == TEX_FMT_ARGB32 && !inv.referenced_tex.contains(key) {
                tol.push((name.clone(), *fmt));
                skip.insert(*key);
            } else {
                kept.push((name.clone(), *fmt));
            }
        }
        kept.sort();
        tol.sort();
        (kept, tol)
    };
    let (r_tex, r_tex_tol) = tex_split(r, &mut r_skip);
    let (o_tex, o_tex_tol) = tex_split(o, &mut o_skip);
    if r_tex != o_tex {
        let (ro, oo) = multiset_diff(&r_tex, &o_tex);
        d.structural.push(format!(
            "texture-set-drift: ref-only [{}] ours-only [{}]",
            fmt_names(&ro),
            fmt_names(&oo)
        ));
    }
    if r_tex_tol != o_tex_tol {
        d.tolerate(TOL_UNREF_FMT5);
    }

    let mat_split = |inv: &SideInv, skip: &mut HashSet<ObjKey>| {
        let mut bound: Vec<String> = Vec::new();
        let mut unbound: Vec<String> = Vec::new();
        for (name, key) in &inv.materials {
            if inv.bound_mats.contains(key) {
                bound.push(name.clone());
            } else {
                unbound.push(name.clone());
                skip.insert(*key);
            }
        }
        bound.sort();
        unbound.sort();
        (bound, unbound)
    };
    let (r_bound, r_unbound) = mat_split(r, &mut r_skip);
    let (o_bound, o_unbound) = mat_split(o, &mut o_skip);
    if r_bound != o_bound {
        let (ro, oo) = multiset_diff(&r_bound, &o_bound);
        d.structural.push(format!(
            "bound-material-drift: ref-only [{}] ours-only [{}]",
            fmt_names(&ro),
            fmt_names(&oo)
        ));
    }
    if r_unbound != o_unbound {
        d.tolerate(TOL_UNREF_MATERIAL);
    }

    let ta_split = |inv: &SideInv, skip: &mut HashSet<ObjKey>| {
        let mut metas: Vec<String> = Vec::new();
        let mut others: Vec<(String, String)> = Vec::new();
        for (name, script, key) in &inv.textassets {
            if name == "metadata" {
                metas.push(script.clone());
                skip.insert(*key);
            } else {
                others.push((name.clone(), script.clone()));
            }
        }
        metas.sort();
        others.sort();
        (metas, others)
    };
    let (r_meta, r_ta) = ta_split(r, &mut r_skip);
    let (o_meta, o_ta) = ta_split(o, &mut o_skip);
    if r_meta.len() != o_meta.len() {
        d.tolerate(TOL_METADATA);
    } else {
        for (rm, om) in r_meta.iter().zip(o_meta.iter()) {
            if rm == om {
                continue;
            }
            match (normalize_metadata_script(rm), normalize_metadata_script(om)) {
                (Some(a), Some(b)) if a == b => d.tolerate(TOL_METADATA),
                _ => d
                    .structural
                    .push("metadata-content-drift: deps differ beyond case/dedup".to_string()),
            }
        }
    }
    if r_ta != o_ta {
        let names = |v: &[(String, String)]| v.iter().map(|x| x.0.clone()).collect::<Vec<_>>();
        let (ro, oo) = multiset_diff(&names(&r_ta), &names(&o_ta));
        if ro.is_empty() && oo.is_empty() {
            d.structural
                .push("textasset-content-drift (same names)".to_string());
        } else {
            d.structural.push(format!(
                "textasset-drift: ref-only [{}] ours-only [{}]",
                fmt_names(&ro),
                fmt_names(&oo)
            ));
        }
    }

    let cont_paths = |inv: &SideInv, skip: &HashSet<ObjKey>| -> Vec<String> {
        let mut v: Vec<String> = inv
            .container
            .iter()
            .filter(|(_, key)| key.map(|k| !skip.contains(&k)).unwrap_or(true))
            .map(|(p, _)| p.clone())
            .collect();
        v.sort();
        v
    };
    let r_cont = cont_paths(r, &r_skip);
    let o_cont = cont_paths(o, &o_skip);
    if r_cont != o_cont {
        let norm = |v: &[String]| -> Vec<String> {
            let mut n: Vec<String> = v.iter().map(|p| p.to_ascii_lowercase()).collect();
            n.sort();
            n.dedup();
            n
        };
        let (rn, on) = (norm(&r_cont), norm(&o_cont));
        if rn == on {
            d.tolerate(TOL_CONTAINER_CASE);
        } else {
            let (ro, oo) = multiset_diff(&rn, &on);
            d.structural.push(format!(
                "container-drift: ref-only [{}] ours-only [{}]",
                fmt_names(&ro),
                fmt_names(&oo)
            ));
        }
    }

    if r.other_classes != o.other_classes {
        let mut parts: Vec<String> = Vec::new();
        let classes: std::collections::BTreeSet<&i32> = r
            .other_classes
            .keys()
            .chain(o.other_classes.keys())
            .collect();
        for c in classes {
            let a = r.other_classes.get(c).copied().unwrap_or(0);
            let b = o.other_classes.get(c).copied().unwrap_or(0);
            if a != b {
                parts.push(format!("class {c}: ref={a} ours={b}"));
            }
        }
        d.structural
            .push(format!("class-count-drift: {}", parts.join("; ")));
    }

    d
}

fn classify(bundle_bytes: &[u8]) -> Option<&'static str> {
    let bundle = Bundle::load_bytes(bundle_bytes).ok()?;
    let mut classes: Vec<i32> = Vec::new();
    for f in &bundle.files {
        if let FileContent::Serialized(sf) = &f.content {
            for obj in &sf.objects {
                classes.push(obj.class_id);
            }
        }
    }
    Some(kind_of(&classes))
}

fn kind_of(classes: &[i32]) -> &'static str {
    let has = |c: i32| classes.contains(&c);
    let only_in = |allowed: &[i32]| classes.iter().all(|c| allowed.contains(c));

    if !has(C_GO) && !has(C_TRANSFORM) && only_in(&[C_ASSETBUNDLE, C_TEXTASSET]) {
        return "bundle-empty";
    }
    if has(C_TEXTURE2D)
        && has(C_TEXTASSET)
        && has(C_ASSETBUNDLE)
        && only_in(&[C_TEXTURE2D, C_TEXTASSET, C_ASSETBUNDLE])
    {
        return "standalone-texture";
    }
    if !has(C_GO)
        && !has(C_TRANSFORM)
        && has(C_TEXTURE2D)
        && has(C_ASSETBUNDLE)
        && only_in(&[C_TEXTURE2D, C_ASSETBUNDLE])
    {
        return "standalone-texture-legacy";
    }
    if has(C_ANIMATORCONTROLLER) && has(C_ANIMATOR) {
        return "glb-emote";
    }
    if has(C_SKINNEDMESHRENDERER) {
        return "glb-wearable";
    }
    if has(C_ANIMATION) && has(C_ANIMATIONCLIP) {
        return "glb-animated";
    }
    if has(C_GO) && !has(C_MESH) && has(C_MATERIAL) && has(C_TRANSFORM) {
        return "glb-scene-empty";
    }
    if has(C_GO) && has(C_MESH) && has(C_MESHCOLLIDER) && !has(C_MESHRENDERER) {
        return "glb-scene-collider";
    }
    if has(C_GO) && has(C_MESH) && has(C_MESHRENDERER) && has(C_MATERIAL) {
        return "glb-scene";
    }
    "other"
}

fn print_and_serialize(
    stats: &[BundleStat],
    json_out: Option<&str>,
    tolerant: bool,
) -> Result<usize> {
    let mut by_kind: HashMap<&'static str, KindAcc> = HashMap::new();
    let mut total = KindAcc::default();
    for s in stats {
        let acc = by_kind.entry(s.kind).or_default();
        acc.add(s);
        total.add(s);
    }
    let mut kinds: Vec<&&'static str> = by_kind.keys().collect();
    kinds.sort();
    println!("kind                    bundles  byte-id   smaller   larger    pair-bits     diff-bits     ppm");
    for k in &kinds {
        let acc = &by_kind[*k];
        println!(
            "{:<23} {:>7}  {:>7}   {:>7}   {:>7}   {:>11}   {:>11}  {:>8.1}",
            k,
            acc.n,
            acc.byte_id,
            acc.smaller,
            acc.larger,
            acc.pair_bits,
            acc.diff_bits,
            acc.ppm()
        );
    }
    println!(
        "{:<23} {:>7}  {:>7}   {:>7}   {:>7}   {:>11}   {:>11}  {:>8.1}",
        "TOTAL",
        total.n,
        total.byte_id,
        total.smaller,
        total.larger,
        total.pair_bits,
        total.diff_bits,
        total.ppm()
    );

    let mut n_structural = 0usize;
    let mut tol_counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut cat_counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    if tolerant {
        for s in stats {
            *cat_counts.entry(s.category).or_default() += 1;
            for t in &s.tolerated {
                *tol_counts.entry(t).or_default() += 1;
            }
        }
        n_structural = cat_counts.get("structural").copied().unwrap_or(0);
        let get = |k: &str| cat_counts.get(k).copied().unwrap_or(0);
        println!(
            "\ntolerant: identical={} payload-drift={} tolerated={} STRUCTURAL={}",
            get("identical"),
            get("payload-drift"),
            get("tolerated"),
            n_structural
        );
        if !tol_counts.is_empty() {
            let classes: Vec<String> = tol_counts.iter().map(|(k, v)| format!("{k}={v}")).collect();
            println!(
                "tolerance classes (benign, reported separately): {}",
                classes.join(" ")
            );
        }
        if n_structural > 0 {
            println!("structural pairs (REAL diffs):");
            const CAP: usize = 50;
            let mut shown = 0usize;
            for s in stats {
                if s.category != "structural" {
                    continue;
                }
                if shown < CAP {
                    println!("  {}/{}: {}", s.entity, s.bundle, s.structural.join("; "));
                }
                shown += 1;
            }
            if shown > CAP {
                println!("  … +{} more (see --json)", shown - CAP);
            }
        }
    }

    if let Some(p) = json_out {
        let mut v = serde_json::json!({
            "bundles": stats.iter().map(|s| {
                let mut b = serde_json::json!({
                    "entity": s.entity, "bundle": s.bundle, "kind": s.kind,
                    "ours_bytes": s.ours_bytes, "ref_bytes": s.ref_bytes,
                    "bits_diff": s.bits_diff, "byte_identical": s.byte_identical,
                });
                if tolerant {
                    b["category"] = serde_json::json!(s.category);
                    b["tolerated"] = serde_json::json!(s.tolerated);
                    b["structural"] = serde_json::json!(s.structural);
                }
                b
            }).collect::<Vec<_>>(),
            "per_kind": kinds.iter().map(|k| (k.to_string(), serde_json::json!({
                "bundles": by_kind[**k].n,
                "byte_identical": by_kind[**k].byte_id,
                "smaller": by_kind[**k].smaller,
                "larger": by_kind[**k].larger,
                "pair_bits": by_kind[**k].pair_bits,
                "diff_bits": by_kind[**k].diff_bits,
                "ppm": by_kind[**k].ppm(),
            }))).collect::<HashMap<_, _>>(),
            "total": { "bundles": total.n, "byte_identical": total.byte_id, "smaller": total.smaller, "larger": total.larger, "pair_bits": total.pair_bits, "diff_bits": total.diff_bits, "ppm": total.ppm() }
        });
        if tolerant {
            v["tolerant"] = serde_json::json!({
                "categories": cat_counts.iter().map(|(k, n)| (k.to_string(), *n)).collect::<HashMap<_, _>>(),
                "tolerance_classes": tol_counts.iter().map(|(k, n)| (k.to_string(), *n)).collect::<HashMap<_, _>>(),
                "structural_pairs": n_structural,
            });
        }
        std::fs::write(p, serde_json::to_vec_pretty(&v)?)?;
    }
    Ok(n_structural)
}

#[derive(Default)]
struct KindAcc {
    n: usize,
    byte_id: usize,
    smaller: usize,
    larger: usize,
    pair_bits: u64,
    diff_bits: u64,
}

impl KindAcc {
    fn add(&mut self, s: &BundleStat) {
        self.n += 1;
        if s.byte_identical {
            self.byte_id += 1;
        }
        match s.ours_bytes.cmp(&s.ref_bytes) {
            std::cmp::Ordering::Less => self.smaller += 1,
            std::cmp::Ordering::Greater => self.larger += 1,
            std::cmp::Ordering::Equal => {}
        }
        self.pair_bits += (s.ours_bytes.max(s.ref_bytes) as u64) * 8;
        self.diff_bits += s.bits_diff;
    }
    fn ppm(&self) -> f64 {
        if self.pair_bits == 0 {
            0.0
        } else {
            (self.diff_bits as f64 / self.pair_bits as f64) * 1e6
        }
    }
}

#[cfg(test)]
mod tolerant_tests {
    use super::*;

    fn base_inv() -> SideInv {
        let mut inv = SideInv::default();
        inv.textures.push(("tex".into(), 25, (0, 10)));
        inv.referenced_tex.insert((0, 10));
        inv.materials.push(("mat".into(), (0, 20)));
        inv.bound_mats.insert((0, 20));
        inv.textassets.push((
            "metadata".into(),
            r#"{"timestamp":1,"version":"7.0","dependencies":["Dep_windows"],"mainAsset":""}"#
                .into(),
            (0, 30),
        ));
        inv.container.push(("qmx.glb".into(), Some((0, 1))));
        inv.container.push(("metadata.json".into(), Some((0, 30))));
        inv.other_classes.insert(C_GO, 1);
        inv.other_classes.insert(C_TRANSFORM, 1);
        inv.other_classes.insert(C_MESH, 1);
        inv.other_classes.insert(C_MESHRENDERER, 1);
        inv
    }

    #[test]
    fn matching_structure_is_payload_drift_only() {
        let d = analyze(&base_inv(), &base_inv());
        assert!(d.structural.is_empty(), "{:?}", d.structural);
        assert!(d.tolerated.is_empty(), "{:?}", d.tolerated);
    }

    #[test]
    fn unreferenced_fmt5_duplicate_is_tolerated() {
        let mut r = base_inv();
        r.textures.push(("tex".into(), 5, (0, 11)));
        r.container.push(("tex.png".into(), Some((0, 11))));
        let o = base_inv();
        let d = analyze(&r, &o);
        assert!(d.structural.is_empty(), "{:?}", d.structural);
        assert_eq!(d.tolerated, vec![TOL_UNREF_FMT5]);
    }

    #[test]
    fn referenced_fmt5_texture_is_structural() {
        let mut r = base_inv();
        r.textures.push(("used".into(), 5, (0, 12)));
        r.referenced_tex.insert((0, 12));
        let o = base_inv();
        let d = analyze(&r, &o);
        assert_eq!(d.structural.len(), 1);
        assert!(d.structural[0].starts_with("texture-set-drift"));
    }

    #[test]
    fn metadata_timestamp_version_and_dep_case_are_tolerated() {
        let r = base_inv();
        let mut o = base_inv();
        o.textassets[0].1 = r#"{"timestamp":999,"version":"7.0","dependencies":["dep_windows","dep_windows","dcl/scene_ignore_windows"],"mainAsset":""}"#.into();
        let d = analyze(&r, &o);
        assert!(d.structural.is_empty(), "{:?}", d.structural);
        assert_eq!(d.tolerated, vec![TOL_METADATA]);
    }

    #[test]
    fn metadata_presence_is_tolerated() {
        let r = base_inv();
        let mut o = base_inv();
        o.textassets.clear();
        o.container.retain(|(p, _)| p != "metadata.json");
        let d = analyze(&r, &o);
        assert!(d.structural.is_empty(), "{:?}", d.structural);
        assert_eq!(d.tolerated, vec![TOL_METADATA]);
    }

    #[test]
    fn metadata_real_dep_drift_is_structural() {
        let r = base_inv();
        let mut o = base_inv();
        o.textassets[0].1 =
            r#"{"timestamp":1,"version":"7.0","dependencies":["other_windows"],"mainAsset":""}"#
                .into();
        let d = analyze(&r, &o);
        assert_eq!(d.structural.len(), 1);
        assert!(d.structural[0].starts_with("metadata-content-drift"));
    }

    #[test]
    fn container_case_and_dedup_are_tolerated() {
        let mut r = base_inv();
        r.container = vec![
            ("QmX.glb".into(), Some((0, 1))),
            ("QmX.glb".into(), Some((0, 1))),
            ("metadata.json".into(), Some((0, 30))),
        ];
        let o = base_inv();
        let d = analyze(&r, &o);
        assert!(d.structural.is_empty(), "{:?}", d.structural);
        assert_eq!(d.tolerated, vec![TOL_CONTAINER_CASE]);
    }

    #[test]
    fn container_real_drift_is_structural() {
        let mut r = base_inv();
        r.container.push(("extra.mat".into(), None));
        let o = base_inv();
        let d = analyze(&r, &o);
        assert_eq!(d.structural.len(), 1);
        assert!(d.structural[0].starts_with("container-drift"));
    }

    #[test]
    fn extra_unbound_material_is_tolerated_with_its_container_entry() {
        let mut r = base_inv();
        r.materials.push(("DCL_Scene".into(), (0, 40)));
        r.container.push(("DCL_Scene.mat".into(), Some((0, 40))));
        let o = base_inv();
        let d = analyze(&r, &o);
        assert!(d.structural.is_empty(), "{:?}", d.structural);
        assert_eq!(d.tolerated, vec![TOL_UNREF_MATERIAL]);
    }

    #[test]
    fn extra_bound_material_is_structural() {
        let mut r = base_inv();
        r.materials.push(("real".into(), (0, 41)));
        r.bound_mats.insert((0, 41));
        let o = base_inv();
        let d = analyze(&r, &o);
        assert_eq!(d.structural.len(), 1);
        assert!(d.structural[0].starts_with("bound-material-drift"));
    }

    #[test]
    fn class_count_drift_is_structural() {
        let r = base_inv();
        let mut o = base_inv();
        *o.other_classes.entry(C_ANIMATOR).or_default() += 1;
        let d = analyze(&r, &o);
        assert_eq!(d.structural.len(), 1);
        assert!(d.structural[0].starts_with("class-count-drift"));
    }

    #[test]
    fn combined_benign_deltas_stay_tolerated() {
        let mut r = base_inv();
        r.textures.push(("tex".into(), 5, (0, 11)));
        r.container.push(("tex.png".into(), Some((0, 11))));
        r.materials.push(("DCL_Scene".into(), (0, 40)));
        r.container.push(("DCL_Scene.mat".into(), Some((0, 40))));
        let mut o = base_inv();
        o.textassets[0].1 = r#"{"timestamp":42,"version":"7.0","dependencies":["dep_windows","dcl/scene_ignore_windows"],"mainAsset":""}"#.into();
        let d = analyze(&r, &o);
        assert!(d.structural.is_empty(), "{:?}", d.structural);
        let mut tol = d.tolerated.clone();
        tol.sort();
        assert_eq!(tol, vec![TOL_METADATA, TOL_UNREF_FMT5, TOL_UNREF_MATERIAL]);
    }

    #[test]
    fn normalize_metadata_script_canonicalizes() {
        let a = normalize_metadata_script(
            r#"{"timestamp":1,"version":"6.0","dependencies":["B_Windows","a_windows"],"mainAsset":""}"#,
        )
        .unwrap();
        let b = normalize_metadata_script(
            r#"{"timestamp":2,"version":"7.0","dependencies":["a_windows","b_windows","b_windows","dcl/scene_ignore_windows"],"mainAsset":""}"#,
        )
        .unwrap();
        assert_eq!(a, b);
        assert!(normalize_metadata_script("not json").is_none());
    }

    #[test]
    fn multiset_diff_counts() {
        let (a, b) = multiset_diff(&["x", "x", "y"], &["x", "z"]);
        assert_eq!(a, vec!["x", "y"]);
        assert_eq!(b, vec!["z"]);
    }
}
