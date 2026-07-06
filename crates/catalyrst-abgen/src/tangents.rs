#[inline]
const fn f32round(x: f64) -> f64 {
    x as f32 as f64
}

pub fn calculate_tangents(
    positions: &[[f64; 3]],
    normals: &[[f64; 3]],
    uvs: &[[f64; 2]],
    indices: &[u32],
) -> Vec<[f64; 4]> {
    let n = positions.len();
    if uvs.is_empty() || indices.is_empty() {
        return vec![[1.0, 0.0, 0.0, 1.0]; n];
    }

    let p: Vec<[f64; 3]> = positions
        .iter()
        .map(|v| [f32round(v[0]), f32round(v[1]), f32round(v[2])])
        .collect();
    let nv: Vec<[f64; 3]> = normals
        .iter()
        .map(|v| [f32round(v[0]), f32round(v[1]), f32round(v[2])])
        .collect();
    let uv: Vec<[f64; 2]> = uvs
        .iter()
        .map(|u| [f32round(u[0]), f32round(u[1])])
        .collect();

    let mut tan1 = vec![[0.0f64; 3]; n];
    let mut tan2 = vec![[0.0f64; 3]; n];

    let m = (indices.len() / 3) * 3;
    let mut k = 0usize;
    while k < m {
        let i1 = indices[k] as usize;
        let i2 = indices[k + 1] as usize;
        let i3 = indices[k + 2] as usize;
        let v1 = p[i1];
        let v2 = p[i2];
        let v3 = p[i3];
        let w1 = uv[i1];
        let w2 = uv[i2];
        let w3 = uv[i3];

        let x1 = f32round(v2[0] - v1[0]);
        let x2 = f32round(v3[0] - v1[0]);
        let y1 = f32round(v2[1] - v1[1]);
        let y2 = f32round(v3[1] - v1[1]);
        let z1 = f32round(v2[2] - v1[2]);
        let z2 = f32round(v3[2] - v1[2]);
        let s1 = f32round(w2[0] - w1[0]);
        let s2 = f32round(w3[0] - w1[0]);
        let t1 = f32round(w2[1] - w1[1]);
        let t2 = f32round(w3[1] - w1[1]);

        let den = s1 * t2 - s2 * t1;
        if den == 0.0 {
            k += 3;
            continue;
        }
        let r = 1.0 / den;

        let mut sx = (t2 * x1 - t1 * x2) * r;
        let mut sy = (t2 * y1 - t1 * y2) * r;
        let mut sz = (t2 * z1 - t1 * z2) * r;
        let mut tx = (s1 * x2 - s2 * x1) * r;
        let mut ty = (s1 * y2 - s2 * y1) * r;
        let mut tz = (s1 * z2 - s2 * z1) * r;

        let sl = (sx * sx + sy * sy + sz * sz).sqrt();
        if sl > 0.0 {
            sx /= sl;
            sy /= sl;
            sz /= sl;
        }
        let tl = (tx * tx + ty * ty + tz * tz).sqrt();
        if tl > 0.0 {
            tx /= tl;
            ty /= tl;
            tz /= tl;
        }

        let absden = den.abs();
        let tri = [i1, i2, i3];
        let pos = [v1, v2, v3];
        for c in 0..3 {
            let p0 = pos[c];
            let pa = pos[(c + 1) % 3];
            let pb = pos[(c + 2) % 3];

            let e1x = f32round(pa[0] - p0[0]);
            let e1y = f32round(pa[1] - p0[1]);
            let e1z = f32round(pa[2] - p0[2]);
            let e2x = f32round(pb[0] - p0[0]);
            let e2y = f32round(pb[1] - p0[1]);
            let e2z = f32round(pb[2] - p0[2]);
            let l1sq = e1x * e1x + e1y * e1y + e1z * e1z;
            let l2sq = e2x * e2x + e2y * e2y + e2z * e2z;
            let wgt = if l1sq > 0.0 && l2sq > 0.0 {
                let d = (e1x * e2x + e1y * e2y + e1z * e2z) / (l1sq * l2sq).sqrt();
                crate::detmath::acos(d.clamp(-1.0, 1.0)) * absden
            } else {
                0.0
            };
            let vi = tri[c];
            tan1[vi][0] += wgt * sx;
            tan1[vi][1] += wgt * sy;
            tan1[vi][2] += wgt * sz;
            tan2[vi][0] += wgt * tx;
            tan2[vi][1] += wgt * ty;
            tan2[vi][2] += wgt * tz;
        }
        k += 3;
    }

    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let nn = nv[i];
        let t = tan1[i];
        let d = nn[0] * t[0] + nn[1] * t[1] + nn[2] * t[2];
        let ox = t[0] - nn[0] * d;
        let oy = t[1] - nn[1] * d;
        let oz = t[2] - nn[2] * d;
        let mag = (ox * ox + oy * oy + oz * oz).sqrt();
        let ax = nn[0].abs();
        let ay = nn[1].abs();
        let az = nn[2].abs();
        let (fbx, fby, fbz, bx, by, bz) = if ax <= ay && ax <= az {
            if ay <= az {
                (1.0, 0.0, 0.0, 0.0, 1.0, 0.0)
            } else {
                (1.0, 0.0, 0.0, 0.0, 0.0, 1.0)
            }
        } else if ay <= az {
            if ax <= az {
                (0.0, 1.0, 0.0, 1.0, 0.0, 0.0)
            } else {
                (0.0, 1.0, 0.0, 0.0, 0.0, 1.0)
            }
        } else if ax <= ay {
            (0.0, 0.0, 1.0, 1.0, 0.0, 0.0)
        } else {
            (0.0, 0.0, 1.0, 0.0, 1.0, 0.0)
        };
        let (tgx, tgy, tgz);

        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        let degenerate = !(mag > 1e-6);
        if !degenerate {
            tgx = f32round(ox / mag);
            tgy = f32round(oy / mag);
            tgz = f32round(oz / mag);
        } else {
            let dd = nn[0] * fbx + nn[1] * fby + nn[2] * fbz;
            let ox2 = fbx - nn[0] * dd;
            let oy2 = fby - nn[1] * dd;
            let oz2 = fbz - nn[2] * dd;
            let mag2 = (ox2 * ox2 + oy2 * oy2 + oz2 * oz2).sqrt();
            if mag2 > 0.0 {
                tgx = f32round(ox2 / mag2);
                tgy = f32round(oy2 / mag2);
                tgz = f32round(oz2 / mag2);
            } else {
                tgx = fbx;
                tgy = fby;
                tgz = fbz;
            }
        }
        let cx = nn[1] * tgz - nn[2] * tgy;
        let cy = nn[2] * tgx - nn[0] * tgz;
        let cz = nn[0] * tgy - nn[1] * tgx;
        let tb = tan2[i];

        let w = if degenerate {
            let hdot_fb = cx * bx + cy * by + cz * bz;
            if hdot_fb > 0.0 {
                1.0
            } else {
                -1.0
            }
        } else {
            let hdot_main = cx * tb[0] + cy * tb[1] + cz * tb[2];
            if hdot_main != 0.0 {
                if hdot_main > 0.0 {
                    1.0
                } else {
                    -1.0
                }
            } else {
                let hdot_fb = cx * bx + cy * by + cz * bz;
                if hdot_fb > 0.0 {
                    1.0
                } else {
                    -1.0
                }
            }
        };
        out.push([tgx, tgy, tgz, w]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn degenerate_fallback() {
        let pos = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let nrm = vec![[0.0, 0.0, 1.0]; 3];
        let idx = vec![0u32, 1, 2];

        assert_eq!(
            calculate_tangents(&pos, &nrm, &[], &idx),
            vec![[1.0, 0.0, 0.0, 1.0]; 3]
        );

        let uv = vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        assert_eq!(
            calculate_tangents(&pos, &nrm, &uv, &[]),
            vec![[1.0, 0.0, 0.0, 1.0]; 3]
        );
    }

    #[test]
    fn matches_reference() {
        let pos = vec![[0.0, 0.0, 0.0], [1.0, 0.5, 0.2], [0.3, 1.0, -0.4]];
        let nrm = vec![[0.1, 0.2, 0.97], [0.0, 0.1, 0.99], [-0.2, 0.0, 0.98]];
        let uv = vec![[0.1, 0.2], [0.9, 0.3], [0.4, 0.8]];
        let idx = vec![0u32, 1, 2];

        let expected = [
            [
                0.9536779522895813,
                0.2618159055709839,
                -0.14815813302993774,
                1.0,
            ],
            [
                0.9521734714508057,
                0.3042945861816406,
                -0.027756698429584503,
                1.0,
            ],
            [
                0.9280225038528442,
                0.320804238319397,
                0.18936432898044586,
                1.0,
            ],
        ];
        let got = calculate_tangents(&pos, &nrm, &uv, &idx);
        for (g, e) in got.iter().zip(expected.iter()) {
            for c in 0..4 {
                assert!((g[c] - e[c]).abs() < 1e-6, "got {g:?} expected {e:?}");
            }
        }
    }

    #[test]
    fn matches_reference_w_negative() {
        let pos = vec![[0.0, 0.0, 0.0], [1.0, 0.5, 0.2], [0.3, 1.0, -0.4]];
        let nrm = vec![[0.1, 0.2, 0.97], [0.0, 0.1, 0.99], [-0.2, 0.0, 0.98]];

        let uv = vec![[0.1, 0.2], [0.4, 0.8], [0.9, 0.3]];
        let idx = vec![0u32, 1, 2];
        let expected = [
            [
                0.15606316924095154,
                0.963642418384552,
                -0.21688134968280792,
                -1.0,
            ],
            [
                0.13819241523742676,
                0.9850354790687561,
                -0.10299479961395264,
                -1.0,
            ],
            [
                0.046927809715270996,
                0.998850405216217,
                0.009777856990695,
                -1.0,
            ],
        ];
        let got = calculate_tangents(&pos, &nrm, &uv, &idx);
        for (g, e) in got.iter().zip(expected.iter()) {
            assert_eq!(g[3], e[3], "w sign mismatch: got {g:?} expected {e:?}");
            for c in 0..4 {
                assert!((g[c] - e[c]).abs() < 1e-6, "got {g:?} expected {e:?}");
            }
        }
    }

    #[test]
    fn matches_reference_multi_triangle() {
        let pos = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.1],
            [0.0, 1.0, 0.2],
            [1.0, 1.0, -0.3],
        ];
        let nrm = vec![
            [0.0, 0.0, 1.0],
            [0.1, 0.0, 0.99],
            [0.0, 0.1, 0.99],
            [-0.1, -0.1, 0.98],
        ];
        let uv = vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]];
        let idx = vec![0u32, 1, 2, 1, 3, 2];
        let expected = [
            [1.0, 0.0, 0.0, 1.0],
            [0.9948405027389526, 0.0, -0.10145119577646255, 1.0],
            [
                0.9998064637184143,
                0.019286872819066048,
                -0.0038768525701016188,
                1.0,
            ],
            [
                0.9946249723434448,
                -0.062362249940633774,
                0.08265642821788788,
                1.0,
            ],
        ];
        let got = calculate_tangents(&pos, &nrm, &uv, &idx);
        for (g, e) in got.iter().zip(expected.iter()) {
            for c in 0..4 {
                assert!((g[c] - e[c]).abs() < 1e-6, "got {g:?} expected {e:?}");
            }
        }
    }
}
