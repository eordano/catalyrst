use crate::build::{build_one, bundle_tmp_path, derive_one_entity};
use crate::{BundleSpec, EffectiveToggles};
use abgen::local_store::LocalContentStore;
use abgen::naming;
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

/// Byte-material bundle flavor: emote/scene/wearable gate which animations a
/// GLB bundle carries. Ordering IS the S1 winner precedence (emote > scene >
/// wearable): an emote missing its AnimatorController is nonfunctional, scene
/// AnimationClips are inert extras for wearable loaders, wearable flavor
/// silently strips scene animations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Flavor {
    Wearable,
    Scene,
    Emote,
}

/// Platform-neutral key over every encode-affecting BundleSpec parameter, so
/// two entities claiming the same <hash>_<platform> bundle name compare equal
/// iff their builds are byte-identical (modulo GPU batch order). Contains no
/// platform anywhere: mac and windows elect the same winner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum VariantKey {
    Glb {
        inputs: Vec<(String, String)>,
        flavor: Flavor,
        ext_is_gltf: bool,
    },
    Tex {
        mr: bool,
        cs0: bool,
        nm: bool,
        key_ext: String,
    },
}

impl VariantKey {
    pub(crate) fn digest(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        match self {
            VariantKey::Glb {
                inputs,
                flavor,
                ext_is_gltf,
            } => {
                h.update(b"glb\0");
                h.update([*flavor as u8, u8::from(*ext_is_gltf)]);
                for (u, c) in inputs {
                    h.update(u.as_bytes());
                    h.update(b"\0");
                    h.update(c.as_bytes());
                    h.update(b"\0");
                }
            }
            VariantKey::Tex {
                mr,
                cs0,
                nm,
                key_ext,
            } => {
                h.update(b"tex\0");
                h.update([u8::from(*mr), u8::from(*cs0), u8::from(*nm)]);
                h.update(key_ext.as_bytes());
            }
        }
        h.finalize().into()
    }

    fn is_glb(&self) -> bool {
        matches!(self, VariantKey::Glb { .. })
    }
}

pub(crate) struct Variant {
    pub(crate) digest: [u8; 32],
    pub(crate) key: VariantKey,
    pub(crate) claimants: Vec<String>,
}

/// Cross-entity dedup record for one bundle_name. `path` is the first file
/// actually materialized under that name (hardlink source during the pass);
/// entry existence implies that file exists. `variants` accumulates every
/// claimant (built, hardlinked, or skip-existing-skipped) under its variant
/// digest so the post-pass reconcile can elect a canonical winner.
pub(crate) struct FwEntry {
    pub(crate) path: PathBuf,
    pub(crate) variants: Vec<Variant>,
}

pub(crate) type FirstWritten = Mutex<HashMap<String, FwEntry>>;

pub(crate) fn record_claim(
    fw: &FirstWritten,
    bundle_name: &str,
    path: &Path,
    key: Option<&VariantKey>,
    entity_id: &str,
) {
    let mut m = fw.lock().unwrap();
    let entry = m.entry(bundle_name.to_string()).or_insert_with(|| FwEntry {
        path: path.to_path_buf(),
        variants: Vec::new(),
    });
    if let Some(k) = key {
        let d = k.digest();
        match entry.variants.iter_mut().find(|v| v.digest == d) {
            Some(v) => v.claimants.push(entity_id.to_string()),
            None => entry.variants.push(Variant {
                digest: d,
                key: k.clone(),
                claimants: vec![entity_id.to_string()],
            }),
        }
    }
}

type DepUriCache = RwLock<HashMap<(String, String), Arc<Vec<String>>>>;

fn dep_uri_cache() -> &'static DepUriCache {
    static CACHE: OnceLock<DepUriCache> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

fn dep_uris_cached(store: &LocalContentStore, cid: &str, ext: &str) -> Arc<Vec<String>> {
    let key = (cid.to_string(), ext.to_string());
    if let Some(u) = dep_uri_cache().read().unwrap().get(&key) {
        return u.clone();
    }
    let uris = Arc::new(
        store
            .fetch_mmap(cid)
            .ok()
            .and_then(|d| naming::parse_gltf_dep_refs(&d, ext).ok())
            .unwrap_or_default(),
    );
    dep_uri_cache()
        .write()
        .unwrap()
        .entry(key)
        .or_insert_with(|| uris.clone())
        .clone()
}

fn tex_filename_ext(source: &str) -> Option<String> {
    let last = source.rsplit(['/', '\\']).next().unwrap_or(source);
    let dot = last.rfind('.')?;
    let ext = &last[dot..];
    let lo = ext.to_ascii_lowercase();
    matches!(lo.as_str(), ".png" | ".jpg" | ".jpeg" | ".psd").then(|| ext.to_string())
}

fn tex_magic_ext(data: &[u8]) -> &'static str {
    if data.len() >= 8 && &data[0..8] == b"\x89PNG\r\n\x1a\n" {
        ".png"
    } else if data.len() >= 2 && data[0] == 0xFF && data[1] == 0xD8 {
        ".jpg"
    } else if data.len() >= 4 && &data[0..4] == b"8BPS" {
        ".psd"
    } else {
        ".png"
    }
}

/// Route family exactly as build_bundle dispatches: GLB iff source ends
/// .gltf or the blob magic is b"glTF"; everything else is a standalone
/// texture. GLB inputs are the FULL resolved URI closure (images AND .gltf
/// buffers via parse_gltf_dep_refs) as raw (uri, content-hash) pairs —
/// unresolved URIs are omitted, which itself distinguishes the variants.
pub(crate) fn variant_key_for(
    store: &LocalContentStore,
    content_by_file: &HashMap<String, String>,
    spec: &BundleSpec,
    toggles: EffectiveToggles,
) -> Option<VariantKey> {
    let source = spec
        .source_file
        .clone()
        .unwrap_or_else(|| format!("{}.glb", spec.cid));
    let src_l = source.to_lowercase();
    let ext_is_gltf = src_l.ends_with(".gltf");
    let glb_family = ext_is_gltf || {
        let d = store.fetch_mmap(&spec.cid).ok()?;
        d.len() >= 4 && &d[0..4] == b"glTF"
    };
    if glb_family {
        let ext = if ext_is_gltf { ".gltf" } else { ".glb" };
        let uris = dep_uris_cached(store, &spec.cid, ext);
        let mut inputs: Vec<(String, String)> = Vec::new();
        for uri in uris.iter() {
            let Ok(resolved) = naming::resolve_uri_to_content_file(uri, &source) else {
                continue;
            };
            if let Some(h) = content_by_file.get(&resolved.to_lowercase()) {
                inputs.push((uri.clone(), h.clone()));
            }
        }
        inputs.sort();
        inputs.dedup();
        let is_emote = (!toggles.collection_mode
            && matches!(spec.entity_type.as_deref(), Some(t) if t.eq_ignore_ascii_case("emote")))
            || src_l.ends_with("_emote.glb");
        let flavor = if is_emote {
            Flavor::Emote
        } else if !toggles.collection_mode
            && matches!(spec.entity_type.as_deref(), Some(t) if t.eq_ignore_ascii_case("wearable"))
        {
            Flavor::Wearable
        } else {
            Flavor::Scene
        };
        Some(VariantKey::Glb {
            inputs,
            flavor,
            ext_is_gltf,
        })
    } else {
        let key_ext = match tex_filename_ext(&source) {
            Some(e) => e,
            None => tex_magic_ext(&store.fetch_mmap(&spec.cid).ok()?).to_string(),
        };
        Some(VariantKey::Tex {
            mr: spec.model_referenced,
            cs0: spec.standalone_color_space == Some(0),
            nm: spec.standalone_normal,
            key_ext,
        })
    }
}

pub(crate) enum Candidate<'a> {
    TexUnion {
        mr: bool,
        cs0: bool,
        nm: bool,
        key_ext: String,
    },
    Variant(&'a Variant),
}

fn conflict_order(a: &Variant, b: &Variant) -> std::cmp::Ordering {
    let na = match &a.key {
        VariantKey::Glb { inputs, .. } => inputs.len(),
        VariantKey::Tex { .. } => 0,
    };
    let nb = match &b.key {
        VariantKey::Glb { inputs, .. } => inputs.len(),
        VariantKey::Tex { .. } => 0,
    };
    b.claimants
        .len()
        .cmp(&a.claimants.len())
        .then(nb.cmp(&na))
        .then(a.digest.cmp(&b.digest))
}

fn strictly_nested(sorted_asc: &[&Variant]) -> bool {
    sorted_asc.windows(2).all(|w| {
        let (VariantKey::Glb { inputs: a, .. }, VariantKey::Glb { inputs: b, .. }) =
            (&w[0].key, &w[1].key)
        else {
            return false;
        };
        if a.len() >= b.len() {
            return false;
        }
        let bs: BTreeSet<&(String, String)> = b.iter().collect();
        a.iter().all(|p| bs.contains(p))
    })
}

/// Deterministic canonical-winner ranking for one divergent bundle_name.
/// Route-mixed names resolve to the family matching the blob content; GLB
/// applies flavor precedence then superset-of-nested-deps, else claimant
/// majority; TEX synthesizes the usage-evidence union (the same union the
/// converter's intra-entity scan applies). Later entries are the
/// deterministic fallbacks if the winner build errors.
pub(crate) fn rank_candidates<'a>(
    variants: &'a [Variant],
    magic_is_glb: bool,
    magic_tex_ext: &str,
) -> Vec<Candidate<'a>> {
    let glb: Vec<&Variant> = variants.iter().filter(|v| v.key.is_glb()).collect();
    let tex: Vec<&Variant> = variants.iter().filter(|v| !v.key.is_glb()).collect();
    let glb_wins = if glb.is_empty() || tex.is_empty() {
        !glb.is_empty()
    } else {
        magic_is_glb
    };
    let mut ranked: Vec<Candidate<'a>> = Vec::new();
    if glb_wins {
        ranked.extend(rank_glb(&glb));
        ranked.extend(rank_tex(&tex, magic_tex_ext));
    } else {
        ranked.extend(rank_tex(&tex, magic_tex_ext));
        ranked.extend(rank_glb(&glb));
    }
    ranked
}

fn rank_glb<'a>(glb: &[&'a Variant]) -> Vec<Candidate<'a>> {
    if glb.is_empty() {
        return Vec::new();
    }
    let flavor_of = |v: &Variant| match &v.key {
        VariantKey::Glb { flavor, .. } => *flavor,
        VariantKey::Tex { .. } => Flavor::Wearable,
    };
    let max_flavor = glb.iter().map(|v| flavor_of(v)).max().unwrap();
    let (mut top, mut rest): (Vec<&Variant>, Vec<&Variant>) =
        glb.iter().partition(|v| flavor_of(v) == max_flavor);
    top.sort_by(|a, b| conflict_order(a, b));
    let mut asc: Vec<&Variant> = top.clone();
    asc.sort_by_key(|v| match &v.key {
        VariantKey::Glb { inputs, .. } => inputs.len(),
        VariantKey::Tex { .. } => 0,
    });
    if top.len() >= 2 && strictly_nested(&asc) {
        asc.reverse();
        top = asc;
    }
    rest.sort_by(|a, b| flavor_of(b).cmp(&flavor_of(a)).then(conflict_order(a, b)));
    top.into_iter()
        .chain(rest)
        .map(Candidate::Variant)
        .collect()
}

fn rank_tex<'a>(tex: &[&'a Variant], magic_tex_ext: &str) -> Vec<Candidate<'a>> {
    if tex.is_empty() {
        return Vec::new();
    }
    let flags = |v: &Variant| match &v.key {
        VariantKey::Tex {
            mr,
            cs0,
            nm,
            key_ext,
        } => (*mr, *cs0, *nm, key_ext.clone()),
        VariantKey::Glb { .. } => (false, false, false, String::new()),
    };
    let mr = tex.iter().any(|v| flags(v).0);
    let cs0 = tex.iter().any(|v| {
        let f = flags(v);
        f.1 || f.2
    });
    let nm = tex.iter().any(|v| flags(v).2)
        && !tex.iter().any(|v| {
            let f = flags(v);
            f.1 && !f.2
        });
    let mut ext_stats: HashMap<String, (usize, [u8; 32])> = HashMap::new();
    for v in tex {
        let e = flags(v).3;
        let s = ext_stats.entry(e).or_insert((0, v.digest));
        s.0 += v.claimants.len();
        s.1 = s.1.min(v.digest);
    }
    let key_ext = ext_stats
        .into_iter()
        .max_by(|(ea, (ca, da)), (eb, (cb, db))| {
            ca.cmp(cb)
                .then((ea == magic_tex_ext).cmp(&(eb == magic_tex_ext)))
                .then(db.cmp(da))
        })
        .map(|(e, _)| e)
        .unwrap_or_else(|| ".png".to_string());
    let mut rest: Vec<&Variant> = tex.to_vec();
    rest.sort_by(|a, b| conflict_order(a, b));
    std::iter::once(Candidate::TexUnion {
        mr,
        cs0,
        nm,
        key_ext,
    })
    .chain(rest.into_iter().map(Candidate::Variant))
    .collect()
}

fn tex_spec(
    cid: &str,
    bundle_name: &str,
    mr: bool,
    cs0: bool,
    nm: bool,
    key_ext: &str,
) -> BundleSpec {
    BundleSpec {
        cid: cid.to_string(),
        bundle_name: bundle_name.to_string(),
        source_file: Some(format!("tex{key_ext}")),
        entity_type: None,
        metadata_deps: Vec::new(),
        model_referenced: mr,
        expect_hash: None,
        standalone_color_space: Some(if cs0 { 0 } else { 1 }),
        standalone_normal: nm,
        force_default_material: false,
    }
}

#[derive(Default)]
pub(crate) struct ReconcileStats {
    pub(crate) divergent: usize,
    pub(crate) rebuilt: usize,
    pub(crate) relinked: usize,
    pub(crate) errs: usize,
}

fn build_candidate(
    store: &LocalContentStore,
    platform: &str,
    cid: &str,
    bundle_name: &str,
    cand: &Candidate<'_>,
    toggles: EffectiveToggles,
    uri_cache: &abgen::glbscan::UriCache,
    tmp: &Path,
) -> abgen::Result<()> {
    match cand {
        Candidate::TexUnion {
            mr,
            cs0,
            nm,
            key_ext,
        } => {
            let spec = tex_spec(cid, bundle_name, *mr, *cs0, *nm, key_ext);
            build_one(store, &HashMap::new(), &spec, tmp, toggles)
        }
        Candidate::Variant(v) => match &v.key {
            VariantKey::Tex {
                mr,
                cs0,
                nm,
                key_ext,
            } => {
                let spec = tex_spec(cid, bundle_name, *mr, *cs0, *nm, key_ext);
                build_one(store, &HashMap::new(), &spec, tmp, toggles)
            }
            VariantKey::Glb { .. } => {
                let rep = v
                    .claimants
                    .iter()
                    .min()
                    .ok_or_else(|| anyhow::anyhow!("variant has no claimants"))?;
                let entry = derive_one_entity(store, rep, platform, uri_cache)
                    .ok_or_else(|| anyhow::anyhow!("re-derive of {rep} failed"))?;
                let spec = entry
                    .bundles
                    .iter()
                    .find(|b| b.bundle_name == bundle_name)
                    .ok_or_else(|| {
                        anyhow::anyhow!("re-derive of {rep} no longer claims {bundle_name}")
                    })?;
                let content_by_file: HashMap<String, String> = entry
                    .content
                    .iter()
                    .map(|c| (c.file.to_lowercase(), c.hash.clone()))
                    .collect();
                let rekey = variant_key_for(store, &content_by_file, spec, toggles)
                    .ok_or_else(|| anyhow::anyhow!("re-key of {bundle_name} in {rep} failed"))?;
                if rekey.digest() != v.digest {
                    anyhow::bail!("re-derived variant of {bundle_name} in {rep} changed digest");
                }
                build_one(store, &content_by_file, spec, tmp, toggles)
            }
        },
    }
}

/// Post-pass winner reconcile: for every bundle_name claimed under >=2
/// distinct variant digests, elect the canonical winner, ALWAYS rebuild it
/// (skip-existing bytes from pre-S1 runs are untrusted), then re-link every
/// claimant path to the fresh inode via hardlink+rename (rename alone would
/// split the hardlink group). Makes final bytes a pure function of the
/// catalog, independent of rayon arrival order.
pub(crate) fn reconcile_divergent(
    store: &LocalContentStore,
    out_root: &Path,
    platform: &str,
    toggles: EffectiveToggles,
    fw: FirstWritten,
) -> ReconcileStats {
    use rayon::prelude::*;
    let map = fw.into_inner().unwrap();
    let uri_cache = abgen::glbscan::UriCache::new();
    let mut divergent: Vec<(&String, &FwEntry)> =
        map.iter().filter(|(_, e)| e.variants.len() >= 2).collect();
    divergent.sort_by_key(|(n, _)| n.as_str());
    let rebuilt = AtomicUsize::new(0);
    let relinked = AtomicUsize::new(0);
    let errs = AtomicUsize::new(0);
    let suffix = format!("_{platform}");
    divergent.par_iter().for_each(|(name, entry)| {
        let Some(cid) = name.strip_suffix(&suffix).filter(|c| !c.is_empty()) else {
            errs.fetch_add(1, Ordering::Relaxed);
            eprintln!("reconcile {name}: bundle name lacks _{platform} suffix");
            return;
        };
        let route_mixed = entry.variants.iter().any(|v| v.key.is_glb())
            && entry.variants.iter().any(|v| !v.key.is_glb());
        let needs_tex_ext = entry
            .variants
            .iter()
            .any(|v| matches!(&v.key, VariantKey::Tex { .. }));
        let (magic_is_glb, magic_ext) = if route_mixed || needs_tex_ext {
            match store.fetch_mmap(cid) {
                Ok(d) => (blob_is_gltf(&d), tex_magic_ext(&d).to_string()),
                Err(_) => (false, ".png".to_string()),
            }
        } else {
            (true, ".png".to_string())
        };
        let ranked = rank_candidates(&entry.variants, magic_is_glb, &magic_ext);
        let mut paths: Vec<PathBuf> = entry
            .variants
            .iter()
            .flat_map(|v| v.claimants.iter())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|ent| out_root.join(ent).join(platform).join(name.as_str()))
            .collect();
        paths.sort();
        let Some(first) = paths.first().cloned() else {
            return;
        };
        let tmp = bundle_tmp_path(&first);
        let mut built_ok = false;
        for cand in &ranked {
            match build_candidate(store, platform, cid, name, cand, toggles, &uri_cache, &tmp) {
                Ok(()) => {
                    built_ok = true;
                    break;
                }
                Err(e) => eprintln!("reconcile {name}: candidate build failed: {e:#}"),
            }
        }
        if !built_ok {
            errs.fetch_add(1, Ordering::Relaxed);
            eprintln!("reconcile {name}: all {} candidates failed", ranked.len());
            return;
        }
        rebuilt.fetch_add(1, Ordering::Relaxed);
        for p in &paths {
            let t2 = bundle_tmp_path(p);
            let staged = std::fs::hard_link(&tmp, &t2).is_ok() || std::fs::copy(&tmp, &t2).is_ok();
            if staged && std::fs::rename(&t2, p).is_ok() {
                relinked.fetch_add(1, Ordering::Relaxed);
            } else {
                let _ = std::fs::remove_file(&t2);
                errs.fetch_add(1, Ordering::Relaxed);
                eprintln!("reconcile {name}: relink {} failed", p.display());
            }
        }
        let _ = std::fs::remove_file(&tmp);
    });
    ReconcileStats {
        divergent: divergent.len(),
        rebuilt: rebuilt.into_inner(),
        relinked: relinked.into_inner(),
        errs: errs.into_inner(),
    }
}

fn blob_is_gltf(data: &[u8]) -> bool {
    if data.len() >= 4 && &data[0..4] == b"glTF" {
        return true;
    }
    serde_json::from_slice::<serde_json::Value>(data)
        .ok()
        .map(|v| v.get("asset").is_some())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glb_variant(inputs: &[(&str, &str)], flavor: Flavor, claimants: &[&str]) -> Variant {
        let key = VariantKey::Glb {
            inputs: inputs
                .iter()
                .map(|(u, h)| (u.to_string(), h.to_string()))
                .collect(),
            flavor,
            ext_is_gltf: false,
        };
        Variant {
            digest: key.digest(),
            key,
            claimants: claimants.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn tex_variant(mr: bool, cs0: bool, nm: bool, ext: &str, claimants: &[&str]) -> Variant {
        let key = VariantKey::Tex {
            mr,
            cs0,
            nm,
            key_ext: ext.to_string(),
        };
        Variant {
            digest: key.digest(),
            key,
            claimants: claimants.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn digest_of(c: &Candidate<'_>) -> Option<[u8; 32]> {
        match c {
            Candidate::Variant(v) => Some(v.digest),
            Candidate::TexUnion { .. } => None,
        }
    }

    #[test]
    fn digest_is_order_insensitive_via_sorted_inputs_and_flavor_sensitive() {
        let a = glb_variant(&[("a.png", "h1"), ("b.png", "h2")], Flavor::Scene, &["e1"]);
        let b = glb_variant(&[("a.png", "h1"), ("b.png", "h2")], Flavor::Scene, &["e2"]);
        assert_eq!(a.digest, b.digest);
        let c = glb_variant(&[("a.png", "h1"), ("b.png", "h2")], Flavor::Emote, &["e1"]);
        assert_ne!(a.digest, c.digest);
        let d = glb_variant(&[("a.png", "h9"), ("b.png", "h2")], Flavor::Scene, &["e1"]);
        assert_ne!(a.digest, d.digest);
    }

    #[test]
    fn nested_deps_pick_superset_even_against_majority() {
        let small = glb_variant(&[("a.png", "h1")], Flavor::Scene, &["e1", "e2", "e3"]);
        let big = glb_variant(&[("a.png", "h1"), ("b.png", "h2")], Flavor::Scene, &["e4"]);
        let vs = [small, big];
        let ranked = rank_candidates(&vs, true, ".png");
        let VariantKey::Glb { inputs, .. } = (match &ranked[0] {
            Candidate::Variant(v) => &v.key,
            _ => panic!("expected variant"),
        }) else {
            panic!("expected glb")
        };
        assert_eq!(inputs.len(), 2);
    }

    #[test]
    fn conflicting_deps_pick_claimant_majority() {
        let a = glb_variant(&[("a.png", "h1")], Flavor::Scene, &["e1"]);
        let b = glb_variant(&[("a.png", "hX")], Flavor::Scene, &["e2", "e3"]);
        let expect = b.digest;
        let vs = [a, b];
        let ranked = rank_candidates(&vs, true, ".png");
        assert_eq!(digest_of(&ranked[0]), Some(expect));
    }

    #[test]
    fn conflict_ties_break_on_inputs_then_digest() {
        let a = glb_variant(&[("a.png", "h1")], Flavor::Scene, &["e1"]);
        let b = glb_variant(&[("a.png", "hX"), ("b.png", "h2")], Flavor::Scene, &["e2"]);
        let expect = b.digest;
        let vs = [a, b];
        let ranked = rank_candidates(&vs, true, ".png");
        assert_eq!(digest_of(&ranked[0]), Some(expect));

        let c = glb_variant(&[("a.png", "h1")], Flavor::Scene, &["e1"]);
        let d = glb_variant(&[("a.png", "hX")], Flavor::Scene, &["e2"]);
        let expect = c.digest.min(d.digest);
        let vs = [c, d];
        let ranked = rank_candidates(&vs, true, ".png");
        assert_eq!(digest_of(&ranked[0]), Some(expect));
    }

    #[test]
    fn flavor_precedence_beats_deps_and_majority() {
        let wearable = glb_variant(
            &[("a.png", "h1"), ("b.png", "h2")],
            Flavor::Wearable,
            &["e1", "e2"],
        );
        let emote = glb_variant(&[("a.png", "h1")], Flavor::Emote, &["e3"]);
        let scene = glb_variant(&[("a.png", "h1")], Flavor::Scene, &["e4", "e5", "e6"]);
        let expect = emote.digest;
        let vs = [wearable, emote, scene];
        let ranked = rank_candidates(&vs, true, ".png");
        assert_eq!(digest_of(&ranked[0]), Some(expect));
        assert_eq!(ranked.len(), 3);
    }

    #[test]
    fn tex_union_merges_usage_evidence() {
        let a = tex_variant(true, false, false, ".png", &["e1"]);
        let b = tex_variant(false, true, true, ".png", &["e2"]);
        let vs = [a, b];
        let ranked = rank_candidates(&vs, false, ".png");
        match &ranked[0] {
            Candidate::TexUnion {
                mr,
                cs0,
                nm,
                key_ext,
            } => {
                assert!(*mr);
                assert!(*cs0);
                assert!(*nm);
                assert_eq!(key_ext, ".png");
            }
            _ => panic!("expected union first"),
        }
    }

    #[test]
    fn tex_union_drops_normal_on_mr_color_evidence() {
        let a = tex_variant(false, true, false, ".png", &["e1"]);
        let b = tex_variant(false, true, true, ".png", &["e2"]);
        let vs = [a, b];
        let ranked = rank_candidates(&vs, false, ".png");
        match &ranked[0] {
            Candidate::TexUnion { mr, cs0, nm, .. } => {
                assert!(!*mr);
                assert!(*cs0);
                assert!(!*nm);
            }
            _ => panic!("expected union first"),
        }
    }

    #[test]
    fn tex_union_keeps_normal_when_unopposed() {
        let a = tex_variant(false, true, true, ".png", &["e1"]);
        let b = tex_variant(false, false, false, ".png", &["e2"]);
        let vs = [a, b];
        let ranked = rank_candidates(&vs, false, ".png");
        match &ranked[0] {
            Candidate::TexUnion { cs0, nm, .. } => {
                assert!(*cs0);
                assert!(*nm);
            }
            _ => panic!("expected union first"),
        }
    }

    #[test]
    fn tex_ext_majority_then_magic() {
        let a = tex_variant(false, false, false, ".jpg", &["e1", "e2"]);
        let b = tex_variant(false, true, false, ".png", &["e3"]);
        let vs = [a, b];
        let ranked = rank_candidates(&vs, false, ".png");
        match &ranked[0] {
            Candidate::TexUnion { key_ext, .. } => assert_eq!(key_ext, ".jpg"),
            _ => panic!("expected union first"),
        }

        let c = tex_variant(false, false, false, ".jpg", &["e1"]);
        let d = tex_variant(false, true, false, ".png", &["e3"]);
        let vs = [c, d];
        let ranked = rank_candidates(&vs, false, ".png");
        match &ranked[0] {
            Candidate::TexUnion { key_ext, .. } => assert_eq!(key_ext, ".png"),
            _ => panic!("expected union first"),
        }
    }

    #[test]
    fn route_mixed_follows_blob_magic() {
        let g = glb_variant(&[], Flavor::Scene, &["e1", "e2", "e3"]);
        let t = tex_variant(false, false, false, ".png", &["e4"]);
        let gexpect = g.digest;
        let vs = [g, t];
        let ranked = rank_candidates(&vs, false, ".png");
        match &ranked[0] {
            Candidate::TexUnion { .. } => {}
            _ => panic!("expected tex family first when magic says tex"),
        }
        let g2 = glb_variant(&[], Flavor::Scene, &["e1"]);
        let t2 = tex_variant(false, false, false, ".png", &["e4", "e5"]);
        let vs = [g2, t2];
        let ranked = rank_candidates(&vs, true, ".png");
        assert_eq!(digest_of(&ranked[0]), Some(gexpect));
    }

    #[test]
    fn single_platform_suffix_strip_examples() {
        assert_eq!(
            "bafkreiabc_windows".strip_suffix("_windows"),
            Some("bafkreiabc")
        );
        assert_eq!("Qmabc_mac".strip_suffix("_mac"), Some("Qmabc"));
    }
}
