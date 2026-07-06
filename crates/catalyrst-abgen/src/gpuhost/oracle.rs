struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Lcg { state: seed }
    }

    fn next_byte(&mut self) -> u8 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.state >> 33) as u8
    }
}

fn solid(r: u8, g: u8, b: u8, a: u8) -> [u8; 64] {
    let mut blk = [0u8; 64];
    for i in 0..16 {
        blk[i * 4] = r;
        blk[i * 4 + 1] = g;
        blk[i * 4 + 2] = b;
        blk[i * 4 + 3] = a;
    }
    blk
}

fn varied_color(a: u8) -> [u8; 64] {
    let mut blk = [0u8; 64];
    for i in 0..16 {
        blk[i * 4] = (i * 16) as u8;
        blk[i * 4 + 1] = (255 - i * 16) as u8;
        blk[i * 4 + 2] = (i * 8 + 64) as u8;
        blk[i * 4 + 3] = a;
    }
    blk
}

fn two_color<F: Fn(usize, usize) -> bool>(ca: [u8; 4], cb: [u8; 4], pick_a: F) -> [u8; 64] {
    let mut blk = [0u8; 64];
    for y in 0..4 {
        for x in 0..4 {
            let i = y * 4 + x;
            let c = if pick_a(x, y) { ca } else { cb };
            blk[i * 4] = c[0];
            blk[i * 4 + 1] = c[1];
            blk[i * 4 + 2] = c[2];
            blk[i * 4 + 3] = c[3];
        }
    }
    blk
}

fn quadrants() -> [u8; 64] {
    let colors = [
        [255u8, 0, 0, 255],
        [0u8, 255, 0, 255],
        [0u8, 0, 255, 255],
        [255u8, 255, 0, 255],
    ];
    let mut blk = [0u8; 64];
    for y in 0..4 {
        for x in 0..4 {
            let i = y * 4 + x;
            let q = (if y < 2 { 0 } else { 2 }) + (if x < 2 { 0 } else { 1 });
            let c = colors[q];
            blk[i * 4] = c[0];
            blk[i * 4 + 1] = c[1];
            blk[i * 4 + 2] = c[2];
            blk[i * 4 + 3] = c[3];
        }
    }
    blk
}

fn gradient() -> [u8; 64] {
    let mut blk = [0u8; 64];
    for y in 0..4 {
        for x in 0..4 {
            let i = y * 4 + x;
            blk[i * 4] = (x * 85) as u8;
            blk[i * 4 + 1] = (y * 85) as u8;
            blk[i * 4 + 2] = ((x + y) * 42) as u8;
            blk[i * 4 + 3] = 255;
        }
    }
    blk
}

fn checkerboard() -> [u8; 64] {
    two_color([0, 0, 0, 255], [255, 255, 255, 255], |x, y| {
        (x + y) % 2 == 0
    })
}

fn near_solid_noise() -> [u8; 64] {
    let mut blk = [0u8; 64];
    for i in 0..16 {
        for c in 0..3 {
            let d = ((i * 4 + c) % 3) as i32 - 1;
            blk[i * 4 + c] = (128 + d) as u8;
        }
        blk[i * 4 + 3] = 255;
    }
    blk
}

fn curated_blocks() -> Vec<[u8; 64]> {
    vec![
        solid(0, 0, 0, 255),
        solid(255, 255, 255, 255),
        solid(128, 128, 128, 255),
        solid(255, 0, 0, 255),
        solid(0, 255, 0, 255),
        solid(0, 0, 255, 255),
        varied_color(0),
        varied_color(255),
        two_color([255, 0, 0, 255], [0, 0, 255, 255], |x, _y| x < 2),
        two_color([255, 0, 0, 255], [0, 0, 255, 255], |_x, y| y < 2),
        two_color([255, 0, 0, 255], [0, 0, 255, 255], |x, y| x + y < 4),
        quadrants(),
        gradient(),
        checkerboard(),
        near_solid_noise(),
    ]
}

pub fn set_oracle_scalar(on: bool) {
    if on {
        std::env::set_var("ABGEN_BC7_SCALAR", "1");
    } else {
        std::env::remove_var("ABGEN_BC7_SCALAR");
    }
}

pub fn gen_blocks(seed: u64, n: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(n * 64);
    let curated = curated_blocks();
    for blk in curated.iter().take(n) {
        out.extend_from_slice(blk);
    }
    let mut lcg = Lcg::new(seed);
    while out.len() < n * 64 {
        out.push(lcg.next_byte());
    }
    out
}

pub fn gen_texture(seed: u64, w: u32, h: u32) -> Vec<u8> {
    let len = w as usize * h as usize * 4;
    let mut lcg = Lcg::new(seed);
    let mut out = Vec::with_capacity(len);
    for _ in 0..len {
        out.push(lcg.next_byte());
    }
    out
}

pub fn oracle_bc7(
    blocks: &[u8],
    num_blocks: usize,
    profile: crate::bc7_pure::Bc7Profile,
    perceptual: bool,
) -> Vec<u8> {
    let params = match profile {
        crate::bc7_pure::Bc7Profile::Slow => crate::bc7_pure::Params::slow(perceptual),
        crate::bc7_pure::Bc7Profile::Basic => crate::bc7_pure::Params::basic(perceptual),
    };
    crate::bc7_pure::encode_blocks(blocks, num_blocks, &params)
}

pub fn oracle_bc7_mip_chain(
    rgba: &[u8],
    width: u32,
    height: u32,
    mip_count: Option<i32>,
    flip: bool,
    srgb: bool,
) -> (Vec<u8>, i32) {
    crate::bc7_pure::encode_rgba32_mip_chain(rgba, width, height, mip_count, flip, srgb)
}

pub fn oracle_dxt1_mip_chain(
    rgba: &[u8],
    width: u32,
    height: u32,
    mip_count: Option<i32>,
    flip: bool,
    srgb: bool,
) -> (Vec<u8>, i32) {
    crate::dxt1_pure::encode_dxt1_mip_chain(rgba, width, height, mip_count, flip, srgb)
}

pub fn oracle_bc5_mip_chain(
    rgba: &[u8],
    width: u32,
    height: u32,
    mip_count: Option<i32>,
    flip: bool,
) -> (Vec<u8>, i32) {
    crate::bc5_pure::encode_bc5_mip_chain(rgba, width, height, mip_count, flip)
}
