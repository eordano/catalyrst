// Layout contract (std430): mirrors kernel-ptx/src/core/mips.rs repr(C) structs.
// WGSL has no u64 — Rust u64 fields travel as (lo, hi) u32 pairs, little-endian.
// Array strides: LinItem 24B, PackItem 32B, HalveItem 24B, prefix vec2<u32> 8B.

struct Meta {
    n_items: u32,
    total: u32,
    gid_base: u32,
    pad: u32,
}

struct LinItem {
    base_px_lo: u32,
    base_px_hi: u32,
    pyr_px_lo: u32,
    pyr_px_hi: u32,
    srgb: u32,
    pad: u32,
}

struct PackItem {
    lvl_px_lo: u32,
    lvl_px_hi: u32,
    blk_off_lo: u32,
    blk_off_hi: u32,
    w: u32,
    h: u32,
    srgb: u32,
    pad: u32,
}

struct HalveItem {
    src_px_lo: u32,
    src_px_hi: u32,
    dst_px_lo: u32,
    dst_px_hi: u32,
    w: u32,
    h: u32,
}

@group(0) @binding(0) var<uniform> job: Meta;
@group(0) @binding(1) var<storage, read> lin_items: array<LinItem>;
@group(0) @binding(2) var<storage, read> pack_items: array<PackItem>;
@group(0) @binding(3) var<storage, read> halve_items: array<HalveItem>;
@group(0) @binding(4) var<storage, read> prefixes: array<vec2<u32>>;
@group(0) @binding(5) var<storage, read> base_rgba: array<u32>;
@group(0) @binding(6) var<storage, read_write> pyr: array<f32>;
@group(0) @binding(7) var<storage, read_write> blocks: array<u32>;

// f32 bit tables copied from kernel-ptx/src/core/mips.rs (generated; must match exactly).
const SRGB_TO_LINEAR_U8_BITS = array<u32, 256>(
    0x00000000u, 0x399f22b4u, 0x3a1f22b4u, 0x3a6eb40eu, 0x3a9f22b4u, 0x3ac6eb61u, 0x3aeeb40eu, 0x3b0b3e5du,
    0x3b1f22b4u, 0x3b33070bu, 0x3b46eb61u, 0x3b5b518du, 0x3b70f18du, 0x3b83e1c6u, 0x3b8fe616u, 0x3b9c87fcu,
    0x3ba9c9b7u, 0x3bb7ad6eu, 0x3bc63549u, 0x3bd56361u, 0x3be539c1u, 0x3bf5ba70u, 0x3c0373b5u, 0x3c0c6152u,
    0x3c15a703u, 0x3c1f45beu, 0x3c293e6bu, 0x3c3391f7u, 0x3c3e4149u, 0x3c494d43u, 0x3c54b6c7u, 0x3c607eb2u,
    0x3c6ca5dfu, 0x3c792d22u, 0x3c830aa8u, 0x3c89af9eu, 0x3c9085dbu, 0x3c978dc4u, 0x3c9ec7c2u, 0x3ca63434u,
    0x3cadd37du, 0x3cb5a601u, 0x3cbdac20u, 0x3cc5e639u, 0x3cce54abu, 0x3cd6f7d5u, 0x3cdfd010u, 0x3ce8ddb9u,
    0x3cf2212cu, 0x3cfb9ac1u, 0x3d02a569u, 0x3d0798dcu, 0x3d0ca7e6u, 0x3d11d2afu, 0x3d171962u, 0x3d1c7c2eu,
    0x3d21fb3cu, 0x3d2796b2u, 0x3d2d4ebbu, 0x3d332381u, 0x3d39152bu, 0x3d3f23e4u, 0x3d454fd1u, 0x3d4b991cu,
    0x3d51ffeeu, 0x3d58846au, 0x3d5f26b8u, 0x3d65e6feu, 0x3d6cc564u, 0x3d73c20fu, 0x3d7add29u, 0x3d810b68u,
    0x3d84b795u, 0x3d887330u, 0x3d8c3e4au, 0x3d9018f6u, 0x3d940345u, 0x3d97fd49u, 0x3d9c0716u, 0x3da020bbu,
    0x3da44a4bu, 0x3da883d7u, 0x3daccd70u, 0x3db12728u, 0x3db59112u, 0x3dba0b3au, 0x3dbe95b5u, 0x3dc33092u,
    0x3dc7dbe2u, 0x3dcc97b6u, 0x3dd1641fu, 0x3dd6412cu, 0x3ddb2eefu, 0x3de02d78u, 0x3de53cd5u, 0x3dea5d19u,
    0x3def8e52u, 0x3df4d091u, 0x3dfa23e8u, 0x3dff8861u, 0x3e027f07u, 0x3e054280u, 0x3e080ea3u, 0x3e0ae377u,
    0x3e0dc106u, 0x3e10a754u, 0x3e13966au, 0x3e168e51u, 0x3e198f0fu, 0x3e1c98acu, 0x3e1fab30u, 0x3e22c6a3u,
    0x3e25eb09u, 0x3e29186cu, 0x3e2c4ed1u, 0x3e2f8e41u, 0x3e32d6c5u, 0x3e362861u, 0x3e39831eu, 0x3e3ce702u,
    0x3e405416u, 0x3e43ca5fu, 0x3e4749e4u, 0x3e4ad2aeu, 0x3e4e64c2u, 0x3e520027u, 0x3e55a4e5u, 0x3e595303u,
    0x3e5d0a8bu, 0x3e60cb7cu, 0x3e6495e0u, 0x3e6869bfu, 0x3e6c4720u, 0x3e702e0cu, 0x3e741e84u, 0x3e781890u,
    0x3e7c1c38u, 0x3e8014c2u, 0x3e82203cu, 0x3e84308du, 0x3e8645bau, 0x3e885fc5u, 0x3e8a7eb1u, 0x3e8ca283u,
    0x3e8ecb3du, 0x3e90f8e1u, 0x3e932b74u, 0x3e9562f8u, 0x3e979f71u, 0x3e99e0e2u, 0x3e9c274du, 0x3e9e72b7u,
    0x3ea0c322u, 0x3ea31891u, 0x3ea57308u, 0x3ea7d28au, 0x3eaa3718u, 0x3eaca0b7u, 0x3eaf0f69u, 0x3eb18333u,
    0x3eb3fc18u, 0x3eb67a18u, 0x3eb8fd37u, 0x3ebb8579u, 0x3ebe12e1u, 0x3ec0a571u, 0x3ec33d2du, 0x3ec5da17u,
    0x3ec87c33u, 0x3ecb2383u, 0x3ecdd00bu, 0x3ed081cdu, 0x3ed338ccu, 0x3ed5f50bu, 0x3ed8b68du, 0x3edb7d55u,
    0x3ede4965u, 0x3ee11ac1u, 0x3ee3f16bu, 0x3ee6cd67u, 0x3ee9aeb7u, 0x3eec955du, 0x3eef815du, 0x3ef272bau,
    0x3ef56976u, 0x3ef86594u, 0x3efb6717u, 0x3efe6e01u, 0x3f00bd2du, 0x3f02460eu, 0x3f03d1a7u, 0x3f055ff9u,
    0x3f06f106u, 0x3f0884cfu, 0x3f0a1b55u, 0x3f0bb49bu, 0x3f0d50a0u, 0x3f0eef67u, 0x3f1090f1u, 0x3f12353eu,
    0x3f13dc51u, 0x3f15862bu, 0x3f1732cdu, 0x3f18e239u, 0x3f1a946fu, 0x3f1c4971u, 0x3f1e0141u, 0x3f1fbbdfu,
    0x3f21794du, 0x3f23398du, 0x3f24fca0u, 0x3f26c286u, 0x3f288b42u, 0x3f2a56d4u, 0x3f2c253du, 0x3f2df680u,
    0x3f2fca9fu, 0x3f31a197u, 0x3f337b6cu, 0x3f355820u, 0x3f3737b3u, 0x3f391a26u, 0x3f3aff7cu, 0x3f3ce7b4u,
    0x3f3ed2d2u, 0x3f40c0d5u, 0x3f42b1beu, 0x3f44a590u, 0x3f469c4bu, 0x3f4895f1u, 0x3f4a9282u, 0x3f4c9201u,
    0x3f4e946eu, 0x3f5099cbu, 0x3f52a218u, 0x3f54ad57u, 0x3f56bb8au, 0x3f58ccb1u, 0x3f5ae0cdu, 0x3f5cf7e0u,
    0x3f5f11ecu, 0x3f612eeeu, 0x3f634eefu, 0x3f6571eau, 0x3f6797e3u, 0x3f69c0d6u, 0x3f6beccdu, 0x3f6e1bc0u,
    0x3f704db8u, 0x3f7282afu, 0x3f74baaeu, 0x3f76f5aeu, 0x3f7933b8u, 0x3f7b74c6u, 0x3f7db8e0u, 0x3f800000u
);

const SRGB_U8_LIN_THRESHOLD_BITS = array<u32, 255>(
    0x391f22b3u, 0x39eeb40eu, 0x3a46eb61u, 0x3a8b3e5du, 0x3ab3070bu, 0x3adacfb7u, 0x3b014c32u, 0x3b153089u,
    0x3b2914dfu, 0x3b3cf936u, 0x3b50f2d1u, 0x3b65fb99u, 0x3b7c3404u, 0x3b89d060u, 0x3b962333u, 0x3ba314bdu,
    0x3bb0a731u, 0x3bbedcb6u, 0x3bcdb76cu, 0x3bdd3966u, 0x3bed64aeu, 0x3bfe3b46u, 0x3c07df91u, 0x3c10f918u,
    0x3c1a6b33u, 0x3c2436c8u, 0x3c2e5cc8u, 0x3c38de1au, 0x3c43bba4u, 0x3c4ef647u, 0x3c5a8ee1u, 0x3c668653u,
    0x3c72dd73u, 0x3c7f9511u, 0x3c865703u, 0x3c8d1490u, 0x3c940395u, 0x3c9b247bu, 0x3ca277a8u, 0x3ca9fd79u,
    0x3cb1b654u, 0x3cb9a29au, 0x3cc1c2aau, 0x3cca16e3u, 0x3cd29fa4u, 0x3cdb5d4du, 0x3ce45033u, 0x3ced78b6u,
    0x3cf6d72fu, 0x3d0035fcu, 0x3d051bb6u, 0x3d0a1ceeu, 0x3d0f39d1u, 0x3d14728au, 0x3d19c745u, 0x3d1f382bu,
    0x3d24c56bu, 0x3d2a6f25u, 0x3d303587u, 0x3d3618b9u, 0x3d3c18e5u, 0x3d423633u, 0x3d4870cbu, 0x3d4ec8d5u,
    0x3d553e75u, 0x3d5bd1d5u, 0x3d62831au, 0x3d69526bu, 0x3d703fefu, 0x3d774bcfu, 0x3d7e7628u, 0x3d82df93u,
    0x3d869374u, 0x3d8a56cbu, 0x3d8e29acu, 0x3d920c27u, 0x3d95fe4fu, 0x3d9a0036u, 0x3d9e11ecu, 0x3da23384u,
    0x3da66510u, 0x3daaa6a1u, 0x3daef847u, 0x3db35a17u, 0x3db7cc1du, 0x3dbc4e6bu, 0x3dc0e115u, 0x3dc58429u,
    0x3dca37b9u, 0x3dcefbd6u, 0x3dd3d090u, 0x3dd8b5f6u, 0x3dddac19u, 0x3de2b30au, 0x3de7cad9u, 0x3decf396u,
    0x3df22d50u, 0x3df7781au, 0x3dfcd3feu, 0x3e012088u, 0x3e03dfafu, 0x3e06a77cu, 0x3e0977f7u, 0x3e0c5127u,
    0x3e0f3314u, 0x3e121dc6u, 0x3e151144u, 0x3e180d95u, 0x3e1b12c2u, 0x3e1e20d1u, 0x3e2137cau, 0x3e2457b7u,
    0x3e27809au, 0x3e2ab27cu, 0x3e2ded67u, 0x3e313160u, 0x3e347e6fu, 0x3e37d49du, 0x3e3b33edu, 0x3e3e9c68u,
    0x3e420e14u, 0x3e4588fau, 0x3e490d21u, 0x3e4c9a8fu, 0x3e50314bu, 0x3e53d15cu, 0x3e577acau, 0x3e5b2d9au,
    0x3e5ee9d4u, 0x3e62af7du, 0x3e667e9fu, 0x3e6a5742u, 0x3e6e3965u, 0x3e722512u, 0x3e761a53u, 0x3e7a192cu,
    0x3e7e21a5u, 0x3e8119e2u, 0x3e8327c7u, 0x3e853a88u, 0x3e875223u, 0x3e896e9fu, 0x3e8b8ffdu, 0x3e8db642u,
    0x3e8fe171u, 0x3e92118cu, 0x3e944697u, 0x3e968095u, 0x3e98bf89u, 0x3e9b0377u, 0x3e9d4c62u, 0x3e9f9a4cu,
    0x3ea1ed38u, 0x3ea4452bu, 0x3ea6a226u, 0x3ea9042eu, 0x3eab6b44u, 0x3eadd76cu, 0x3eb048aau, 0x3eb2bf02u,
    0x3eb53a71u, 0x3eb7bb00u, 0x3eba40b1u, 0x3ebccb85u, 0x3ebf5b81u, 0x3ec1f0a6u, 0x3ec48af9u, 0x3ec72a7cu,
    0x3ec9cf32u, 0x3ecc791du, 0x3ecf2842u, 0x3ed1dca2u, 0x3ed49641u, 0x3ed75521u, 0x3eda1945u, 0x3edce2b1u,
    0x3edfb168u, 0x3ee2856au, 0x3ee55ebdu, 0x3ee83d62u, 0x3eeb215du, 0x3eee0ab0u, 0x3ef0f95eu, 0x3ef3ed6au,
    0x3ef6e6d7u, 0x3ef9e5a5u, 0x3efce9dfu, 0x3efff37eu, 0x3f018145u, 0x3f030b82u, 0x3f049877u, 0x3f062827u,
    0x3f07ba91u, 0x3f094fbau, 0x3f0ae79fu, 0x3f0c8245u, 0x3f0e1faau, 0x3f0fbfd2u, 0x3f1162beu, 0x3f13086eu,
    0x3f14b0e4u, 0x3f165c22u, 0x3f180a29u, 0x3f19baf9u, 0x3f1b6e96u, 0x3f1d24feu, 0x3f1ede35u, 0x3f209a3cu,
    0x3f225912u, 0x3f241abbu, 0x3f25df37u, 0x3f27a688u, 0x3f2970aeu, 0x3f2b3dabu, 0x3f2d0d83u, 0x3f2ee033u,
    0x3f30b5bdu, 0x3f328e24u, 0x3f346968u, 0x3f36478cu, 0x3f38288fu, 0x3f3a0c73u, 0x3f3bf33au, 0x3f3ddce5u,
    0x3f3fc974u, 0x3f41b8eau, 0x3f43ab48u, 0x3f45a08eu, 0x3f4798bfu, 0x3f4993dau, 0x3f4b91e2u, 0x3f4d92d8u,
    0x3f4f96bdu, 0x3f519d91u, 0x3f53a757u, 0x3f55b410u, 0x3f57c3bdu, 0x3f59d65eu, 0x3f5bebf6u, 0x3f5e0485u,
    0x3f60200du, 0x3f623e90u, 0x3f64600bu, 0x3f668486u, 0x3f68abfau, 0x3f6ad671u, 0x3f6d03e3u, 0x3f6f345bu,
    0x3f7167d0u, 0x3f739e4du, 0x3f75d7cbu, 0x3f781452u, 0x3f7a53dbu, 0x3f7c9671u, 0x3f7edc0eu
);

fn srgb_to_linear_u8(c: u32) -> f32 {
    return bitcast<f32>(SRGB_TO_LINEAR_U8_BITS[c]);
}

// mips.rs linear_to_srgb_u8: lin <= 0.0 or NaN -> 0; lin >= 1.0 -> 255; else
// partition_point over the threshold table. Positive-float order == u32 bit order.
fn linear_to_srgb_u8(lin: f32) -> u32 {
    let bits = bitcast<u32>(lin);
    if ((bits & 0x80000000u) != 0u || bits == 0u || bits > 0x7f800000u) {
        return 0u;
    }
    if (bits >= 0x3f800000u) {
        return 255u;
    }
    var lo = 0u;
    var hi = 255u;
    loop {
        if (lo >= hi) {
            break;
        }
        let mid = (lo + hi) / 2u;
        if (SRGB_U8_LIN_THRESHOLD_BITS[mid] <= bits) {
            lo = mid + 1u;
        } else {
            hi = mid;
        }
    }
    return lo;
}

fn round_half_up_u8(v: f32) -> u32 {
    let r = floor(v + 0.5);
    if (r <= 0.0) {
        return 0u;
    }
    if (r >= 255.0) {
        return 255u;
    }
    return u32(r);
}

fn u64_le_u32(p: vec2<u32>, gid: u32) -> bool {
    if (p.y != 0u) {
        return false;
    }
    return p.x <= gid;
}

// kernel-ptx/src/lib.rs find_item: binary search over the u64 prefix array.
fn find_item(n_items: u32, gid: u32) -> u32 {
    var lo = 0u;
    var hi = n_items;
    loop {
        if (hi - lo <= 1u) {
            break;
        }
        let mid = (lo + hi) / 2u;
        if (u64_le_u32(prefixes[mid], gid)) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    return lo;
}

fn quantize_pixel_word(px: vec4<f32>, srgb: u32) -> u32 {
    var r: u32;
    var g: u32;
    var b: u32;
    if (srgb != 0u) {
        r = linear_to_srgb_u8(px.x);
        g = linear_to_srgb_u8(px.y);
        b = linear_to_srgb_u8(px.z);
    } else {
        r = round_half_up_u8(px.x);
        g = round_half_up_u8(px.y);
        b = round_half_up_u8(px.z);
    }
    let a = round_half_up_u8(px.w);
    return r | (g << 8u) | (b << 16u) | (a << 24u);
}

@compute @workgroup_size(256)
fn blockify_linearize(@builtin(global_invocation_id) giv: vec3<u32>) {
    let gid = job.gid_base + giv.x;
    if (gid >= job.total) {
        return;
    }
    let idx = find_item(job.n_items, gid);
    let it = lin_items[idx];
    let p = gid - prefixes[idx].x;
    let word = base_rgba[it.base_px_lo + p];
    let d = (it.pyr_px_lo + p) * 4u;
    if (it.srgb != 0u) {
        pyr[d] = srgb_to_linear_u8(word & 0xffu);
        pyr[d + 1u] = srgb_to_linear_u8((word >> 8u) & 0xffu);
        pyr[d + 2u] = srgb_to_linear_u8((word >> 16u) & 0xffu);
    } else {
        pyr[d] = f32(word & 0xffu);
        pyr[d + 1u] = f32((word >> 8u) & 0xffu);
        pyr[d + 2u] = f32((word >> 16u) & 0xffu);
    }
    pyr[d + 3u] = f32((word >> 24u) & 0xffu);
}

@compute @workgroup_size(256)
fn blockify_halve(@builtin(global_invocation_id) giv: vec3<u32>) {
    let gid = job.gid_base + giv.x;
    if (gid >= job.total) {
        return;
    }
    let idx = find_item(job.n_items, gid);
    let it = halve_items[idx];
    let np = gid - prefixes[idx].x;
    let w = it.w;
    let h = it.h;
    let nw = max(w / 2u, 1u);
    let nx = np % nw;
    let ny = np / nw;
    var fh = 1u;
    if (h > 1u) {
        fh = 2u;
    }
    var fw = 1u;
    if (w > 1u) {
        fw = 2u;
    }
    // denom is 1/2/4 (a power of two): multiplying by the exact reciprocal is
    // bit-identical to the corelib f32 division and spec-exact in WGSL
    // (mul is correctly rounded; div carries 2.5 ULP slack).
    var inv = 1.0;
    if (fh * fw == 2u) {
        inv = 0.5;
    } else if (fh * fw == 4u) {
        inv = 0.25;
    }
    let sbase = it.src_px_lo * 4u;
    let d = (it.dst_px_lo + np) * 4u;
    for (var ch = 0u; ch < 4u; ch = ch + 1u) {
        var acc = 0.0;
        for (var dy = 0u; dy < fh; dy = dy + 1u) {
            for (var dx = 0u; dx < fw; dx = dx + 1u) {
                let y = ny * fh + dy;
                let x = nx * fw + dx;
                acc = acc + pyr[sbase + (y * w + x) * 4u + ch];
            }
        }
        pyr[d + ch] = acc * inv;
    }
}

@compute @workgroup_size(256)
fn blockify_quantize_pack(@builtin(global_invocation_id) giv: vec3<u32>) {
    let gid = job.gid_base + giv.x;
    if (gid >= job.total) {
        return;
    }
    let idx = find_item(job.n_items, gid);
    let it = pack_items[idx];
    let lb = gid - prefixes[idx].x;
    let w = it.w;
    let h = it.h;
    let bw = (w + 3u) / 4u;
    let bx = lb % bw;
    let by = lb / bw;
    let lbase = it.lvl_px_lo * 4u;
    let obase = (it.blk_off_lo + lb) * 16u;
    for (var r = 0u; r < 4u; r = r + 1u) {
        let sy = (by * 4u + r) % h;
        for (var dx = 0u; dx < 4u; dx = dx + 1u) {
            let sx = (bx * 4u + dx) % w;
            let s = lbase + (sy * w + sx) * 4u;
            let px = vec4<f32>(pyr[s], pyr[s + 1u], pyr[s + 2u], pyr[s + 3u]);
            blocks[obase + r * 4u + dx] = quantize_pixel_word(px, it.srgb);
        }
    }
}
