#![allow(clippy::needless_range_loop)]

pub struct Node {
    pub feature: i16,
    pub threshold: i32,
    pub left: i16,
    pub right: i16,
}

pub const TREE_NODES: usize = 127;

pub const TREE: [Node; TREE_NODES] = [
    Node {
        feature: 1,
        threshold: 0,
        left: 1,
        right: 2,
    },
    Node {
        feature: 2,
        threshold: 0,
        left: 7,
        right: 8,
    },
    Node {
        feature: 7,
        threshold: 254,
        left: 3,
        right: 4,
    },
    Node {
        feature: 6,
        threshold: 0,
        left: 21,
        right: 22,
    },
    Node {
        feature: 2,
        threshold: 31,
        left: 5,
        right: 6,
    },
    Node {
        feature: 9,
        threshold: 203,
        left: 9,
        right: 10,
    },
    Node {
        feature: 6,
        threshold: 251,
        left: 19,
        right: 20,
    },
    Node {
        feature: 4,
        threshold: 0,
        left: 15,
        right: 16,
    },
    Node {
        feature: 4,
        threshold: 0,
        left: 23,
        right: 24,
    },
    Node {
        feature: 3,
        threshold: 0,
        left: 17,
        right: 18,
    },
    Node {
        feature: 9,
        threshold: 235,
        left: 11,
        right: 12,
    },
    Node {
        feature: 3,
        threshold: 0,
        left: 13,
        right: 14,
    },
    Node {
        feature: 1,
        threshold: 3,
        left: 53,
        right: 54,
    },
    Node {
        feature: 2,
        threshold: 0,
        left: 49,
        right: 50,
    },
    Node {
        feature: 2,
        threshold: 8,
        left: 29,
        right: 30,
    },
    Node {
        feature: 3,
        threshold: 0,
        left: 33,
        right: 34,
    },
    Node {
        feature: 9,
        threshold: 233,
        left: 77,
        right: 78,
    },
    Node {
        feature: 2,
        threshold: 1,
        left: 61,
        right: 62,
    },
    Node {
        feature: 5,
        threshold: 0,
        left: 35,
        right: 36,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 4,
        right: 6623,
    },
    Node {
        feature: 8,
        threshold: 72,
        left: 27,
        right: 28,
    },
    Node {
        feature: 4,
        threshold: 0,
        left: 37,
        right: 38,
    },
    Node {
        feature: 4,
        threshold: 0,
        left: 43,
        right: 44,
    },
    Node {
        feature: 9,
        threshold: 207,
        left: 25,
        right: 26,
    },
    Node {
        feature: 3,
        threshold: 5,
        left: 55,
        right: 56,
    },
    Node {
        feature: 6,
        threshold: 254,
        left: 67,
        right: 68,
    },
    Node {
        feature: 7,
        threshold: 254,
        left: 31,
        right: 32,
    },
    Node {
        feature: 3,
        threshold: 51,
        left: 57,
        right: 58,
    },
    Node {
        feature: 0,
        threshold: 12186,
        left: 79,
        right: 80,
    },
    Node {
        feature: 3,
        threshold: 1,
        left: 51,
        right: 52,
    },
    Node {
        feature: 3,
        threshold: 12,
        left: 45,
        right: 46,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 6,
        right: 9914,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 5,
        right: 8047,
    },
    Node {
        feature: 9,
        threshold: 176,
        left: 105,
        right: 106,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 6,
        right: 3998,
    },
    Node {
        feature: 2,
        threshold: 0,
        left: 41,
        right: 42,
    },
    Node {
        feature: 6,
        threshold: 123,
        left: 103,
        right: 104,
    },
    Node {
        feature: 9,
        threshold: 225,
        left: 39,
        right: 40,
    },
    Node {
        feature: 3,
        threshold: 4,
        left: 115,
        right: 116,
    },
    Node {
        feature: 0,
        threshold: 87,
        left: 81,
        right: 82,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 5,
        right: 5000,
    },
    Node {
        feature: 3,
        threshold: 1,
        left: 89,
        right: 90,
    },
    Node {
        feature: 1,
        threshold: 6,
        left: 47,
        right: 48,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 6,
        right: 9997,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 7,
        right: 9245,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 3,
        right: 7947,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 6,
        right: 6619,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 6,
        right: 4218,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 6,
        right: 5609,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 6,
        right: 4568,
    },
    Node {
        feature: 2,
        threshold: 11,
        left: 71,
        right: 72,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 3,
        right: 5546,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 3,
        right: 4627,
    },
    Node {
        feature: 2,
        threshold: 0,
        left: 59,
        right: 60,
    },
    Node {
        feature: 6,
        threshold: 249,
        left: 65,
        right: 66,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 5,
        right: 6228,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 4,
        right: 8039,
    },
    Node {
        feature: 1,
        threshold: 29,
        left: 63,
        right: 64,
    },
    Node {
        feature: 9,
        threshold: 40,
        left: 87,
        right: 88,
    },
    Node {
        feature: 1,
        threshold: 1,
        left: 75,
        right: 76,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 6,
        right: 9045,
    },
    Node {
        feature: 2,
        threshold: 0,
        left: 117,
        right: 118,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 6,
        right: 9511,
    },
    Node {
        feature: 9,
        threshold: 184,
        left: 91,
        right: 92,
    },
    Node {
        feature: 2,
        threshold: 55,
        left: 69,
        right: 70,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 5,
        right: 8623,
    },
    Node {
        feature: 3,
        threshold: 30,
        left: 83,
        right: 84,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 6,
        right: 9367,
    },
    Node {
        feature: 9,
        threshold: 137,
        left: 73,
        right: 74,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 6,
        right: 4762,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 1,
        right: 5383,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 5,
        right: 8386,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 6,
        right: 6187,
    },
    Node {
        feature: 3,
        threshold: 10,
        left: 119,
        right: 120,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 6,
        right: 7504,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 1,
        right: 9792,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 6,
        right: 5661,
    },
    Node {
        feature: 4,
        threshold: 48,
        left: 97,
        right: 98,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 7,
        right: 7887,
    },
    Node {
        feature: 0,
        threshold: 8408,
        left: 93,
        right: 94,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 6,
        right: 6984,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 6,
        right: 8285,
    },
    Node {
        feature: 1,
        threshold: 254,
        left: 85,
        right: 86,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 6,
        right: 9279,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 1,
        right: 6405,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 6,
        right: 6886,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 4,
        right: 4201,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 1,
        right: 8363,
    },
    Node {
        feature: 9,
        threshold: 217,
        left: 113,
        right: 114,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 1,
        right: 9287,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 6,
        right: 4125,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 1,
        right: 5754,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 2,
        right: 3179,
    },
    Node {
        feature: 9,
        threshold: 66,
        left: 95,
        right: 96,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 1,
        right: 9698,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 1,
        right: 9364,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 1,
        right: 7181,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 4,
        right: 9425,
    },
    Node {
        feature: 4,
        threshold: 254,
        left: 99,
        right: 100,
    },
    Node {
        feature: 6,
        threshold: 24,
        left: 101,
        right: 102,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 4,
        right: 10000,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 7,
        right: 7054,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 5,
        right: 6365,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 7,
        right: 7371,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 5,
        right: 4848,
    },
    Node {
        feature: 9,
        threshold: 173,
        left: 107,
        right: 108,
    },
    Node {
        feature: 9,
        threshold: 251,
        left: 123,
        right: 124,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 5,
        right: 9845,
    },
    Node {
        feature: 7,
        threshold: 1,
        left: 109,
        right: 110,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 5,
        right: 8718,
    },
    Node {
        feature: 5,
        threshold: 0,
        left: 111,
        right: 112,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 5,
        right: 10000,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 6,
        right: 10000,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 1,
        right: 5687,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 6,
        right: 5035,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 5,
        right: 6891,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 4,
        right: 4893,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 1,
        right: 4695,
    },
    Node {
        feature: 9,
        threshold: 114,
        left: 121,
        right: 122,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 6,
        right: 3699,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 6,
        right: 6633,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 6,
        right: 8061,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 1,
        right: 3396,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 5,
        right: 9999,
    },
    Node {
        feature: 6,
        threshold: 0,
        left: 125,
        right: 126,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 5,
        right: 10000,
    },
    Node {
        feature: -1,
        threshold: 0,
        left: 5,
        right: 9138,
    },
];

#[inline]
fn luma(r: i32, g: i32, b: i32) -> i32 {
    (54 * r + 183 * g + 19 * b) >> 8
}

pub fn block_features(pixels_rgba: &[[i32; 4]; 16]) -> [i32; 10] {
    let mut s = [0i64; 3];
    let mut sq = [0i64; 3];
    let mut mn = [255i32; 4];
    let mut mx = [0i32; 4];
    for p in pixels_rgba.iter() {
        for k in 0..3 {
            let v = p[k] as i64;
            s[k] += v;
            sq[k] += v * v;
        }
        for k in 0..4 {
            if p[k] < mn[k] {
                mn[k] = p[k];
            }
            if p[k] > mx[k] {
                mx[k] = p[k];
            }
        }
    }
    let mut var = 0i64;
    for k in 0..3 {
        var += sq[k] / 16 - (s[k] / 16) * (s[k] / 16);
    }
    let var_rgb = var.clamp(0, 65535) as i32;
    let mdr = mx[0] - mn[0];
    let mdg = mx[1] - mn[1];
    let mdb = mx[2] - mn[2];
    let mda = mx[3] - mn[3];
    let has_alpha = if mn[3] < 255 { 1 } else { 0 };
    let a_min = mn[3];
    let a_max = mx[3];
    let mean_r = (s[0] / 16) as i32;
    let mean_g = (s[1] / 16) as i32;
    let mean_b = (s[2] / 16) as i32;
    let mean_luma = luma(mean_r, mean_g, mean_b);
    let mut qmin = i32::MAX;
    let mut qmax = i32::MIN;
    for qy in 0..2 {
        for qx in 0..2 {
            let mut acc = [0i32; 3];
            for dy in 0..2 {
                for dx in 0..2 {
                    let p = &pixels_rgba[(qy * 2 + dy) * 4 + (qx * 2 + dx)];
                    acc[0] += p[0];
                    acc[1] += p[1];
                    acc[2] += p[2];
                }
            }
            let ql = luma(acc[0] / 4, acc[1] / 4, acc[2] / 4);
            if ql < qmin {
                qmin = ql;
            }
            if ql > qmax {
                qmax = ql;
            }
        }
    }
    let quad_spread = qmax - qmin;
    [
        var_rgb,
        mdr,
        mdg,
        mdb,
        mda,
        has_alpha,
        a_min,
        a_max,
        quad_spread,
        mean_luma,
    ]
}

#[inline]
pub fn predict(feat: &[i32; 10]) -> (u8, u16) {
    let mut idx: i16 = 0;
    loop {
        let node = &TREE[idx as usize];
        if node.feature < 0 {
            return (node.left as u8, node.right as u16);
        }
        let f = feat[node.feature as usize];
        idx = if f <= node.threshold {
            node.left
        } else {
            node.right
        };
    }
}
