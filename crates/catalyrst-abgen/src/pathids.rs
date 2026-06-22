pub const FILE_TYPE_SERIALIZED_ASSET: i32 = 2;

pub const FILE_TYPE_META_ASSET: i32 = 3;

fn md5(message: &[u8]) -> [u8; 16] {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];

    const K: [u32; 64] = [
        0xd76a_a478,
        0xe8c7_b756,
        0x2420_70db,
        0xc1bd_ceee,
        0xf57c_0faf,
        0x4787_c62a,
        0xa830_4613,
        0xfd46_9501,
        0x6980_98d8,
        0x8b44_f7af,
        0xffff_5bb1,
        0x895c_d7be,
        0x6b90_1122,
        0xfd98_7193,
        0xa679_438e,
        0x49b4_0821,
        0xf61e_2562,
        0xc040_b340,
        0x265e_5a51,
        0xe9b6_c7aa,
        0xd62f_105d,
        0x0244_1453,
        0xd8a1_e681,
        0xe7d3_fbc8,
        0x21e1_cde6,
        0xc337_07d6,
        0xf4d5_0d87,
        0x455a_14ed,
        0xa9e3_e905,
        0xfcef_a3f8,
        0x676f_02d9,
        0x8d2a_4c8a,
        0xfffa_3942,
        0x8771_f681,
        0x6d9d_6122,
        0xfde5_380c,
        0xa4be_ea44,
        0x4bde_cfa9,
        0xf6bb_4b60,
        0xbebf_bc70,
        0x289b_7ec6,
        0xeaa1_27fa,
        0xd4ef_3085,
        0x0488_1d05,
        0xd9d4_d039,
        0xe6db_99e5,
        0x1fa2_7cf8,
        0xc4ac_5665,
        0xf429_2244,
        0x432a_ff97,
        0xab94_23a7,
        0xfc93_a039,
        0x655b_59c3,
        0x8f0c_cc92,
        0xffef_f47d,
        0x8584_5dd1,
        0x6fa8_7e4f,
        0xfe2c_e6e0,
        0xa301_4314,
        0x4e08_11a1,
        0xf753_7e82,
        0xbd3a_f235,
        0x2ad7_d2bb,
        0xeb86_d391,
    ];

    let orig_bits = (message.len() as u64).wrapping_mul(8);
    let mut msg = Vec::with_capacity(message.len() + 72);
    msg.extend_from_slice(message);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&orig_bits.to_le_bytes());

    let (mut a0, mut b0, mut c0, mut d0) = (
        0x6745_2301u32,
        0xEFCD_AB89u32,
        0x98BA_DCFEu32,
        0x1032_5476u32,
    );

    let mut m = [0u32; 16];
    for chunk in msg.chunks_exact(64) {
        for (mi, word) in m.iter_mut().zip(chunk.chunks_exact(4)) {
            *mi = u32::from_le_bytes([word[0], word[1], word[2], word[3]]);
        }
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);

        for i in 0..64usize {
            let (f, g) = match i {
                0..=15 => ((b & c) | (!b & d), i),
                16..=31 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                32..=47 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let tmp = d;
            d = c;
            c = b;
            let sum = a.wrapping_add(f).wrapping_add(K[i]).wrapping_add(m[g]);
            b = b.wrapping_add(sum.rotate_left(S[i]));
            a = tmp;
        }

        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }

    let mut out = [0u8; 16];
    out[0..4].copy_from_slice(&a0.to_le_bytes());
    out[4..8].copy_from_slice(&b0.to_le_bytes());
    out[8..12].copy_from_slice(&c0.to_le_bytes());
    out[12..16].copy_from_slice(&d0.to_le_bytes());
    out
}

fn hex16(d: &[u8; 16]) -> String {
    let mut s = String::with_capacity(32);
    for b in d {
        s.push(char::from_digit((b >> 4) as u32, 16).unwrap());
        s.push(char::from_digit((b & 0xf) as u32, 16).unwrap());
    }
    s
}

pub fn asset_guid(content_hash: &str) -> String {
    hex16(&md5(content_hash.as_bytes()))
}

const XXH_P1: u64 = 0x9E37_79B1_85EB_CA87;
const XXH_P2: u64 = 0xC2B2_AE3D_27D4_EB4F;
const XXH_P3: u64 = 0x1656_67B1_9E37_79F9;
const XXH_P4: u64 = 0x85EB_CA77_C2B2_AE63;
const XXH_P5: u64 = 0x27D4_EB2F_1656_67C5;

#[inline(always)]
fn read_u64_le(data: &[u8], i: usize) -> u64 {
    u64::from_le_bytes(data[i..i + 8].try_into().unwrap())
}
#[inline(always)]
fn read_u32_le(data: &[u8], i: usize) -> u32 {
    u32::from_le_bytes(data[i..i + 4].try_into().unwrap())
}

fn xxh64(data: &[u8], seed: u64) -> u64 {
    let n = data.len();
    let mut i = 0usize;
    let mut h: u64;

    #[inline(always)]
    const fn round(acc: u64, k: u64) -> u64 {
        acc.wrapping_add(k.wrapping_mul(XXH_P2))
            .rotate_left(31)
            .wrapping_mul(XXH_P1)
    }

    if n >= 32 {
        let mut v1 = seed.wrapping_add(XXH_P1).wrapping_add(XXH_P2);
        let mut v2 = seed.wrapping_add(XXH_P2);
        let mut v3 = seed;
        let mut v4 = seed.wrapping_sub(XXH_P1);
        while i + 32 <= n {
            v1 = round(v1, read_u64_le(data, i));
            v2 = round(v2, read_u64_le(data, i + 8));
            v3 = round(v3, read_u64_le(data, i + 16));
            v4 = round(v4, read_u64_le(data, i + 24));
            i += 32;
        }
        h = v1
            .rotate_left(1)
            .wrapping_add(v2.rotate_left(7))
            .wrapping_add(v3.rotate_left(12))
            .wrapping_add(v4.rotate_left(18));
        for v in [v1, v2, v3, v4] {
            let v = v.wrapping_mul(XXH_P2).rotate_left(31).wrapping_mul(XXH_P1);
            h = (h ^ v).wrapping_mul(XXH_P1).wrapping_add(XXH_P4);
        }
    } else {
        h = seed.wrapping_add(XXH_P5);
    }

    h = h.wrapping_add(n as u64);
    while i + 8 <= n {
        let k = read_u64_le(data, i)
            .wrapping_mul(XXH_P2)
            .rotate_left(31)
            .wrapping_mul(XXH_P1);
        h = (h ^ k)
            .rotate_left(27)
            .wrapping_mul(XXH_P1)
            .wrapping_add(XXH_P4);
        i += 8;
    }
    if i + 4 <= n {
        let k = (read_u32_le(data, i) as u64).wrapping_mul(XXH_P1);
        h = (h ^ k)
            .rotate_left(23)
            .wrapping_mul(XXH_P2)
            .wrapping_add(XXH_P3);
        i += 4;
    }
    while i < n {
        let k = (data[i] as u64).wrapping_mul(XXH_P5);
        h = (h ^ k).rotate_left(11).wrapping_mul(XXH_P1);
        i += 1;
    }

    h ^= h >> 33;
    h = h.wrapping_mul(XXH_P2);
    h ^= h >> 29;
    h = h.wrapping_mul(XXH_P3);
    h ^= h >> 32;
    h
}

const SC: u64 = 0xDEAD_BEEF_DEAD_BEEF;

pub fn spooky_short(msg: &[u8], seed1: u64, seed2: u64) -> (u64, u64) {
    let n = msg.len();
    let mut rem = n % 32;
    let (mut a, mut b, mut c, mut d) = (seed1, seed2, SC, SC);
    let mut i = 0usize;

    #[inline(always)]
    const fn smix(mut a: u64, mut b: u64, mut c: u64, mut d: u64) -> (u64, u64, u64, u64) {
        c = c.rotate_left(50);
        c = c.wrapping_add(d);
        a ^= c;
        d = d.rotate_left(52);
        d = d.wrapping_add(a);
        b ^= d;
        a = a.rotate_left(30);
        a = a.wrapping_add(b);
        c ^= a;
        b = b.rotate_left(41);
        b = b.wrapping_add(c);
        d ^= b;
        c = c.rotate_left(54);
        c = c.wrapping_add(d);
        a ^= c;
        d = d.rotate_left(48);
        d = d.wrapping_add(a);
        b ^= d;
        a = a.rotate_left(38);
        a = a.wrapping_add(b);
        c ^= a;
        b = b.rotate_left(37);
        b = b.wrapping_add(c);
        d ^= b;
        c = c.rotate_left(62);
        c = c.wrapping_add(d);
        a ^= c;
        d = d.rotate_left(34);
        d = d.wrapping_add(a);
        b ^= d;
        a = a.rotate_left(5);
        a = a.wrapping_add(b);
        c ^= a;
        b = b.rotate_left(36);
        b = b.wrapping_add(c);
        d ^= b;
        (a, b, c, d)
    }

    if n > 15 {
        let endb = (n / 32) * 32;
        while i < endb {
            c = c.wrapping_add(read_u64_le(msg, i));
            d = d.wrapping_add(read_u64_le(msg, i + 8));
            let (na, nb, nc, nd) = smix(a, b, c, d);
            a = na.wrapping_add(read_u64_le(msg, i + 16));
            b = nb.wrapping_add(read_u64_le(msg, i + 24));
            c = nc;
            d = nd;
            i += 32;
        }
        if rem >= 16 {
            c = c.wrapping_add(read_u64_le(msg, i));
            d = d.wrapping_add(read_u64_le(msg, i + 8));
            let (na, nb, nc, nd) = smix(a, b, c, d);
            a = na;
            b = nb;
            c = nc;
            d = nd;
            i += 16;
            rem -= 16;
        }
    }

    d = d.wrapping_add((n as u64) << 56);
    let p = &msg[i..];

    let mut r = rem as isize;
    if r == 15 {
        d = d.wrapping_add((p[14] as u64) << 48);
        r = 14;
    }
    if r == 14 {
        d = d.wrapping_add((p[13] as u64) << 40);
        r = 13;
    }
    if r == 13 {
        d = d.wrapping_add((p[12] as u64) << 32);
        r = 12;
    }
    if r == 12 {
        d = d.wrapping_add(read_u32_le(p, 8) as u64);
        c = c.wrapping_add(read_u64_le(p, 0));
        r = -1;
    }
    if r == 11 {
        d = d.wrapping_add((p[10] as u64) << 16);
        r = 10;
    }
    if r == 10 {
        d = d.wrapping_add((p[9] as u64) << 8);
        r = 9;
    }
    if r == 9 {
        d = d.wrapping_add(p[8] as u64);
        r = 8;
    }
    if r == 8 {
        c = c.wrapping_add(read_u64_le(p, 0));
        r = -1;
    }
    if r == 7 {
        c = c.wrapping_add((p[6] as u64) << 48);
        r = 6;
    }
    if r == 6 {
        c = c.wrapping_add((p[5] as u64) << 40);
        r = 5;
    }
    if r == 5 {
        c = c.wrapping_add((p[4] as u64) << 32);
        r = 4;
    }
    if r == 4 {
        c = c.wrapping_add(read_u32_le(p, 0) as u64);
        r = -1;
    }
    if r == 3 {
        c = c.wrapping_add((p[2] as u64) << 16);
        r = 2;
    }
    if r == 2 {
        c = c.wrapping_add((p[1] as u64) << 8);
        r = 1;
    }
    if r == 1 {
        c = c.wrapping_add(p[0] as u64);
        r = -1;
    }
    if r == 0 {
        c = c.wrapping_add(SC);
        d = d.wrapping_add(SC);
    }

    d ^= c;
    c = c.rotate_left(15);
    d = d.wrapping_add(c);
    a ^= d;
    d = d.rotate_left(52);
    a = a.wrapping_add(d);
    b ^= a;
    a = a.rotate_left(26);
    b = b.wrapping_add(a);
    c ^= b;
    b = b.rotate_left(51);
    c = c.wrapping_add(b);
    d ^= c;
    c = c.rotate_left(28);
    d = d.wrapping_add(c);
    a ^= d;
    d = d.rotate_left(9);
    a = a.wrapping_add(d);
    b ^= a;
    a = a.rotate_left(47);
    b = b.wrapping_add(a);
    c ^= b;
    b = b.rotate_left(54);
    c = c.wrapping_add(b);
    d ^= c;
    c = c.rotate_left(32);
    d = d.wrapping_add(c);
    a ^= d;
    d = d.rotate_left(25);
    a = a.wrapping_add(d);
    b ^= a;
    a = a.rotate_left(63);
    b = b.wrapping_add(a);

    (a, b)
}

fn guid_raw(guid_hex: &str) -> [u8; 16] {
    let h = guid_hex.as_bytes();
    let mut out = [0u8; 16];
    for (i, o) in out.iter_mut().enumerate() {
        let hi = (h[2 * i + 1] as char).to_digit(16).unwrap() as u8;
        let lo = (h[2 * i] as char).to_digit(16).unwrap() as u8;
        *o = (hi << 4) | lo;
    }
    out
}

fn hash128_append(seed: (u64, u64), data: &[u8], block: usize) -> (u64, u64) {
    let (mut s1, mut s2) = seed;
    let mut i = 0;
    while i < data.len() {
        let end = (i + block).min(data.len());
        let (a, b) = spooky_short(&data[i..end], s1, s2);
        s1 = a;
        s2 = b;
        i += block;
    }
    (s1, s2)
}

pub fn prefab_packed_path_id(guid: &str, local_id: i64, file_type: i32) -> i64 {
    let gb = guid_raw(guid);
    let mut s = hash128_append((0, 0), &gb, 4);
    let a1 = s.0;
    s = spooky_short(&local_id.to_le_bytes(), s.0, s.1);
    s = spooky_short(&file_type.to_le_bytes(), s.0, s.1);

    let ab = a1.to_le_bytes();
    let mut ob = s.0.to_le_bytes();
    ob[0] = ab[0];
    ob[1] = ab[1];
    i64::from_le_bytes(ob)
}

pub fn local_id_for_recycle_name(short_type: &str, recycle_name: &str) -> i64 {
    local_id_for_recycle_name_indexed(short_type, recycle_name, 0)
}

pub fn local_id_for_recycle_name_indexed(short_type: &str, recycle_name: &str, index: u32) -> i64 {
    let pre = format!("Type:{short_type}->{recycle_name}{index}");
    xxh64(pre.as_bytes(), 0) as i64
}

pub fn deterministic_sub_asset_path_id(seed: &str, idx: usize) -> i64 {
    let s = format!("{seed}/{idx}");
    let d = md5(s.as_bytes());
    i64::from_le_bytes(d[..8].try_into().unwrap())
}
