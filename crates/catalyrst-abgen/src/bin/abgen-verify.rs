use abgen::unity::bundle_file::{Bundle, FileContent};
use abgen::Result;
use rayon::prelude::*;
use std::collections::HashMap;
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
        "usage: abgen-verify <ours-dir> <reference-dir> [-j JOBS] [--json PATH]\n\
         \n\
         walks <reference-dir>/<entity>/<bundle-name> looking for a matching\n\
         <ours-dir>/<entity>/<bundle-name>; diffs bytes and classifies each\n\
         bundle by Unity-class set (standalone-texture / standalone-texture-legacy\n\
         / glb-scene / glb-emote / glb-wearable / glb-animated / glb-with-morph /\n\
         glb-scene-collider / glb-scene-empty / bundle-empty / other). Prints\n\
         per-kind ppm-bits and size-delta histogram. Writes machine-readable\n\
         JSON if --json given."
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
        .for_each(
            |(ref_path, ours_path, ent, name)| match stat_pair(ref_path, ours_path) {
                Ok(s) => per_bundle.lock().unwrap().push(BundleStat {
                    entity: ent.clone(),
                    bundle: name.clone(),
                    ..s
                }),
                Err(e) => eprintln!("err {ent}/{name}: {e}"),
            },
        );

    let stats = per_bundle.into_inner().unwrap();
    print_and_serialize(&stats, json_out.as_deref())?;
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
}

fn enumerate_pairs(ref_root: &Path, ours_root: &Path) -> Vec<(PathBuf, PathBuf, String, String)> {
    let mut out = Vec::new();
    let Ok(ent_iter) = std::fs::read_dir(ref_root) else {
        return out;
    };
    for ent in ent_iter.flatten() {
        let ft = match ent.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if ft.is_file() {
            let fname = ent.file_name().to_string_lossy().into_owned();
            let ours = ours_root.join(&fname);
            if ours.exists() {
                out.push((ent.path(), ours, String::new(), fname));
            }
            continue;
        }
        if !ft.is_dir() {
            continue;
        }
        let ent_name = ent.file_name().to_string_lossy().into_owned();
        let Ok(file_iter) = std::fs::read_dir(ent.path()) else {
            continue;
        };
        for f in file_iter.flatten() {
            if !f.file_type().map(|t| t.is_file()).unwrap_or(false) {
                continue;
            }
            let fname = f.file_name().to_string_lossy().into_owned();
            let ours = ours_root.join(&ent_name).join(&fname);
            if !ours.exists() {
                continue;
            }
            out.push((f.path(), ours, ent_name.clone(), fname));
        }
    }
    out
}

fn stat_pair(ref_path: &Path, ours_path: &Path) -> Result<BundleStat> {
    let r_bytes = std::fs::read(ref_path)?;
    let o_bytes = std::fs::read(ours_path)?;
    let kind = classify(&r_bytes).unwrap_or("other");
    let bits = bits_diff(&o_bytes, &r_bytes);
    Ok(BundleStat {
        entity: String::new(),
        bundle: String::new(),
        kind,
        ours_bytes: o_bytes.len(),
        ref_bytes: r_bytes.len(),
        bits_diff: bits,
        byte_identical: o_bytes == r_bytes,
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

fn print_and_serialize(stats: &[BundleStat], json_out: Option<&str>) -> Result<()> {
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
    if let Some(p) = json_out {
        let v = serde_json::json!({
            "bundles": stats.iter().map(|s| serde_json::json!({
                "entity": s.entity, "bundle": s.bundle, "kind": s.kind,
                "ours_bytes": s.ours_bytes, "ref_bytes": s.ref_bytes,
                "bits_diff": s.bits_diff, "byte_identical": s.byte_identical,
            })).collect::<Vec<_>>(),
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
        std::fs::write(p, serde_json::to_vec_pretty(&v)?)?;
    }
    Ok(())
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
