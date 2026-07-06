use crate::dedup::{
    reconcile_divergent, record_claim, variant_key_for, FirstWritten, ReconcileStats,
};
use crate::{BundleSpec, ContentItem, EffectiveToggles, EntityEntry};
use abgen::builder::{build_bundle_multi, BuildOpts};
use abgen::local_store::LocalContentStore;
use abgen::{naming, Result};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

/// Same-directory sibling temp path for atomic bundle publication: stage to
/// `<bundle_name>.tmp.<pid>.<seq>` then rename over the final name, so
/// concurrent shard processes sharing one out_root never observe a torn
/// bundle (rename replaces atomically on the same fs). pid + a per-process
/// sequence keeps temp names collision-proof even when two threads in one
/// process race on the same bundle_name; skip-existing and the manifest
/// is_file partition match exact bundle names, so `.tmp.*` files are never
/// read as bundles.
pub(crate) fn bundle_tmp_path(out_path: &Path) -> PathBuf {
    static TMP_SEQ: AtomicUsize = AtomicUsize::new(0);
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let name = out_path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    out_path.with_file_name(format!("{name}.tmp.{}.{seq}", std::process::id()))
}

fn write_bundle_atomic(out_path: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = bundle_tmp_path(out_path);
    std::fs::write(&tmp, data)
        .and_then(|_| std::fs::rename(&tmp, out_path))
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })
}

pub(crate) fn build_one(
    store: &LocalContentStore,
    content_by_file: &HashMap<String, String>,
    spec: &BundleSpec,
    out_path: &std::path::Path,
    toggles: EffectiveToggles,
) -> Result<()> {
    build_group(store, content_by_file, &[spec], &[out_path], toggles)
}

/// One fetch + one build for a group of platform-sibling specs (same cid,
/// bundle names differing only in the "_<platform>" suffix); BuildOpts come
/// from specs[0] and build_bundle_multi retargets the rest — sound because
/// sibling metadata_deps are exactly the suffix swap the retarget applies.
/// A single-element group is byte-for-byte the old single-platform path.
fn build_group(
    store: &LocalContentStore,
    content_by_file: &HashMap<String, String>,
    specs: &[&BundleSpec],
    out_paths: &[&Path],
    toggles: EffectiveToggles,
) -> Result<()> {
    let spec = specs[0];
    let glb = store.fetch_mmap(&spec.cid)?;
    let effective_source = spec
        .source_file
        .clone()
        .unwrap_or_else(|| format!("{}.glb", spec.cid));
    let sf_for_bytes = effective_source.clone();
    let resolve_fn = |uri: &str| -> Option<Vec<u8>> {
        let key = naming::resolve_uri_to_content_file(uri, &sf_for_bytes)
            .ok()?
            .to_lowercase();
        let h = content_by_file.get(&key)?;
        store.fetch(h).ok()
    };
    let resolve: abgen::gltf::Resolve = if !content_by_file.is_empty() {
        Some(&resolve_fn)
    } else {
        None
    };
    let sf_for_hash = effective_source.clone();
    let resolve_hash_fn = |uri: &str| -> Option<String> {
        let key = naming::resolve_uri_to_content_file(uri, &sf_for_hash)
            .ok()?
            .to_lowercase();
        content_by_file.get(&key).cloned()
    };
    let resolve_hash: Option<abgen::builder::ResolveHash> =
        if !content_by_file.is_empty() && spec.source_file.is_some() {
            Some(&resolve_hash_fn)
        } else {
            None
        };
    let opts = BuildOpts {
        keep_forward_plus: true,
        source_file: Some(&effective_source),
        entity_type: spec.entity_type.as_deref(),
        resolve,
        resolve_hash,
        model_referenced: spec.model_referenced,
        metadata_dependencies: &spec.metadata_deps,
        expect_hash: spec.expect_hash.as_deref(),
        standalone_color_space: spec.standalone_color_space,
        standalone_normal: spec.standalone_normal,
        force_default_material: spec.force_default_material,
        magenta_missing: toggles.magenta_missing,
        collection_mode: toggles.collection_mode,
        real_textures: toggles.real_textures,
        v38_compat: toggles.v38_compat,
        v38_timestamp: toggles.v38_timestamp,
        lod: None,
    };
    let names: Vec<String> = specs.iter().map(|s| s.bundle_name.clone()).collect();
    let artifacts = build_bundle_multi(&glb[..], &names, &spec.cid, &opts)?;
    for (artifact, out_path) in artifacts.iter().zip(out_paths) {
        write_bundle_atomic(out_path, &artifact.data)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn write_cdn_manifest(
    out_root: &Path,
    entity_id: &str,
    platform: &str,
    bundles: &[BundleSpec],
    ab_version: &str,
    content_server_url: &str,
    date: &str,
) -> Result<usize> {
    let mut names: Vec<String> = bundles.iter().map(|b| b.bundle_name.clone()).collect();
    names.sort();
    names.dedup();
    let bundle_dir = out_root.join(entity_id).join(platform);
    let (built, missing): (Vec<String>, Vec<String>) = names
        .into_iter()
        .partition(|n| bundle_dir.join(n).is_file());
    if built.is_empty() && !missing.is_empty() {
        return Ok(missing.len());
    }

    abgen::manifest::write_corpus_manifest(
        out_root,
        entity_id,
        platform,
        &built,
        ab_version,
        content_server_url,
        abgen::manifest::exit_code_for_failures(missing.len()),
        date,
    )?;
    Ok(missing.len())
}

pub(crate) struct BuildOutcome {
    pub(crate) built: usize,
    pub(crate) errs: usize,
    pub(crate) skipped: usize,
    pub(crate) manifest_errs: usize,
    pub(crate) manifest_incomplete: usize,
    pub(crate) reconcile: ReconcileStats,
}

pub(crate) struct BuildCounters<'a> {
    pub(crate) built: &'a AtomicUsize,
    pub(crate) errs: &'a AtomicUsize,
    pub(crate) skipped: &'a AtomicUsize,
}

/// Build (or skip / hardlink-dedup) one bundle into `ent_out`. The single
/// per-bundle build path shared by the phased build loop and the fused
/// streaming path, so both route through identical skip-existing, cdn
/// hardlink-dedup, and build_one + panic-guard logic. `first_written` is the
/// cross-entity dedup map for cdn-layout: first-writer-wins hardlinking is
/// unchanged during the pass, but every claimant (built, hardlinked, or
/// skipped) also records its variant key + entity id so reconcile_divergent
/// can elect the canonical winner afterwards. Pass None for flat/default
/// layouts (which don't dedup by bundle name).
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_bundle_at(
    store: &LocalContentStore,
    content_by_file: &HashMap<String, String>,
    spec: &BundleSpec,
    ent_out: &Path,
    entity_id: &str,
    toggles: EffectiveToggles,
    skip_existing: bool,
    force: bool,
    first_written: Option<&FirstWritten>,
    c: &BuildCounters,
) {
    let out_path = ent_out.join(&spec.bundle_name);
    let vkey = first_written.and_then(|_| variant_key_for(store, content_by_file, spec, toggles));
    if skip_existing && !force {
        if let Ok(m) = std::fs::metadata(&out_path) {
            if m.is_file() && m.len() > 0 {
                c.skipped.fetch_add(1, Ordering::Relaxed);
                if let Some(fw) = first_written {
                    record_claim(fw, &spec.bundle_name, &out_path, vkey.as_ref(), entity_id);
                }
                return;
            }
        }
    }
    if let Some(fw) = first_written {
        let prior = fw
            .lock()
            .unwrap()
            .get(&spec.bundle_name)
            .map(|e| e.path.clone());
        if let Some(src) = prior {
            let tmp = bundle_tmp_path(&out_path);
            let staged =
                std::fs::hard_link(&src, &tmp).is_ok() || std::fs::copy(&src, &tmp).is_ok();
            let linked = staged && std::fs::rename(&tmp, &out_path).is_ok();
            if linked {
                c.built.fetch_add(1, Ordering::Relaxed);
                record_claim(fw, &spec.bundle_name, &out_path, vkey.as_ref(), entity_id);
            } else {
                let _ = std::fs::remove_file(&tmp);
                c.errs.fetch_add(1, Ordering::Relaxed);
                eprintln!("link {}/{}: failed", entity_id, spec.bundle_name);
            }
            return;
        }
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        build_one(store, content_by_file, spec, &out_path, toggles)
    }));
    match result {
        Ok(Ok(_)) => {
            if let Some(fw) = first_written {
                record_claim(fw, &spec.bundle_name, &out_path, vkey.as_ref(), entity_id);
            }
            c.built.fetch_add(1, Ordering::Relaxed);
        }
        Ok(Err(e)) => {
            c.errs.fetch_add(1, Ordering::Relaxed);
            eprintln!("err {}/{}: {e}", entity_id, spec.bundle_name);
        }
        Err(_) => {
            c.errs.fetch_add(1, Ordering::Relaxed);
            eprintln!("panic {}/{} (skipped)", entity_id, spec.bundle_name);
        }
    }
}

/// Multi-platform sibling of build_bundle_at: one slot per platform, each
/// with its own spec (suffix-swapped names), out dir, and first_written map
/// (per-platform namespaces keep S1's winner election independent). Skip-
/// existing and hardlink-dedup resolve per slot; whatever remains builds as
/// ONE encode-once group (build_group -> build_bundle_multi), so a pair with
/// one platform already on disk still builds exactly once and writes only
/// the missing file.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_bundle_multi_at(
    store: &LocalContentStore,
    content_by_file: &HashMap<String, String>,
    specs: &[&BundleSpec],
    ent_outs: &[PathBuf],
    entity_id: &str,
    toggles: EffectiveToggles,
    skip_existing: bool,
    force: bool,
    first_written: &[FirstWritten],
    c: &BuildCounters,
) {
    let vkey = variant_key_for(store, content_by_file, specs[0], toggles);
    let mut pending: Vec<usize> = Vec::new();
    for (i, spec) in specs.iter().enumerate() {
        let out_path = ent_outs[i].join(&spec.bundle_name);
        if skip_existing && !force {
            if let Ok(m) = std::fs::metadata(&out_path) {
                if m.is_file() && m.len() > 0 {
                    c.skipped.fetch_add(1, Ordering::Relaxed);
                    record_claim(
                        &first_written[i],
                        &spec.bundle_name,
                        &out_path,
                        vkey.as_ref(),
                        entity_id,
                    );
                    continue;
                }
            }
        }
        let prior = first_written[i]
            .lock()
            .unwrap()
            .get(&spec.bundle_name)
            .map(|e| e.path.clone());
        if let Some(src) = prior {
            let tmp = bundle_tmp_path(&out_path);
            let staged =
                std::fs::hard_link(&src, &tmp).is_ok() || std::fs::copy(&src, &tmp).is_ok();
            let linked = staged && std::fs::rename(&tmp, &out_path).is_ok();
            if linked {
                c.built.fetch_add(1, Ordering::Relaxed);
                record_claim(
                    &first_written[i],
                    &spec.bundle_name,
                    &out_path,
                    vkey.as_ref(),
                    entity_id,
                );
            } else {
                let _ = std::fs::remove_file(&tmp);
                c.errs.fetch_add(1, Ordering::Relaxed);
                eprintln!("link {}/{}: failed", entity_id, spec.bundle_name);
            }
            continue;
        }
        pending.push(i);
    }
    if pending.is_empty() {
        return;
    }
    let group_specs: Vec<&BundleSpec> = pending.iter().map(|&i| specs[i]).collect();
    let out_paths: Vec<PathBuf> = pending
        .iter()
        .map(|&i| ent_outs[i].join(&specs[i].bundle_name))
        .collect();
    let out_path_refs: Vec<&Path> = out_paths.iter().map(|p| p.as_path()).collect();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        build_group(
            store,
            content_by_file,
            &group_specs,
            &out_path_refs,
            toggles,
        )
    }));
    match result {
        Ok(Ok(_)) => {
            for (&i, out_path) in pending.iter().zip(out_paths.iter()) {
                record_claim(
                    &first_written[i],
                    &specs[i].bundle_name,
                    out_path,
                    vkey.as_ref(),
                    entity_id,
                );
                c.built.fetch_add(1, Ordering::Relaxed);
            }
        }
        Ok(Err(e)) => {
            c.errs.fetch_add(pending.len(), Ordering::Relaxed);
            eprintln!("err {}/{}: {e}", entity_id, group_specs[0].bundle_name);
        }
        Err(_) => {
            c.errs.fetch_add(pending.len(), Ordering::Relaxed);
            eprintln!(
                "panic {}/{} (skipped)",
                entity_id, group_specs[0].bundle_name
            );
        }
    }
}

/// Materialize the sibling-platform BundleSpec for a derived primary spec:
/// identical encode parameters, "_<primary>" name suffixes swapped to the
/// sibling platform (strip_suffix, never substring replace) on bundle_name
/// and metadata_deps — exactly what derive_one_entity(sibling) would emit.
fn sibling_spec(spec: &BundleSpec, primary: &str, platform: &str) -> BundleSpec {
    let old_suffix = format!("_{primary}");
    let swap = |s: &str| -> String {
        s.strip_suffix(old_suffix.as_str())
            .map(|stem| format!("{stem}_{platform}"))
            .unwrap_or_else(|| s.to_string())
    };
    BundleSpec {
        bundle_name: swap(&spec.bundle_name),
        metadata_deps: spec.metadata_deps.iter().map(|d| swap(d)).collect(),
        ..spec.clone()
    }
}

pub(crate) fn derive_one_entity(
    store: &LocalContentStore,
    ent_id: &str,
    platform: &str,
    uri_cache: &abgen::glbscan::UriCache,
) -> Option<EntityEntry> {
    let entity = load_entity_json(store, ent_id)?;
    let entity_type = entity
        .get("type")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let content_items: Vec<ContentItem> = entity
        .get("content")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| {
                    let f = c.get("file").and_then(|v| v.as_str())?.to_string();
                    let h = c.get("hash").and_then(|v| v.as_str())?.to_string();
                    Some(ContentItem { file: f, hash: h })
                })
                .collect()
        })
        .unwrap_or_default();
    if content_items.is_empty() {
        return None;
    }
    let content_by_file: HashMap<String, String> = content_items
        .iter()
        .map(|c| (c.file.to_lowercase(), c.hash.clone()))
        .collect();
    let scan = abgen::glbscan::scan_entity(store, &content_by_file, uri_cache);
    let (model_refs, linear_refs, normal_refs) =
        (&scan.model_refs, &scan.linear_refs, &scan.normal_refs);

    let mut bundles: Vec<BundleSpec> = Vec::new();
    let mut local_seen: HashSet<String> = HashSet::new();
    for c in &content_items {
        let fl = c.file.to_lowercase();
        let is_glb = fl.ends_with(".glb") || fl.ends_with(".gltf");
        let is_image = IMAGE_EXTS.iter().any(|e| fl.ends_with(e));
        if !is_glb && !is_image {
            continue;
        }
        if !store.exists(&c.hash) {
            continue;
        }
        let bundle_name = format!("{}_{platform}", c.hash);
        if !local_seen.insert(bundle_name.clone()) {
            continue;
        }
        let m_deps = if is_glb {
            scan.metadata_deps(store, &c.file, &c.hash, &content_by_file, platform)
        } else {
            Vec::new()
        };
        let model_ref = is_image && model_refs.contains(&c.hash);
        let standalone_color_space = if is_image {
            Some(if linear_refs.contains(&c.hash) { 0 } else { 1 })
        } else {
            None
        };
        let standalone_normal = is_image && normal_refs.contains(&c.hash);
        bundles.push(BundleSpec {
            cid: c.hash.clone(),
            bundle_name,
            source_file: Some(c.file.clone()),
            entity_type: entity_type.clone(),
            metadata_deps: m_deps,
            model_referenced: model_ref,
            expect_hash: None,
            standalone_color_space,
            standalone_normal,
            force_default_material: false,
        });
    }
    if bundles.is_empty() {
        return None;
    }
    Some(EntityEntry {
        entity_id: ent_id.to_string(),
        content: content_items,
        bundles,
    })
}

/// Fused derive+build+manifest streaming pass for the --entity-ids --cdn-layout
/// path: each entity flows scan -> build -> per-entity manifest inside one
/// par_iter, so the GPU/encode work of early entities overlaps the scan of
/// later ones (the phased path derives ALL manifests before building ANY
/// bundle, leaving the GPU idle through the whole derive phase). Bundles build
/// at bundle granularity via a nested par_iter on the shared global pool:
/// a many-bundle entity no longer serializes its bundles on one worker (the
/// catalog max is ~4.8k bundles ~= a multi-minute serial chain) — idle workers
/// steal its bundle tasks across entity boundaries, and the entity task blocks
/// (work-stealing while it waits) until its own bundles drain, so the manifest
/// is still written only after every one of its bundles is on disk. Bundle
/// bytes and manifests are identical to the phased path: keep_shared_bundles
/// is true under cdn-layout (no cross-entity dedup in derive), cross-entity
/// bundle dedup is the same first_written claim map the phased build uses
/// (winner election in reconcile_divergent is order-independent, so bundle
/// reordering cannot change post-reconcile bytes), and each entity's manifest
/// is written only after its own bundles are on disk.
///
/// With a two-platform list (windows,mac) the pass derives each entity ONCE
/// (sibling specs are the "_<platform>" suffix swap of the primary derive),
/// builds/encodes each bundle once and serializes it per platform inside the
/// same bundle task, keeps one first_written map + one reconcile pass per
/// platform, and writes one manifest per platform. Counters count
/// platform-files (a fresh pair increments built twice) so campaign totals
/// stay comparable to two single-platform runs.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_fused_entity_ids(
    ids: &[String],
    store: &LocalContentStore,
    out_root: &Path,
    platforms: &[String],
    toggles: EffectiveToggles,
    ab_version: &str,
    content_server_url: &str,
    skip_existing: bool,
    force: bool,
) -> BuildOutcome {
    let uri_cache = abgen::glbscan::UriCache::new();
    let first_written: Vec<FirstWritten> = platforms
        .iter()
        .map(|_| Mutex::new(HashMap::new()))
        .collect();
    let built = AtomicUsize::new(0);
    let errs = AtomicUsize::new(0);
    let skipped = AtomicUsize::new(0);
    let manifest_errs = AtomicUsize::new(0);
    let manifest_incomplete = AtomicUsize::new(0);
    let processed = AtomicUsize::new(0);
    let build_date = abgen::live::build_scoped_date();
    let n_total = ids.len();
    let t0 = Instant::now();
    let counters = BuildCounters {
        built: &built,
        errs: &errs,
        skipped: &skipped,
    };
    let primary = &platforms[0];

    ids.par_iter().for_each(|ent_id| {
        let done = processed.fetch_add(1, Ordering::Relaxed) + 1;
        if done.is_multiple_of(5000) {
            let secs = t0.elapsed().as_secs_f64().max(0.001);
            eprintln!(
                "  fused: {done}/{n_total} entities ({:.0}/s, {:.0}s) | built={} skipped={} errs={}",
                done as f64 / secs,
                secs,
                built.load(Ordering::Relaxed),
                skipped.load(Ordering::Relaxed),
                errs.load(Ordering::Relaxed),
            );
        }
        let entry = match derive_one_entity(store, ent_id, primary, &uri_cache) {
            Some(e) => e,
            None => return,
        };
        let EntityEntry {
            entity_id,
            content,
            bundles,
        } = entry;
        let content_by_file: HashMap<String, String> = content
            .iter()
            .map(|c| (c.file.to_lowercase(), c.hash.clone()))
            .collect();
        let mut plat_bundles: Vec<Vec<BundleSpec>> = Vec::with_capacity(platforms.len());
        plat_bundles.push(bundles);
        for plat in &platforms[1..] {
            plat_bundles.push(
                plat_bundles[0]
                    .iter()
                    .map(|s| sibling_spec(s, primary, plat))
                    .collect(),
            );
        }
        let ent_outs: Vec<PathBuf> = platforms
            .iter()
            .map(|p| out_root.join(&entity_id).join(p))
            .collect();
        for ent_out in &ent_outs {
            if let Err(e) = std::fs::create_dir_all(ent_out) {
                eprintln!("mkdir {}: {e}", ent_out.display());
                errs.fetch_add(
                    plat_bundles.iter().map(|b| b.len()).sum::<usize>(),
                    Ordering::Relaxed,
                );
                return;
            }
        }
        let n_bundles = plat_bundles[0].len();
        (0..n_bundles).into_par_iter().for_each(|bi| {
            if platforms.len() == 1 {
                build_bundle_at(
                    store,
                    &content_by_file,
                    &plat_bundles[0][bi],
                    &ent_outs[0],
                    &entity_id,
                    toggles,
                    skip_existing,
                    force,
                    Some(&first_written[0]),
                    &counters,
                );
            } else {
                let specs: Vec<&BundleSpec> = plat_bundles.iter().map(|l| &l[bi]).collect();
                build_bundle_multi_at(
                    store,
                    &content_by_file,
                    &specs,
                    &ent_outs,
                    &entity_id,
                    toggles,
                    skip_existing,
                    force,
                    &first_written,
                    &counters,
                );
            }
        });
        for (pi, plat) in platforms.iter().enumerate() {
            match write_cdn_manifest(
                out_root,
                &entity_id,
                plat,
                &plat_bundles[pi],
                ab_version,
                content_server_url,
                &build_date,
            ) {
                Err(e) => {
                    manifest_errs.fetch_add(1, Ordering::Relaxed);
                    eprintln!("manifest {entity_id}: {e}");
                }
                Ok(0) => {}
                Ok(n) => {
                    manifest_incomplete.fetch_add(1, Ordering::Relaxed);
                    eprintln!("manifest {entity_id}: {n} failed bundle(s) omitted");
                }
            }
        }
    });

    let mut reconcile = ReconcileStats::default();
    for (fw, plat) in first_written.into_iter().zip(platforms.iter()) {
        let rs = reconcile_divergent(store, out_root, plat, toggles, fw);
        if platforms.len() > 1 {
            eprintln!(
                "reconcile[{plat}]: divergent={} rebuilt={} relinked={} errs={}",
                rs.divergent, rs.rebuilt, rs.relinked, rs.errs
            );
        }
        reconcile.divergent += rs.divergent;
        reconcile.rebuilt += rs.rebuilt;
        reconcile.relinked += rs.relinked;
        reconcile.errs += rs.errs;
    }

    BuildOutcome {
        built: built.into_inner(),
        errs: errs.into_inner(),
        skipped: skipped.into_inner(),
        manifest_errs: manifest_errs.into_inner(),
        manifest_incomplete: manifest_incomplete.into_inner(),
        reconcile,
    }
}

#[allow(dead_code)]
fn iso8601_utc_now() -> String {
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let total_secs = dur.as_secs();
    let millis = dur.subsec_millis();
    let days = (total_secs / 86_400) as i64;
    let secs_of_day = (total_secs % 86_400) as i64;
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let second = secs_of_day % 60;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

const fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

pub(crate) const IMAGE_EXTS: [&str; 3] = [".png", ".jpg", ".jpeg"];

pub(crate) fn load_entity_json(store: &LocalContentStore, cid: &str) -> Option<serde_json::Value> {
    let bytes = store.fetch(cid).ok()?;
    serde_json::from_slice(&bytes).ok()
}
