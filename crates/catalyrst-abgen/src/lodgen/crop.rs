use crate::lodgen::model::{LodModel, LodPrimitive};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default)]
pub struct CropReport {
    pub rects: usize,
    pub tris_in: usize,
    pub tris_out: usize,
    pub tris_clipped: usize,
    pub tris_dropped: usize,
    pub prims_dropped: usize,
    pub verts_dropped: usize,
}

impl CropReport {
    pub fn summary(&self) -> String {
        format!(
            "rects={} tris_in={} tris_out={} tris_clipped={} tris_dropped={} prims_dropped={} verts_dropped={}",
            self.rects,
            self.tris_in,
            self.tris_out,
            self.tris_clipped,
            self.tris_dropped,
            self.prims_dropped,
            self.verts_dropped
        )
    }
}

pub fn crop_rect_rh(base: (i32, i32), parcels: &[(i32, i32)]) -> [f64; 4] {
    let plane = crate::lods::plane_clipping(parcels);
    let bx = base.0 as f64 * 16.0;
    let bz = base.1 as f64 * 16.0;
    [
        -(plane[1] - bx),
        -(plane[0] - bx),
        plane[2] - bz,
        plane[3] - bz,
    ]
}

fn cell_rects(parcels: &[(i32, i32)]) -> Vec<(i32, i32, i32, i32)> {
    let mut rows: BTreeMap<i32, Vec<i32>> = BTreeMap::new();
    for &(x, y) in parcels {
        rows.entry(y).or_default().push(x);
    }
    let mut rects: Vec<(i32, i32, i32, i32)> = Vec::new();
    for (y, xs) in rows.iter_mut() {
        xs.sort_unstable();
        xs.dedup();
        let mut i = 0;
        while i < xs.len() {
            let x0 = xs[i];
            let mut x1 = x0;
            while i + 1 < xs.len() && xs[i + 1] == x1 + 1 {
                i += 1;
                x1 = xs[i];
            }
            match rects
                .iter_mut()
                .find(|r| r.0 == x0 && r.1 == x1 && r.3 + 1 == *y)
            {
                Some(r) => r.3 = *y,
                None => rects.push((x0, x1, *y, *y)),
            }
            i += 1;
        }
    }
    rects
}

pub fn crop_rects_rh(base: (i32, i32), parcels: &[(i32, i32)]) -> Vec<[f64; 4]> {
    let bx = base.0 as f64 * 16.0;
    let bz = base.1 as f64 * 16.0;
    cell_rects(parcels)
        .into_iter()
        .map(|(x0, x1, y0, y1)| {
            let wx0 = x0 as f64 * 16.0 - 0.05;
            let wx1 = (x1 + 1) as f64 * 16.0 + 0.05;
            let wz0 = y0 as f64 * 16.0 - 0.05;
            let wz1 = (y1 + 1) as f64 * 16.0 + 0.05;
            [-(wx1 - bx), -(wx0 - bx), wz0 - bz, wz1 - bz]
        })
        .collect()
}

const DEFAULT_NORMAL: [f32; 3] = [0.0, 0.0, 1.0];
const DEFAULT_TANGENT: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const DEFAULT_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

#[derive(Clone, Copy)]
struct ClipVert {
    pos: [f64; 3],
    normal: [f64; 3],
    uv: [f64; 2],
    tangent: [f64; 4],
    color: [f64; 4],
}

fn clip_vert(prim: &LodPrimitive, i: usize) -> ClipVert {
    let p = prim.positions[i];
    let n = prim.normals.get(i).copied().unwrap_or(DEFAULT_NORMAL);
    let uv = prim.uvs.get(i).copied().unwrap_or([0.0, 0.0]);
    let t = prim.tangents.get(i).copied().unwrap_or(DEFAULT_TANGENT);
    let c = prim.colors.get(i).copied().unwrap_or(DEFAULT_COLOR);
    ClipVert {
        pos: [p[0] as f64, p[1] as f64, p[2] as f64],
        normal: [n[0] as f64, n[1] as f64, n[2] as f64],
        uv: [uv[0] as f64, uv[1] as f64],
        tangent: [t[0] as f64, t[1] as f64, t[2] as f64, t[3] as f64],
        color: [c[0] as f64, c[1] as f64, c[2] as f64, c[3] as f64],
    }
}

fn plane_dist(v: &ClipVert, plane: usize, rect: &[f64; 4]) -> f64 {
    match plane {
        0 => v.pos[0] - rect[0],
        1 => rect[1] - v.pos[0],
        2 => v.pos[2] - rect[2],
        _ => rect[3] - v.pos[2],
    }
}

fn lerp_vert(a: &ClipVert, b: &ClipVert, t: f64) -> ClipVert {
    let mut pos = [0.0; 3];
    let mut n = [0.0; 3];
    let mut tan = [0.0; 3];
    for i in 0..3 {
        pos[i] = a.pos[i] + (b.pos[i] - a.pos[i]) * t;
        n[i] = a.normal[i] + (b.normal[i] - a.normal[i]) * t;
        tan[i] = a.tangent[i] + (b.tangent[i] - a.tangent[i]) * t;
    }
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    let normal = if len > 1e-12 {
        [n[0] / len, n[1] / len, n[2] / len]
    } else {
        a.normal
    };
    let tlen = (tan[0] * tan[0] + tan[1] * tan[1] + tan[2] * tan[2]).sqrt();
    let tangent = if tlen > 1e-12 {
        [tan[0] / tlen, tan[1] / tlen, tan[2] / tlen, a.tangent[3]]
    } else {
        a.tangent
    };
    let mut color = [0.0; 4];
    for i in 0..4 {
        color[i] = a.color[i] + (b.color[i] - a.color[i]) * t;
    }
    ClipVert {
        pos,
        normal,
        uv: [
            a.uv[0] + (b.uv[0] - a.uv[0]) * t,
            a.uv[1] + (b.uv[1] - a.uv[1]) * t,
        ],
        tangent,
        color,
    }
}

fn clip_poly(poly: &mut Vec<ClipVert>, scratch: &mut Vec<ClipVert>, rect: &[f64; 4]) {
    for plane in 0..4 {
        if poly.is_empty() {
            return;
        }
        scratch.clear();
        let n = poly.len();
        for i in 0..n {
            let cur = poly[i];
            let prev = poly[(i + n - 1) % n];
            let dc = plane_dist(&cur, plane, rect);
            let dp = plane_dist(&prev, plane, rect);
            let cur_in = dc >= 0.0;
            let prev_in = dp >= 0.0;
            if cur_in {
                if !prev_in {
                    scratch.push(lerp_vert(&prev, &cur, dp / (dp - dc)));
                }
                scratch.push(cur);
            } else if prev_in {
                scratch.push(lerp_vert(&prev, &cur, dp / (dp - dc)));
            }
        }
        std::mem::swap(poly, scratch);
    }
}

fn tri_area(a: &ClipVert, b: &ClipVert, c: &ClipVert) -> f64 {
    let u = [
        b.pos[0] - a.pos[0],
        b.pos[1] - a.pos[1],
        b.pos[2] - a.pos[2],
    ];
    let v = [
        c.pos[0] - a.pos[0],
        c.pos[1] - a.pos[1],
        c.pos[2] - a.pos[2],
    ];
    let x = u[1] * v[2] - u[2] * v[1];
    let y = u[2] * v[0] - u[0] * v[2];
    let z = u[0] * v[1] - u[1] * v[0];
    0.5 * (x * x + y * y + z * z).sqrt()
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnionStats {
    pub rects: usize,
    pub buffer_verts: usize,
    pub referenced_verts: usize,
    pub outside: usize,
}

impl UnionStats {
    pub fn outside_fraction(&self) -> f64 {
        if self.referenced_verts == 0 {
            0.0
        } else {
            self.outside as f64 / self.referenced_verts as f64
        }
    }
}

pub fn union_stats(model: &LodModel, rects: &[[f64; 4]], eps: f64) -> UnionStats {
    let mut stats = UnionStats {
        rects: rects.len(),
        ..Default::default()
    };
    for prim in &model.primitives {
        stats.buffer_verts += prim.positions.len();
        let mut used = vec![false; prim.positions.len()];
        for &i in &prim.indices {
            if let Some(u) = used.get_mut(i as usize) {
                *u = true;
            }
        }
        for (i, &u) in used.iter().enumerate() {
            if !u {
                continue;
            }
            stats.referenced_verts += 1;
            let p = prim.positions[i];
            let x = p[0] as f64;
            let z = p[2] as f64;
            let inside = rects
                .iter()
                .any(|r| x >= r[0] - eps && x <= r[1] + eps && z >= r[2] - eps && z <= r[3] + eps);
            if !inside {
                stats.outside += 1;
            }
        }
    }
    stats
}

struct OutBuffers {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    tangents: Vec<[f32; 4]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
    carry_tangents: bool,
    carry_colors: bool,
}

impl OutBuffers {
    fn push_tri(&mut self, verts: [ClipVert; 3]) {
        let base = self.positions.len() as u32;
        for v in verts {
            self.positions
                .push([v.pos[0] as f32, v.pos[1] as f32, v.pos[2] as f32]);
            self.normals
                .push([v.normal[0] as f32, v.normal[1] as f32, v.normal[2] as f32]);
            self.uvs.push([v.uv[0] as f32, v.uv[1] as f32]);
            if self.carry_tangents {
                self.tangents.push([
                    v.tangent[0] as f32,
                    v.tangent[1] as f32,
                    v.tangent[2] as f32,
                    v.tangent[3] as f32,
                ]);
            }
            if self.carry_colors {
                self.colors.push([
                    v.color[0] as f32,
                    v.color[1] as f32,
                    v.color[2] as f32,
                    v.color[3] as f32,
                ]);
            }
        }
        self.indices.extend_from_slice(&[base, base + 1, base + 2]);
    }

    fn push_copy(&mut self, prim: &LodPrimitive, idx: [usize; 3]) {
        let base = self.positions.len() as u32;
        for &i in &idx {
            self.positions.push(prim.positions[i]);
            self.normals
                .push(prim.normals.get(i).copied().unwrap_or(DEFAULT_NORMAL));
            self.uvs
                .push(prim.uvs.get(i).copied().unwrap_or([0.0, 0.0]));
            if self.carry_tangents {
                self.tangents
                    .push(prim.tangents.get(i).copied().unwrap_or(DEFAULT_TANGENT));
            }
            if self.carry_colors {
                self.colors
                    .push(prim.colors.get(i).copied().unwrap_or(DEFAULT_COLOR));
            }
        }
        self.indices.extend_from_slice(&[base, base + 1, base + 2]);
    }
}

pub fn crop(model: &mut LodModel, rects: &[[f64; 4]]) -> CropReport {
    let mut report = CropReport {
        rects: rects.len(),
        ..Default::default()
    };
    for prim in &mut model.primitives {
        report.tris_in += prim.indices.len() / 3;
        let mut changed = false;
        let mut out = OutBuffers {
            positions: Vec::new(),
            normals: Vec::new(),
            uvs: Vec::new(),
            tangents: Vec::new(),
            colors: Vec::new(),
            indices: Vec::new(),
            carry_tangents: !prim.tangents.is_empty(),
            carry_colors: !prim.colors.is_empty(),
        };
        let mut tri_poly: Vec<ClipVert> = Vec::new();
        let mut poly: Vec<ClipVert> = Vec::new();
        let mut scratch: Vec<ClipVert> = Vec::new();
        for tri in prim.indices.chunks_exact(3) {
            let idx = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
            let inside = |i: usize, r: &[f64; 4]| {
                let p = prim.positions[i];
                let x = p[0] as f64;
                let z = p[2] as f64;
                x >= r[0] && x <= r[1] && z >= r[2] && z <= r[3]
            };
            if rects.iter().any(|r| idx.iter().all(|&i| inside(i, r))) {
                out.push_copy(prim, idx);
                report.tris_out += 1;
                continue;
            }
            changed = true;
            tri_poly.clear();
            for &i in &idx {
                tri_poly.push(clip_vert(prim, i));
            }
            let (mut mnx, mut mxx, mut mnz, mut mxz) = (
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
            );
            for v in &tri_poly {
                mnx = mnx.min(v.pos[0]);
                mxx = mxx.max(v.pos[0]);
                mnz = mnz.min(v.pos[2]);
                mxz = mxz.max(v.pos[2]);
            }
            let mut emitted = 0usize;
            for rect in rects {
                if mxx < rect[0] || mnx > rect[1] || mxz < rect[2] || mnz > rect[3] {
                    continue;
                }
                poly.clear();
                poly.extend_from_slice(&tri_poly);
                clip_poly(&mut poly, &mut scratch, rect);
                for k in 1..poly.len().saturating_sub(1) {
                    let (a, b, c) = (poly[0], poly[k], poly[k + 1]);
                    if tri_area(&a, &b, &c) < 1e-12 {
                        continue;
                    }
                    out.push_tri([a, b, c]);
                    emitted += 1;
                }
            }
            if emitted == 0 {
                report.tris_dropped += 1;
            } else {
                report.tris_clipped += 1;
                report.tris_out += emitted;
            }
        }
        if changed {
            prim.positions = out.positions;
            prim.normals = out.normals;
            prim.uvs = out.uvs;
            prim.indices = out.indices;
            if out.carry_tangents {
                prim.tangents = out.tangents;
            }
            if out.carry_colors {
                prim.colors = out.colors;
            }
        }
    }
    let before = model.primitives.len();
    model.primitives.retain(|p| !p.indices.is_empty());
    report.prims_dropped = before - model.primitives.len();
    for prim in &mut model.primitives {
        report.verts_dropped += prim.compact_orphans();
    }
    model
        .log
        .push(format!("crop: union={:?} {}", rects, report.summary()));
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prim(positions: Vec<[f32; 3]>, normals: Vec<[f32; 3]>, uvs: Vec<[f32; 2]>) -> LodPrimitive {
        let indices = (0..positions.len() as u32).collect();
        LodPrimitive {
            positions,
            normals,
            uvs,
            indices,
            material: 0,
            ..Default::default()
        }
    }

    fn model_of(prims: Vec<LodPrimitive>) -> LodModel {
        LodModel {
            root_name: "crop-test".to_string(),
            primitives: prims,
            materials: Vec::new(),
            images: Vec::new(),
            log: Vec::new(),
        }
    }

    #[test]
    fn plaza_rect_rh() {
        let parcels: Vec<(i32, i32)> = (-3..=3)
            .flat_map(|x| (-4..=9).map(move |y| (x, y)))
            .collect();
        assert_eq!(parcels.len(), 98);
        let rect = crop_rect_rh((-3, -2), &parcels);
        let want = [-112.05, 0.05, -32.05, 192.05];
        for i in 0..4 {
            assert!(
                (rect[i] - want[i]).abs() < 1e-9,
                "rect[{i}] = {} want {}",
                rect[i],
                want[i]
            );
        }
    }

    #[test]
    fn rect_union_matches_single_rect_for_rectangular_scenes() {
        let plaza: Vec<(i32, i32)> = (-3..=3)
            .flat_map(|x| (-4..=9).map(move |y| (x, y)))
            .collect();
        let cases: Vec<((i32, i32), Vec<(i32, i32)>)> = vec![
            ((74, -126), vec![(74, -126)]),
            (
                (-67, -116),
                vec![(-67, -116), (-66, -116), (-67, -115), (-66, -115)],
            ),
            (
                (-63, -23),
                (-66..=-63)
                    .flat_map(|x| (-26..=-23).map(move |y| (x, y)))
                    .collect(),
            ),
            ((-3, -2), plaza),
        ];
        for (base, parcels) in cases {
            let rects = crop_rects_rh(base, &parcels);
            assert_eq!(rects.len(), 1, "base {base:?}");
            assert_eq!(rects[0], crop_rect_rh(base, &parcels), "base {base:?}");
        }
    }

    #[test]
    fn rect_union_decomposition_shapes() {
        assert_eq!(
            cell_rects(&[(0, 0), (1, 0), (0, 1), (1, 1)]),
            vec![(0, 1, 0, 1)]
        );
        assert_eq!(
            cell_rects(&[(0, 0), (1, 0), (0, 1)]),
            vec![(0, 1, 0, 0), (0, 0, 1, 1)]
        );
        assert_eq!(
            cell_rects(&[(0, 0), (2, 0)]),
            vec![(0, 0, 0, 0), (2, 2, 0, 0)]
        );
        assert_eq!(cell_rects(&[(5, 3), (5, 5), (5, 4)]), vec![(5, 5, 3, 5)]);
        let l = crop_rects_rh((0, 0), &[(0, 0), (1, 0), (0, 1)]);
        assert_eq!(l.len(), 2);
        assert_eq!(l[0], [-32.05, 0.05, -0.05, 16.05]);
        assert_eq!(l[1], [-16.05, 0.05, 15.95, 32.05]);
    }

    #[test]
    fn l_shape_drops_missing_corner_and_keeps_union() {
        let rects = crop_rects_rh((0, 0), &[(0, 0), (1, 0), (0, 1)]);
        let inside_cell00 = prim(
            vec![[-2.0, 0.0, 2.0], [-4.0, 0.0, 2.0], [-2.0, 1.0, 4.0]],
            vec![[0.0, 1.0, 0.0]; 3],
            vec![[0.0, 0.0]; 3],
        );
        let orig = inside_cell00.clone();
        let in_missing_corner = prim(
            vec![[-20.0, 0.0, 20.0], [-24.0, 0.0, 20.0], [-20.0, 1.0, 24.0]],
            vec![[0.0, 1.0, 0.0]; 3],
            vec![[0.0, 0.0]; 3],
        );
        let straddler = prim(
            vec![[-14.0, 0.0, 20.0], [-22.0, 0.0, 20.0], [-14.0, 0.0, 24.0]],
            vec![[0.0, 1.0, 0.0]; 3],
            vec![[0.0, 0.0]; 3],
        );
        let mut m = model_of(vec![inside_cell00, in_missing_corner, straddler]);
        let r = crop(&mut m, &rects);
        assert_eq!(r.rects, 2);
        assert_eq!(r.tris_in, 3);
        assert_eq!(r.tris_dropped, 1);
        assert_eq!(r.tris_clipped, 1);
        assert_eq!(r.prims_dropped, 1);
        assert_eq!(m.primitives.len(), 2);
        assert_eq!(m.primitives[0].positions, orig.positions);
        assert_eq!(m.primitives[0].indices, orig.indices);
        for p in &m.primitives[1].positions {
            let x = p[0] as f64;
            let z = p[2] as f64;
            assert!(
                rects.iter().any(|r| x >= r[0] - 1e-6
                    && x <= r[1] + 1e-6
                    && z >= r[2] - 1e-6
                    && z <= r[3] + 1e-6),
                "{p:?} outside union"
            );
            if z > 16.05 + 1e-6 {
                assert!(x >= -16.05 - 1e-6, "{p:?} in missing corner");
            }
        }
        let stats = union_stats(&m, &rects, 1e-3);
        assert_eq!(stats.outside, 0);
        assert_eq!(stats.referenced_verts, stats.buffer_verts);
        assert_eq!(stats.outside_fraction(), 0.0);
    }

    #[test]
    fn fully_inside_triangle_bit_identical() {
        let p = prim(
            vec![[0.125, 3.5, 0.25], [1.0, 0.0, 0.0], [0.0, 1.0, 2.0]],
            vec![[0.0, 0.0, 1.0], [0.6, 0.0, 0.8], [1.0, 0.0, 0.0]],
            vec![[0.1, 0.2], [0.9, 0.4], [0.5, 0.75]],
        );
        let orig = p.clone();
        let mut m = model_of(vec![p]);
        let rect = [-10.0, 10.0, -10.0, 10.0];
        let r = crop(&mut m, &[rect]);
        assert_eq!(r.tris_in, 1);
        assert_eq!(r.tris_out, 1);
        assert_eq!(r.tris_clipped, 0);
        assert_eq!(r.tris_dropped, 0);
        assert_eq!(r.prims_dropped, 0);
        assert_eq!(r.verts_dropped, 0);
        assert_eq!(m.primitives.len(), 1);
        assert_eq!(m.primitives[0].positions, orig.positions);
        assert_eq!(m.primitives[0].normals, orig.normals);
        assert_eq!(m.primitives[0].uvs, orig.uvs);
        assert_eq!(m.primitives[0].indices, orig.indices);
        assert!(m.log.iter().any(|l| l.starts_with("crop:")), "{:?}", m.log);
    }

    #[test]
    fn fully_outside_each_plane_dropped_and_prim_removed() {
        let rect = [-4.0, 4.0, -4.0, 4.0];
        let outside: Vec<[[f32; 3]; 3]> = vec![
            [[-6.0, 0.0, 0.0], [-5.0, 0.0, 0.0], [-6.0, 1.0, 1.0]],
            [[6.0, 0.0, 0.0], [5.0, 0.0, 0.0], [6.0, 1.0, 1.0]],
            [[0.0, 0.0, -6.0], [1.0, 0.0, -5.0], [0.0, 1.0, -6.0]],
            [[0.0, 0.0, 6.0], [1.0, 0.0, 5.0], [0.0, 1.0, 6.0]],
        ];
        let mut positions = Vec::new();
        for tri in &outside {
            positions.extend_from_slice(tri);
        }
        let n = positions.len();
        let p = prim(positions, vec![[0.0, 1.0, 0.0]; n], vec![[0.0, 0.0]; n]);
        let mut m = model_of(vec![p]);
        let r = crop(&mut m, &[rect]);
        assert_eq!(r.tris_in, 4);
        assert_eq!(r.tris_out, 0);
        assert_eq!(r.tris_clipped, 0);
        assert_eq!(r.tris_dropped, 4);
        assert_eq!(r.prims_dropped, 1);
        assert!(m.primitives.is_empty());
    }

    #[test]
    fn straddling_triangle_clipped_at_plane_with_lerped_attributes() {
        let rect = [-4.0, 1.0, -4.0, 4.0];
        let p = prim(
            vec![[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 0.0, 2.0]],
            vec![[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
            vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
        );
        let mut m = model_of(vec![p]);
        let r = crop(&mut m, &[rect]);
        assert_eq!(r.tris_in, 1);
        assert_eq!(r.tris_clipped, 1);
        assert_eq!(r.tris_dropped, 0);
        assert_eq!(r.tris_out, 2);
        assert_eq!(m.primitives.len(), 1);
        let out = &m.primitives[0];
        assert_eq!(out.indices.len(), 6);
        let at_plane: Vec<usize> = (0..out.positions.len())
            .filter(|&i| out.positions[i][0] == 1.0)
            .collect();
        assert!(at_plane.len() >= 2, "{:?}", out.positions);
        let mut found_edge01 = false;
        let mut found_edge12 = false;
        let inv_sqrt2 = std::f32::consts::FRAC_1_SQRT_2;
        for &i in &at_plane {
            let pos = out.positions[i];
            let uv = out.uvs[i];
            let nrm = out.normals[i];
            if pos == [1.0, 0.0, 0.0] {
                assert_eq!(uv, [0.5, 0.0]);
                assert!((nrm[0] - inv_sqrt2).abs() < 1e-6, "{nrm:?}");
                assert_eq!(nrm[1], 0.0);
                assert!((nrm[2] - inv_sqrt2).abs() < 1e-6, "{nrm:?}");
                found_edge01 = true;
            }
            if pos == [1.0, 0.0, 1.0] {
                assert_eq!(uv, [0.5, 0.5]);
                assert!((nrm[0] - 1.0).abs() < 1e-6, "{nrm:?}");
                found_edge12 = true;
            }
        }
        assert!(found_edge01);
        assert!(found_edge12);
        for pos in &out.positions {
            assert!(pos[0] as f64 <= rect[1] + 1e-9, "{pos:?}");
        }
    }

    #[test]
    fn cut_edge_lerps_tangents_and_colors() {
        let rect = [-4.0, 1.0, -4.0, 4.0];
        let mut p = prim(
            vec![[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 0.0, 2.0]],
            vec![[0.0, 0.0, 1.0]; 3],
            vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
        );
        p.tangents = vec![
            [1.0, 0.0, 0.0, -1.0],
            [0.0, 0.0, 1.0, -1.0],
            [0.0, 1.0, 0.0, -1.0],
        ];
        p.colors = vec![
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0, 0.5],
            [0.0, 0.0, 1.0, 1.0],
        ];
        let mut m = model_of(vec![p]);
        let r = crop(&mut m, &[rect]);
        assert_eq!(r.tris_clipped, 1);
        let out = &m.primitives[0];
        assert_eq!(out.tangents.len(), out.positions.len());
        assert_eq!(out.colors.len(), out.positions.len());
        let inv_sqrt2 = std::f32::consts::FRAC_1_SQRT_2;
        let mut found_cut01 = false;
        for i in 0..out.positions.len() {
            if out.positions[i] == [1.0, 0.0, 0.0] {
                let t = out.tangents[i];
                assert!((t[0] - inv_sqrt2).abs() < 1e-6, "{t:?}");
                assert_eq!(t[1], 0.0);
                assert!((t[2] - inv_sqrt2).abs() < 1e-6, "{t:?}");
                assert_eq!(t[3], -1.0);
                assert_eq!(out.colors[i], [0.5, 0.5, 0.0, 0.75]);
                found_cut01 = true;
            }
        }
        assert!(found_cut01, "{:?}", out.positions);
        for i in 0..out.positions.len() {
            if out.positions[i] == [0.0, 0.0, 0.0] {
                assert_eq!(out.tangents[i], [1.0, 0.0, 0.0, -1.0]);
                assert_eq!(out.colors[i], [1.0, 0.0, 0.0, 1.0]);
            }
        }
    }

    #[test]
    fn crop_compacts_orphan_verts_in_untouched_prims() {
        let mut p = prim(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 1.0]],
            vec![[0.0, 0.0, 1.0]; 3],
            vec![[0.0, 0.0]; 3],
        );
        p.positions.push([99.0, 0.0, 99.0]);
        p.normals.push([0.0, 1.0, 0.0]);
        p.uvs.push([0.5, 0.5]);
        let mut m = model_of(vec![p]);
        let rect = [-10.0, 10.0, -10.0, 10.0];
        let r = crop(&mut m, &[rect]);
        assert_eq!(r.tris_clipped, 0);
        assert_eq!(r.tris_out, 1);
        assert_eq!(r.verts_dropped, 1);
        let out = &m.primitives[0];
        assert_eq!(out.positions.len(), 3);
        assert_eq!(out.indices, vec![0, 1, 2]);
        let stats = union_stats(&m, &[rect], 1e-3);
        assert_eq!(stats.buffer_verts, 3);
        assert_eq!(stats.referenced_verts, 3);
        assert_eq!(stats.outside, 0);
    }

    #[test]
    fn union_stats_counts_outside_referenced_verts() {
        let p = prim(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [50.0, 0.0, 50.0]],
            vec![[0.0, 0.0, 1.0]; 3],
            vec![[0.0, 0.0]; 3],
        );
        let m = model_of(vec![p]);
        let stats = union_stats(&m, &[[-10.0, 10.0, -10.0, 10.0]], 1e-3);
        assert_eq!(stats.buffer_verts, 3);
        assert_eq!(stats.referenced_verts, 3);
        assert_eq!(stats.outside, 1);
        assert!((stats.outside_fraction() - 1.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn spanning_quad_keeps_rect_area_and_bounds_shrink() {
        let rect = [-1.0, 1.0, -1.0, 1.0];
        let p = prim(
            vec![
                [-5.0, 0.0, -5.0],
                [5.0, 0.0, -5.0],
                [5.0, 0.0, 5.0],
                [-5.0, 0.0, -5.0],
                [5.0, 0.0, 5.0],
                [-5.0, 0.0, 5.0],
            ],
            vec![[0.0, 1.0, 0.0]; 6],
            vec![[0.0, 0.0]; 6],
        );
        let mut m = model_of(vec![p]);
        let r = crop(&mut m, &[rect]);
        assert_eq!(r.tris_in, 2);
        assert!(r.tris_out >= 2);
        assert_eq!(r.tris_dropped, 0);
        let out = &m.primitives[0];
        let mut area = 0.0f64;
        for tri in out.indices.chunks_exact(3) {
            let v = |i: u32| {
                let p = out.positions[i as usize];
                ClipVert {
                    pos: [p[0] as f64, p[1] as f64, p[2] as f64],
                    normal: [0.0, 1.0, 0.0],
                    uv: [0.0, 0.0],
                    tangent: [1.0, 0.0, 0.0, 1.0],
                    color: [1.0, 1.0, 1.0, 1.0],
                }
            };
            area += tri_area(&v(tri[0]), &v(tri[1]), &v(tri[2]));
        }
        assert!((area - 4.0).abs() < 1e-9, "area {area}");
        let (mn, mx) = m.bounds();
        assert!((mn[0] as f64 - rect[0]).abs() < 1e-6, "{mn:?}");
        assert!((mx[0] as f64 - rect[1]).abs() < 1e-6, "{mx:?}");
        assert!((mn[2] as f64 - rect[2]).abs() < 1e-6, "{mn:?}");
        assert!((mx[2] as f64 - rect[3]).abs() < 1e-6, "{mx:?}");
    }

    #[test]
    fn crop_is_deterministic_on_clones() {
        let rects = crop_rects_rh((0, 0), &[(0, 0), (-1, 0), (0, -1)]);
        let prims = vec![
            prim(
                vec![[0.5, 0.0, 0.5], [1.5, 0.0, 0.25], [0.75, 1.0, 1.5]],
                vec![[0.0, 1.0, 0.0]; 3],
                vec![[0.1, 0.1], [0.8, 0.2], [0.4, 0.9]],
            ),
            prim(
                vec![[-3.0, 0.0, 0.0], [3.0, 0.5, 0.1], [0.0, 0.25, 3.0]],
                vec![[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            ),
            prim(
                vec![[19.0, 0.0, -19.0], [20.0, 0.0, -19.0], [19.0, 1.0, -20.0]],
                vec![[0.0, 1.0, 0.0]; 3],
                vec![[0.0, 0.0]; 3],
            ),
        ];
        let mut a = model_of(prims.clone());
        let mut b = model_of(prims);
        let ra = crop(&mut a, &rects);
        let rb = crop(&mut b, &rects);
        assert_eq!(ra.summary(), rb.summary());
        assert_eq!(a.primitives.len(), b.primitives.len());
        for (pa, pb) in a.primitives.iter().zip(b.primitives.iter()) {
            assert_eq!(pa.positions, pb.positions);
            assert_eq!(pa.normals, pb.normals);
            assert_eq!(pa.uvs, pb.uvs);
            assert_eq!(pa.indices, pb.indices);
            assert_eq!(pa.material, pb.material);
        }
    }
}
