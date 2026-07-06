use anyhow::Result;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AlphaClass {
    Opaque,
    Mask,
    Blend,
}

impl AlphaClass {
    pub fn from_alpha_mode(mode: &str) -> AlphaClass {
        match mode {
            "MASK" => AlphaClass::Mask,
            "BLEND" => AlphaClass::Blend,
            _ => AlphaClass::Opaque,
        }
    }

    pub fn gltf_name(&self) -> &'static str {
        match self {
            AlphaClass::Opaque => "OPAQUE",
            AlphaClass::Mask => "MASK",
            AlphaClass::Blend => "BLEND",
        }
    }
}

#[derive(Clone, Debug)]
pub struct LodImage {
    pub bytes: Vec<u8>,
    pub mime: String,
}

#[derive(Clone, Debug)]
pub struct LodMaterial {
    pub name: String,
    pub class: AlphaClass,
    pub base_color: [f64; 4],
    pub cutoff: f64,
    pub image: Option<usize>,
    pub double_sided: bool,
}

#[derive(Clone, Debug, Default)]
pub struct LodPrimitive {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub tangents: Vec<[f32; 4]>,
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
    pub material: usize,
}

fn keep_used<T: Copy>(v: &mut Vec<T>, used: &[bool]) {
    if v.len() != used.len() {
        return;
    }
    let mut w = 0usize;
    for i in 0..used.len() {
        if used[i] {
            v[w] = v[i];
            w += 1;
        }
    }
    v.truncate(w);
}

impl LodPrimitive {
    pub fn compact_orphans(&mut self) -> usize {
        let n = self.positions.len();
        let mut used = vec![false; n];
        for &i in &self.indices {
            used[i as usize] = true;
        }
        let kept = used.iter().filter(|&&u| u).count();
        if kept == n {
            return 0;
        }
        let mut remap = vec![0u32; n];
        let mut next = 0u32;
        for (i, &u) in used.iter().enumerate() {
            if u {
                remap[i] = next;
                next += 1;
            }
        }
        keep_used(&mut self.positions, &used);
        keep_used(&mut self.normals, &used);
        keep_used(&mut self.uvs, &used);
        keep_used(&mut self.tangents, &used);
        keep_used(&mut self.colors, &used);
        for idx in self.indices.iter_mut() {
            *idx = remap[*idx as usize];
        }
        n - kept
    }
}

#[derive(Clone, Debug, Default)]
pub struct LodModel {
    pub root_name: String,
    pub primitives: Vec<LodPrimitive>,
    pub materials: Vec<LodMaterial>,
    pub images: Vec<LodImage>,
    pub log: Vec<String>,
}

impl LodModel {
    pub fn total_tris(&self) -> usize {
        self.primitives.iter().map(|p| p.indices.len() / 3).sum()
    }

    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        let mut mn = [f32::INFINITY; 3];
        let mut mx = [f32::NEG_INFINITY; 3];
        for prim in &self.primitives {
            for p in &prim.positions {
                for i in 0..3 {
                    mn[i] = mn[i].min(p[i]);
                    mx[i] = mx[i].max(p[i]);
                }
            }
        }
        if mn[0] > mx[0] {
            return ([0.0; 3], [0.0; 3]);
        }
        (mn, mx)
    }
}

pub(crate) fn mat4_identity() -> [f64; 16] {
    let mut m = [0.0; 16];
    m[0] = 1.0;
    m[5] = 1.0;
    m[10] = 1.0;
    m[15] = 1.0;
    m
}

pub(crate) fn mat4_mul(a: &[f64; 16], b: &[f64; 16]) -> [f64; 16] {
    let mut out = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            let mut acc = 0.0;
            for k in 0..4 {
                acc += a[k * 4 + r] * b[c * 4 + k];
            }
            out[c * 4 + r] = acc;
        }
    }
    out
}

pub(crate) fn mat4_from_trs(t: [f64; 3], q: [f64; 4], s: [f64; 3]) -> [f64; 16] {
    let (x, y, z, w) = (q[0], q[1], q[2], q[3]);
    let r = [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y + z * w),
            2.0 * (x * z - y * w),
        ],
        [
            2.0 * (x * y - z * w),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z + x * w),
        ],
        [
            2.0 * (x * z + y * w),
            2.0 * (y * z - x * w),
            1.0 - 2.0 * (x * x + y * y),
        ],
    ];
    let mut m = [0.0; 16];
    for c in 0..3 {
        for row in 0..3 {
            m[c * 4 + row] = r[c][row] * s[c];
        }
    }
    m[12] = t[0];
    m[13] = t[1];
    m[14] = t[2];
    m[15] = 1.0;
    m
}

pub(crate) fn det3(m: &[f64; 16]) -> f64 {
    let c0 = [m[0], m[1], m[2]];
    let c1 = [m[4], m[5], m[6]];
    let c2 = [m[8], m[9], m[10]];
    c0[0] * (c1[1] * c2[2] - c1[2] * c2[1]) - c1[0] * (c0[1] * c2[2] - c0[2] * c2[1])
        + c2[0] * (c0[1] * c1[2] - c0[2] * c1[1])
}

pub(crate) fn inv_transpose3(m: &[f64; 16]) -> Option<[f64; 9]> {
    let a00 = m[0];
    let a10 = m[1];
    let a20 = m[2];
    let a01 = m[4];
    let a11 = m[5];
    let a21 = m[6];
    let a02 = m[8];
    let a12 = m[9];
    let a22 = m[10];
    let det = det3(m);
    if det.abs() < 1e-20 {
        return None;
    }
    let c = [
        a11 * a22 - a12 * a21,
        -(a10 * a22 - a12 * a20),
        a10 * a21 - a11 * a20,
        -(a01 * a22 - a02 * a21),
        a00 * a22 - a02 * a20,
        -(a00 * a21 - a01 * a20),
        a01 * a12 - a02 * a11,
        -(a00 * a12 - a02 * a10),
        a00 * a11 - a01 * a10,
    ];
    let mut out = [0.0; 9];
    for i in 0..9 {
        out[i] = c[i] / det;
    }
    Some(out)
}

pub(crate) fn mul_point(m: &[f64; 16], p: [f64; 3]) -> [f64; 3] {
    [
        m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
        m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
        m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
    ]
}

pub(crate) fn mul_normal(n: &[f64; 9], v: [f64; 3]) -> [f64; 3] {
    [
        n[0] * v[0] + n[1] * v[1] + n[2] * v[2],
        n[3] * v[0] + n[4] * v[1] + n[5] * v[2],
        n[6] * v[0] + n[7] * v[1] + n[8] * v[2],
    ]
}

fn sniff_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() >= 8 && bytes[0..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
        return Some("image/png");
    }
    if bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF {
        return Some("image/jpeg");
    }
    None
}

fn png_reencode(
    scene: &crate::scene::Scene,
    idx: usize,
    model: &mut LodModel,
) -> Option<(Vec<u8>, String)> {
    let img = scene.images.get(idx)?.as_ref()?;
    let mut cur = std::io::Cursor::new(Vec::new());
    img.write_to(&mut cur, image::ImageFormat::Png).ok()?;
    model.log.push(format!("image {idx}: reencoded to png"));
    Some((cur.into_inner(), "image/png".to_string()))
}

pub(crate) fn intern_image(
    scene: &crate::scene::Scene,
    idx: usize,
    by_hash: &mut HashMap<String, usize>,
    model: &mut LodModel,
) -> Option<usize> {
    let raw = scene.image_bytes.get(idx).and_then(|b| b.clone());
    let (bytes, mime) = match raw {
        Some(r) => match sniff_mime(&r) {
            Some(m) => (r, m.to_string()),
            None => png_reencode(scene, idx, model)?,
        },
        None => png_reencode(scene, idx, model)?,
    };
    let key = crate::hashes::sha256_hex(&bytes);
    if let Some(&i) = by_hash.get(&key) {
        return Some(i);
    }
    let i = model.images.len();
    model.images.push(LodImage { bytes, mime });
    by_hash.insert(key, i);
    Some(i)
}

fn walk(
    scene: &crate::scene::Scene,
    idx: usize,
    parent: &[f64; 16],
    scene_mat_count: usize,
    fallback: &mut Option<usize>,
    model: &mut LodModel,
) {
    if idx >= scene.nodes.len() {
        return;
    }
    let node = &scene.nodes[idx];
    if node.is_collider || node.name_is_collider {
        model
            .log
            .push(format!("skip collider subtree {:?}", node.name));
        return;
    }
    let local = mat4_from_trs(node.translation, node.rotation, node.scale);
    let world = mat4_mul(parent, &local);
    let det = det3(&world);
    let nmat = inv_transpose3(&world);
    for prim in &node.primitives {
        if prim.positions.is_empty() || prim.indices.len() < 3 {
            continue;
        }
        let mut positions = Vec::with_capacity(prim.positions.len());
        for p in &prim.positions {
            let w = mul_point(&world, *p);
            positions.push([(-w[0]) as f32, w[1] as f32, w[2] as f32]);
        }
        let mut normals = Vec::with_capacity(prim.normals.len());
        for n in &prim.normals {
            let v = match &nmat {
                Some(m) => mul_normal(m, *n),
                None => *n,
            };
            let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            let v = if len > 1e-12 {
                [v[0] / len, v[1] / len, v[2] / len]
            } else {
                v
            };
            normals.push([(-v[0]) as f32, v[1] as f32, v[2] as f32]);
        }
        if normals.len() != positions.len() {
            normals.resize(positions.len(), [0.0, 0.0, 1.0]);
        }
        let mut uvs: Vec<[f32; 2]> = match prim.uvs.as_ref() {
            Some(uv) => uv
                .iter()
                .map(|u| [u[0] as f32, (1.0 - u[1]) as f32])
                .collect(),
            None => vec![[0.0, 0.0]; positions.len()],
        };
        if uvs.len() != positions.len() {
            uvs.resize(positions.len(), [0.0, 0.0]);
        }
        let mut indices = Vec::with_capacity(prim.indices.len());
        for tri in prim.indices.chunks_exact(3) {
            if det >= 0.0 {
                indices.extend_from_slice(&[tri[0], tri[2], tri[1]]);
            } else {
                indices.extend_from_slice(&[tri[0], tri[1], tri[2]]);
            }
        }
        let material = match prim.material_index {
            Some(mi) if mi < scene_mat_count => mi,
            _ => {
                if fallback.is_none() {
                    model.materials.push(LodMaterial {
                        name: "default".to_string(),
                        class: AlphaClass::Opaque,
                        base_color: [1.0, 1.0, 1.0, 1.0],
                        cutoff: 0.5,
                        image: None,
                        double_sided: false,
                    });
                    *fallback = Some(model.materials.len() - 1);
                }
                fallback.unwrap()
            }
        };
        model.primitives.push(LodPrimitive {
            positions,
            normals,
            uvs,
            indices,
            material,
            ..Default::default()
        });
    }
    for &c in &node.children {
        walk(scene, c, &world, scene_mat_count, fallback, model);
    }
}

pub fn from_glb_bytes(bytes: &[u8], root_name: &str) -> Result<LodModel> {
    let scene = crate::gltf::parse(bytes, ".glb", None, false, true)?;
    let mut model = LodModel {
        root_name: root_name.to_string(),
        ..Default::default()
    };
    let mut by_hash: HashMap<String, usize> = HashMap::new();
    let mut image_slot: Vec<Option<Option<usize>>> = vec![None; scene.images.len()];
    for m in &scene.materials {
        let image = match m.base_color_image {
            Some(tr) if tr.image < scene.images.len() => {
                if image_slot[tr.image].is_none() {
                    image_slot[tr.image] =
                        Some(intern_image(&scene, tr.image, &mut by_hash, &mut model));
                }
                image_slot[tr.image].unwrap()
            }
            _ => None,
        };
        model.materials.push(LodMaterial {
            name: m.name.clone(),
            class: AlphaClass::from_alpha_mode(&m.alpha_mode),
            base_color: m.base_color,
            cutoff: m.alpha_cutoff,
            image,
            double_sided: m.double_sided,
        });
    }
    let scene_mat_count = scene.materials.len();
    let mut fallback: Option<usize> = None;
    let ident = mat4_identity();
    for &r in &scene.root_nodes {
        walk(
            &scene,
            r,
            &ident,
            scene_mat_count,
            &mut fallback,
            &mut model,
        );
    }
    Ok(model)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpha_mode_mapping() {
        assert_eq!(AlphaClass::from_alpha_mode("OPAQUE"), AlphaClass::Opaque);
        assert_eq!(AlphaClass::from_alpha_mode("MASK"), AlphaClass::Mask);
        assert_eq!(AlphaClass::from_alpha_mode("BLEND"), AlphaClass::Blend);
        assert_eq!(AlphaClass::from_alpha_mode(""), AlphaClass::Opaque);
        assert_eq!(AlphaClass::from_alpha_mode("mask"), AlphaClass::Opaque);
    }

    #[test]
    fn tris_and_bounds() {
        let model = LodModel {
            root_name: "r".to_string(),
            primitives: vec![
                LodPrimitive {
                    positions: vec![[-1.5, 0.0, 2.0], [1.0, 0.25, -3.5], [0.5, 2.0, 0.75]],
                    normals: vec![[0.0, 0.0, 1.0]; 3],
                    uvs: vec![[0.0, 0.0]; 3],
                    indices: vec![0, 1, 2],
                    material: 0,
                    ..Default::default()
                },
                LodPrimitive {
                    positions: vec![[10.0, -4.0, 0.0], [11.0, 0.0, 0.0], [10.0, 1.0, 7.0]],
                    normals: vec![[0.0, 0.0, 1.0]; 3],
                    uvs: vec![[0.0, 0.0]; 3],
                    indices: vec![0, 1, 2, 0, 2, 1],
                    material: 0,
                    ..Default::default()
                },
            ],
            materials: Vec::new(),
            images: Vec::new(),
            log: Vec::new(),
        };
        assert_eq!(model.total_tris(), 3);
        let (mn, mx) = model.bounds();
        assert_eq!(mn, [-1.5, -4.0, -3.5]);
        assert_eq!(mx, [11.0, 2.0, 7.0]);
        let empty = LodModel::default();
        assert_eq!(empty.total_tris(), 0);
        assert_eq!(empty.bounds(), ([0.0; 3], [0.0; 3]));
    }

    #[test]
    fn compact_orphans_noop_when_all_referenced() {
        let mut p = LodPrimitive {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            indices: vec![2, 0, 1],
            material: 3,
            ..Default::default()
        };
        let orig = p.clone();
        assert_eq!(p.compact_orphans(), 0);
        assert_eq!(p.positions, orig.positions);
        assert_eq!(p.normals, orig.normals);
        assert_eq!(p.uvs, orig.uvs);
        assert_eq!(p.indices, orig.indices);
        assert!(p.tangents.is_empty());
        assert!(p.colors.is_empty());
    }

    #[test]
    fn compact_orphans_drops_unreferenced_and_remaps() {
        let mut p = LodPrimitive {
            positions: vec![
                [0.0, 0.0, 0.0],
                [9.0, 9.0, 9.0],
                [1.0, 0.0, 0.0],
                [8.0, 8.0, 8.0],
                [0.0, 1.0, 0.0],
            ],
            normals: vec![
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            uvs: vec![[0.0, 0.0], [0.9, 0.9], [1.0, 0.0], [0.8, 0.8], [0.0, 1.0]],
            tangents: vec![
                [1.0, 0.0, 0.0, 1.0],
                [0.0, 1.0, 0.0, -1.0],
                [0.0, 0.0, 1.0, 1.0],
                [0.0, 1.0, 0.0, -1.0],
                [1.0, 0.0, 0.0, -1.0],
            ],
            colors: vec![
                [1.0, 0.0, 0.0, 1.0],
                [0.5, 0.5, 0.5, 0.5],
                [0.0, 1.0, 0.0, 1.0],
                [0.5, 0.5, 0.5, 0.5],
                [0.0, 0.0, 1.0, 1.0],
            ],
            indices: vec![0, 2, 4],
            material: 0,
        };
        assert_eq!(p.compact_orphans(), 2);
        assert_eq!(
            p.positions,
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
        );
        assert_eq!(
            p.normals,
            vec![[0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
        );
        assert_eq!(p.uvs, vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
        assert_eq!(
            p.tangents,
            vec![
                [1.0, 0.0, 0.0, 1.0],
                [0.0, 0.0, 1.0, 1.0],
                [1.0, 0.0, 0.0, -1.0]
            ]
        );
        assert_eq!(
            p.colors,
            vec![
                [1.0, 0.0, 0.0, 1.0],
                [0.0, 1.0, 0.0, 1.0],
                [0.0, 0.0, 1.0, 1.0]
            ]
        );
        assert_eq!(p.indices, vec![0, 1, 2]);
        assert_eq!(p.compact_orphans(), 0);
    }
}
