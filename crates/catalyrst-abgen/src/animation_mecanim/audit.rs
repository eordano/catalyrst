use super::curves::{classify_constant, gather_clip_curves, CONST_CURVE_VALUE_TOL};
use super::ATTR_ROTATION;
use crate::animation::glb;

pub fn clip_partition_counts(glb_bytes: &[u8], tol: f32) -> Vec<(String, usize, usize)> {
    let g = glb::parse(glb_bytes);
    let buffers = std::slice::from_ref(&g.bin);
    let animations = match g.json["animations"].as_array() {
        Some(a) if !a.is_empty() => a.clone(),
        _ => return Vec::new(),
    };
    let (names, parent) = glb::node_names_and_parents(&g);
    let mut out = Vec::new();
    for anim in animations.iter() {
        let (ordered_bindings, scalar_curves) =
            gather_clip_curves(&g.json, buffers, anim, &names, &parent);
        let class = classify_constant(&scalar_curves, &ordered_bindings, tol);
        let nconst = class.iter().filter(|&&c| c).count();
        out.push((
            anim["name"].as_str().unwrap_or("").to_string(),
            scalar_curves.len() - nconst,
            nconst,
        ));
    }
    out
}

pub fn binding_tie_audit(
    glb_bytes: &[u8],
) -> Vec<(String, Vec<(String, i64, bool, usize, bool, u32, u32)>)> {
    let g = glb::parse(glb_bytes);
    let buffers = std::slice::from_ref(&g.bin);
    let animations = match g.json["animations"].as_array() {
        Some(a) if !a.is_empty() => a.clone(),
        _ => return Vec::new(),
    };
    let (names, parent) = glb::node_names_and_parents(&g);
    let mut out = Vec::new();
    for anim in animations.iter() {
        let (ordered_bindings, scalar_curves) =
            gather_clip_curves(&g.json, buffers, anim, &names, &parent);
        let class = classify_constant(&scalar_curves, &ordered_bindings, CONST_CURVE_VALUE_TOL);
        let mut rows = Vec::new();
        let mut ci = 0usize;
        for (path, attr, is_step) in ordered_bindings.iter() {
            let dim = if *attr == ATTR_ROTATION { 4 } else { 3 };
            let mut vmax = 0f32;
            let mut smax = 0f32;
            for i in ci..ci + dim {
                let keys = &scalar_curves[i].1;
                if let Some(k0) = keys.first() {
                    let v0 = k0.value as f32;
                    for k in keys.iter() {
                        vmax = vmax.max(((k.value as f32) - v0).abs());
                        smax = smax.max((k.slope as f32).abs());
                    }
                }
            }
            let our_collapse = (ci..ci + dim).all(|i| class[i]);
            rows.push((
                path.clone(),
                *attr,
                *is_step,
                dim,
                our_collapse,
                vmax.to_bits(),
                smax.to_bits(),
            ));
            ci += dim;
        }
        out.push((anim["name"].as_str().unwrap_or("").to_string(), rows));
    }
    out
}

pub fn binding_key_dump(
    glb_bytes: &[u8],
) -> Vec<(String, Vec<(String, i64, bool, Vec<Vec<f64>>)>)> {
    let g = glb::parse(glb_bytes);
    let buffers = std::slice::from_ref(&g.bin);
    let animations = match g.json["animations"].as_array() {
        Some(a) if !a.is_empty() => a.clone(),
        _ => return Vec::new(),
    };
    let (names, parent) = glb::node_names_and_parents(&g);
    let mut out = Vec::new();
    for anim in animations.iter() {
        let (ordered_bindings, scalar_curves) =
            gather_clip_curves(&g.json, buffers, anim, &names, &parent);
        let mut rows = Vec::new();
        let mut ci = 0usize;
        for (path, attr, is_step) in ordered_bindings.iter() {
            let dim = if *attr == ATTR_ROTATION { 4 } else { 3 };
            let mut comps = Vec::new();
            for i in ci..ci + dim {
                comps.push(scalar_curves[i].1.iter().map(|k| k.value).collect());
            }
            rows.push((path.clone(), *attr, *is_step, comps));
            ci += dim;
        }
        out.push((anim["name"].as_str().unwrap_or("").to_string(), rows));
    }
    out
}

pub fn binding_max_diffs(glb_bytes: &[u8]) -> Vec<(String, Vec<(String, i64, bool, usize, f32)>)> {
    let g = glb::parse(glb_bytes);
    let buffers = std::slice::from_ref(&g.bin);
    let animations = match g.json["animations"].as_array() {
        Some(a) if !a.is_empty() => a.clone(),
        _ => return Vec::new(),
    };
    let (names, parent) = glb::node_names_and_parents(&g);
    let mut out = Vec::new();
    for anim in animations.iter() {
        let (ordered_bindings, scalar_curves) =
            gather_clip_curves(&g.json, buffers, anim, &names, &parent);
        let mut rows = Vec::new();
        let mut ci = 0usize;
        for (path, attr, is_step) in ordered_bindings.iter() {
            let dim = if *attr == ATTR_ROTATION { 4 } else { 3 };
            let mut md = 0f32;
            for i in ci..ci + dim {
                let keys = &scalar_curves[i].1;
                if let Some(k0) = keys.first() {
                    let v0 = k0.value as f32;
                    for k in keys.iter() {
                        md = md.max(((k.value as f32) - v0).abs());
                    }
                }
            }
            rows.push((path.clone(), *attr, *is_step, dim, md));
            ci += dim;
        }
        out.push((anim["name"].as_str().unwrap_or("").to_string(), rows));
    }
    out
}
