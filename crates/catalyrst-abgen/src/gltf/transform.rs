use super::accessors::jarr;
use serde_json::Value as J;

fn dot3_f32(a: [f32; 3], b: [f32; 3]) -> f32 {
    f32::mul_add(a[2], b[2], f32::mul_add(a[1], b[1], a[0] * b[0]))
}
fn dot4_f32(a: [f32; 4], b: [f32; 4]) -> f32 {
    f32::mul_add(
        a[3],
        b[3],
        f32::mul_add(a[2], b[2], f32::mul_add(a[1], b[1], a[0] * b[0])),
    )
}
fn normalize3_f32(c: [f32; 3]) -> [f32; 3] {
    let inv = 1.0f32 / dot3_f32(c, c).sqrt();
    [c[0] * inv, c[1] * inv, c[2] * inv]
}

fn quat_from_3x3_unity(c0: [f32; 3], c1: [f32; 3], c2: [f32; 3]) -> [f32; 4] {
    let (ux, uy, uz) = (c0[0], c0[1], c0[2]);
    let (vx, vy, vz) = (c1[0], c1[1], c1[2]);
    let (wx, wy, wz) = (c2[0], c2[1], c2[2]);
    let u_sign = ux.to_bits() & 0x8000_0000;
    let t = vy + f32::from_bits(wz.to_bits() ^ u_sign);
    let u_mask: u32 = if u_sign != 0 { 0xFFFF_FFFF } else { 0 };
    let t_mask: u32 = if t.to_bits() & 0x8000_0000 != 0 {
        0xFFFF_FFFF
    } else {
        0
    };
    let tr = 1.0f32 + ux.abs();
    let base = [0u32, 0x8000_0000, 0x8000_0000, 0x8000_0000];
    let ux_xor = [0u32, 0x8000_0000, 0u32, 0x8000_0000];
    let tx_xor = [0x8000_0000u32, 0x8000_0000, 0x8000_0000, 0u32];
    let mut sf = [0u32; 4];
    for i in 0..4 {
        sf[i] = base[i] ^ (u_mask & ux_xor[i]) ^ (t_mask & tx_xor[i]);
    }
    let lhs = [tr, uy, wx, vz];
    let rhs_in = [t, vx, uz, wy];
    let mut v = [0f32; 4];
    for i in 0..4 {
        v[i] = lhs[i] + f32::from_bits(rhs_in[i].to_bits() ^ sf[i]);
    }
    if u_mask != 0 {
        v = [v[2], v[3], v[0], v[1]];
    }
    if t_mask == 0 {
        v = [v[3], v[2], v[1], v[0]];
    }
    let inv = 1.0f32 / dot4_f32(v, v).sqrt();
    [v[0] * inv, v[1] * inv, v[2] * inv, v[3] * inv]
}

fn trs_from_matrix(m: &[f64; 16]) -> ([f64; 3], [f64; 4], [f64; 3]) {
    let t = [m[12], m[13], m[14]];
    let mut c0 = [m[0] as f32, -(m[1] as f32), -(m[2] as f32)];
    let mut c1 = [-(m[4] as f32), m[5] as f32, m[6] as f32];
    let mut c2 = [-(m[8] as f32), m[9] as f32, m[10] as f32];
    let len0 = dot3_f32(c0, c0).sqrt();
    let len1 = dot3_f32(c1, c1).sqrt();
    let len2 = dot3_f32(c2, c2).sqrt();
    for i in 0..3 {
        c0[i] /= len0;
        c1[i] /= len1;
        c2[i] /= len2;
    }
    let mut s = [len0, len1, len2];
    let cross = [
        c0[1] * c1[2] - c0[2] * c1[1],
        c0[2] * c1[0] - c0[0] * c1[2],
        c0[0] * c1[1] - c0[1] * c1[0],
    ];
    if dot3_f32(cross, c2) < 0.0 {
        for i in 0..3 {
            c0[i] = -c0[i];
            c1[i] = -c1[i];
            c2[i] = -c2[i];
        }
        for i in 0..3 {
            s[i] = -s[i];
        }
    }
    c0 = normalize3_f32(c0);
    c1 = normalize3_f32(c1);
    c2 = normalize3_f32(c2);
    let q = quat_from_3x3_unity(c0, c1, c2);
    (
        t,
        [q[0] as f64, q[1] as f64, q[2] as f64, q[3] as f64],
        [s[0] as f64, s[1] as f64, s[2] as f64],
    )
}

fn normalize_quat_f32(q: [f64; 4]) -> [f64; 4] {
    let qq = [q[0] as f32, q[1] as f32, q[2] as f32, q[3] as f32];
    let sq = [qq[0] * qq[0], qq[1] * qq[1], qq[2] * qq[2], qq[3] * qq[3]];

    let s0 = (sq[0] + sq[1]) + (sq[2] + sq[3]);

    let s7 = (sq[0] + sq[3]) + (sq[1] + sq[2]);
    if s0 == 0.0 || s7 == 0.0 {
        return [qq[0] as f64, qq[1] as f64, qq[2] as f64, qq[3] as f64];
    }
    let n0 = s0.sqrt();
    let n7 = s7.sqrt();
    [
        (qq[0] / n0) as f64,
        (qq[1] / n7) as f64,
        (qq[2] / n0) as f64,
        (qq[3] / n7) as f64,
    ]
}

pub(super) fn node_trs(node: &J) -> ([f64; 3], [f64; 4], [f64; 3], bool) {
    if let Some(marr) = jarr(node, "matrix") {
        if marr.len() == 16 {
            let mut m = [0.0f64; 16];
            for (i, v) in marr.iter().enumerate() {
                m[i] = v.as_f64().unwrap_or(0.0);
            }
            let (t, r, s) = trs_from_matrix(&m);

            let r = normalize_quat_f32(r);
            return (t, r, s, true);
        }
    }
    let (t, has_translation) = match jarr(node, "translation") {
        Some(a) => (
            [
                a[0].as_f64().unwrap(),
                a[1].as_f64().unwrap(),
                a[2].as_f64().unwrap(),
            ],
            true,
        ),
        None => ([0.0, 0.0, 0.0], false),
    };

    let r = match jarr(node, "rotation") {
        Some(a) => {
            let rq = [
                a[0].as_f64().unwrap(),
                -a[1].as_f64().unwrap(),
                -a[2].as_f64().unwrap(),
                a[3].as_f64().unwrap(),
            ];
            normalize_quat_f32(rq)
        }
        None => [0.0, 0.0, 0.0, 1.0],
    };
    let s = jarr(node, "scale")
        .map(|a| {
            [
                a[0].as_f64().unwrap(),
                a[1].as_f64().unwrap(),
                a[2].as_f64().unwrap(),
            ]
        })
        .unwrap_or([1.0, 1.0, 1.0]);
    (t, r, s, has_translation)
}
