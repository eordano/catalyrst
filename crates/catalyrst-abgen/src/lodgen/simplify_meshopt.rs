use anyhow::{anyhow, bail, Context, Result};
use std::path::Path;

use super::model::{LodModel, LodPrimitive};
use super::simplify::SimplifyReport;

// 1.0 = 100% of mesh extents: the budget target must dominate, mirroring the
// relaxed -se ceiling the gltfpack lane binary-searches toward. When the
// count-driven pass undershoots the [0.8*cap, cap] window the error bound is
// bisected down toward TIGHT_TARGET_ERROR, the same fill-back recipe as the
// gltfpack lane's -se search.
const LOOSE_TARGET_ERROR: f32 = 1.0;
const TIGHT_TARGET_ERROR: f32 = 0.01;

pub fn apportion(counts: &[usize], cap: u64) -> Vec<u64> {
    let total: u64 = counts.iter().map(|&c| c as u64).sum();
    if total <= cap {
        return counts.iter().map(|&c| c as u64).collect();
    }
    let mut shares: Vec<u64> = Vec::with_capacity(counts.len());
    let mut remainders: Vec<(u128, usize)> = Vec::with_capacity(counts.len());
    let mut assigned: u64 = 0;
    for (i, &c) in counts.iter().enumerate() {
        let num = c as u128 * cap as u128;
        let share = (num / total as u128) as u64;
        shares.push(share);
        assigned += share;
        remainders.push((num % total as u128, i));
    }
    remainders.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut left = cap - assigned;
    for &(_, i) in &remainders {
        if left == 0 {
            break;
        }
        shares[i] += 1;
        left -= 1;
    }
    shares
}

fn simplify_prim(
    prim: &LodPrimitive,
    target_tris: u64,
    target_error: f32,
) -> Result<(LodPrimitive, bool)> {
    let target_indices = (target_tris as usize).saturating_mul(3);
    if prim.indices.len() <= target_indices {
        return Ok((prim.clone(), false));
    }
    let bytes = meshopt::typed_to_bytes(&prim.positions);
    let adapter = meshopt::VertexDataAdapter::new(bytes, 12, 0)
        .map_err(|e| anyhow!("meshopt vertex adapter: {e}"))?;
    let mut indices = meshopt::simplify(
        &prim.indices,
        &adapter,
        target_indices,
        target_error,
        meshopt::SimplifyOptions::empty(),
        None,
    );
    let mut sloppy = false;
    if indices.len() > target_indices {
        let alt =
            meshopt::simplify_sloppy(&prim.indices, &adapter, target_indices, target_error, None);
        if alt.len() < indices.len() {
            indices = alt;
            sloppy = true;
        }
    }
    let mut out = LodPrimitive {
        positions: prim.positions.clone(),
        normals: prim.normals.clone(),
        uvs: prim.uvs.clone(),
        tangents: prim.tangents.clone(),
        colors: prim.colors.clone(),
        indices,
        material: prim.material,
    };
    out.compact_orphans();
    Ok((out, sloppy))
}

fn run_pass(
    model: &LodModel,
    budget: u64,
    target_error: f32,
) -> Result<(Vec<LodPrimitive>, usize, bool)> {
    let counts: Vec<usize> = model
        .primitives
        .iter()
        .map(|p| p.indices.len() / 3)
        .collect();
    let targets = apportion(&counts, budget);
    let mut primitives = Vec::with_capacity(model.primitives.len());
    let mut sloppy_any = false;
    let mut total = 0usize;
    for (prim, &target) in model.primitives.iter().zip(targets.iter()) {
        let (p, sloppy) = simplify_prim(prim, target, target_error)?;
        sloppy_any |= sloppy;
        if !p.indices.is_empty() {
            total += p.indices.len() / 3;
            primitives.push(p);
        }
    }
    Ok((primitives, total, sloppy_any))
}

pub fn simplify_model(
    model: &LodModel,
    target_tris: u64,
    enforce_cap: bool,
) -> Result<(LodModel, SimplifyReport)> {
    let tris_before = model.total_tris();
    let mut ratios_run: Vec<f64> = Vec::new();
    ratios_run.push(if tris_before == 0 {
        1.0
    } else {
        target_tris as f64 / tris_before as f64
    });
    let (mut prims, mut total, mut sloppy) = run_pass(model, target_tris, LOOSE_TARGET_ERROR)?;
    if enforce_cap && total as u64 > target_tris {
        bail!(
            "meshopt simplify missed the tri cap: {total} tris > cap {target_tris} \
             (topology-preserving and sloppy passes exhausted)"
        );
    }
    if enforce_cap {
        let floor = (target_tris as f64 * 0.8) as usize;
        let (mut lo, mut hi) = (TIGHT_TARGET_ERROR, LOOSE_TARGET_ERROR);
        for _ in 0..6 {
            if total >= floor {
                break;
            }
            let mid = (lo + hi) / 2.0;
            let (p, t, s) = run_pass(model, target_tris, mid)?;
            ratios_run.push(mid as f64);
            if t as u64 <= target_tris {
                hi = mid;
                if t > total {
                    prims = p;
                    total = t;
                    sloppy = s;
                }
            } else {
                lo = mid;
            }
        }
    }
    let out = LodModel {
        root_name: model.root_name.clone(),
        primitives: prims,
        materials: model.materials.clone(),
        images: model.images.clone(),
        log: Vec::new(),
    };
    Ok((
        out,
        SimplifyReport {
            tris_before,
            tris_after: total,
            ratios_run,
            aggressive_final: sloppy,
            passthrough: false,
            unsimplified: false,
        },
    ))
}

pub fn simplify_file(
    input: &Path,
    output: &Path,
    ratio: f64,
    tri_cap: Option<u64>,
) -> Result<SimplifyReport> {
    let bytes = std::fs::read(input).with_context(|| format!("read {}", input.display()))?;
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("lod")
        .to_string();
    let model = super::model::from_glb_bytes(&bytes, &stem)
        .with_context(|| format!("parse {}", input.display()))?;
    let tris = model.total_tris();
    let under_cap = tri_cap.is_none_or(|c| tris as u64 <= c);
    if ratio >= 1.0 && under_cap {
        if input != output {
            std::fs::copy(input, output)
                .with_context(|| format!("copy {} -> {}", input.display(), output.display()))?;
        }
        return Ok(SimplifyReport {
            tris_before: tris,
            tris_after: tris,
            passthrough: true,
            ..Default::default()
        });
    }
    let ratio_target = (tris as f64 * ratio.clamp(0.0, 1.0)).round() as u64;
    let (target, enforce) = match tri_cap {
        Some(cap) if (tris as u64) > cap => (cap, true),
        Some(cap) => (ratio_target.min(cap), true),
        None => (ratio_target, false),
    };
    let (out_model, report) = simplify_model(&model, target, enforce)?;
    let glb = super::emit::emit_glb(&out_model)?;
    std::fs::write(output, &glb).with_context(|| format!("write {}", output.display()))?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lodgen::emit::emit_glb;
    use crate::lodgen::model::{from_glb_bytes, AlphaClass, LodMaterial};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "abgen-lod-meshopt-test-{tag}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn grid_model(n: u32) -> LodModel {
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut uvs = Vec::new();
        for j in 0..=n {
            for i in 0..=n {
                let x = i as f32 / n as f32;
                let z = j as f32 / n as f32;
                let y = 0.05
                    * ((x * 12.0).sin() + (z * 12.0).cos())
                    * (1.0 + 0.3 * ((x * 5.0 + z * 7.0).sin()));
                positions.push([x * 10.0, y, z * 10.0]);
                normals.push([0.0, 1.0, 0.0]);
                uvs.push([x, z]);
            }
        }
        let mut indices = Vec::new();
        for j in 0..n {
            for i in 0..n {
                let a = j * (n + 1) + i;
                let b = a + 1;
                let c = a + n + 1;
                let d = c + 1;
                indices.extend_from_slice(&[a, c, b, b, c, d]);
            }
        }
        LodModel {
            root_name: "grid".to_string(),
            primitives: vec![LodPrimitive {
                positions,
                normals,
                uvs,
                indices,
                material: 0,
                ..Default::default()
            }],
            materials: vec![LodMaterial {
                name: "m".to_string(),
                class: AlphaClass::Opaque,
                base_color: [1.0, 1.0, 1.0, 1.0],
                cutoff: 0.5,
                image: None,
                double_sided: false,
            }],
            images: Vec::new(),
            log: Vec::new(),
        }
    }

    #[test]
    fn apportion_is_exact_and_proportional() {
        assert_eq!(apportion(&[800, 150, 50], 500), vec![400, 75, 25]);
        let shares = apportion(&[700, 200, 100], 501);
        assert_eq!(shares.iter().sum::<u64>(), 501);
        assert_eq!(shares, vec![351, 100, 50]);
        assert_eq!(apportion(&[100, 200], 400), vec![100, 200]);
        assert_eq!(apportion(&[0, 300], 100), vec![0, 100]);
        assert_eq!(apportion(&[3, 3, 3], 7), vec![3, 2, 2]);
        assert_eq!(apportion(&[], 100), Vec::<u64>::new());
    }

    #[test]
    fn capped_grid_lands_in_the_prod_window() {
        let model = grid_model(32);
        assert_eq!(model.total_tris(), 2048);
        let (out, report) = simplify_model(&model, 500, true).unwrap();
        assert_eq!(report.tris_before, 2048);
        assert_eq!(report.tris_after, out.total_tris());
        assert!(report.tris_after <= 500, "{}", report.tris_after);
        assert!(report.tris_after >= 400, "{}", report.tris_after);
        assert!(!report.passthrough);
        assert!(!report.unsimplified);
        assert_eq!(report.ratios_run.len(), 1);
        let orphans = out
            .primitives
            .iter()
            .map(|p| {
                let mut q = p.clone();
                q.compact_orphans()
            })
            .sum::<usize>();
        assert_eq!(orphans, 0);
    }

    #[test]
    fn simplify_is_deterministic_at_the_byte_level() {
        let model = grid_model(32);
        let (a, _) = simplify_model(&model, 500, true).unwrap();
        let (b, _) = simplify_model(&model, 500, true).unwrap();
        assert_eq!(emit_glb(&a).unwrap(), emit_glb(&b).unwrap());
    }

    #[test]
    fn zero_target_drops_the_primitive_via_sloppy_fallback() {
        let model = grid_model(8);
        let (out, report) = simplify_model(&model, 0, true).unwrap();
        assert_eq!(report.tris_after, 0);
        assert!(out.primitives.is_empty());
        assert_eq!(out.materials.len(), 1);
    }

    #[test]
    fn under_target_prims_pass_through_untouched() {
        let model = grid_model(8);
        let (out, report) = simplify_model(&model, 10_000, false).unwrap();
        assert_eq!(report.tris_before, report.tris_after);
        assert_eq!(out.primitives[0].indices, model.primitives[0].indices);
        assert!(!report.aggressive_final);
    }

    #[test]
    fn file_lane_passthrough_and_capped_window() {
        let dir = temp_dir("file");
        let glb = emit_glb(&grid_model(32)).unwrap();
        let input = dir.join("in.glb");
        let output = dir.join("out.glb");
        std::fs::write(&input, &glb).unwrap();

        let report = simplify_file(&input, &output, 1.0, Some(1_000_000)).unwrap();
        assert!(report.passthrough);
        assert_eq!(report.tris_before, 2048);
        assert_eq!(std::fs::read(&output).unwrap(), glb);

        let report = simplify_file(&input, &output, 1.0, Some(500)).unwrap();
        assert!(!report.passthrough);
        assert!(report.tris_after <= 500 && report.tris_after >= 400);
        let back = from_glb_bytes(&std::fs::read(&output).unwrap(), "grid").unwrap();
        assert_eq!(back.total_tris(), report.tris_after);

        let report = simplify_file(&input, &output, 0.25, None).unwrap();
        assert!(report.tris_after <= 512 && report.tris_after >= 400);
    }
}
