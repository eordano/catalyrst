const WIDTH: u32 = 160;
const HEIGHT: u32 = 60;
const ANSWER_RANGE: u32 = 101;

pub fn answer_for_seed(seed: u64) -> f64 {
    (seed % ANSWER_RANGE as u64) as f64
}

pub fn render_png(answer_x: f64) -> Vec<u8> {
    let x = answer_x.max(0.0) as u32;
    let r = (50 + (x * 3) % 180) as u8;
    let g = (90 + (x * 7) % 140) as u8;
    let b = (140 + (x * 5) % 110) as u8;

    let stride = (WIDTH * 3 + 1) as usize;
    let mut raw = Vec::with_capacity(stride * HEIGHT as usize);
    for _y in 0..HEIGHT {
        raw.push(0u8);
        for col in 0..WIDTH {
            if col == x {
                raw.extend_from_slice(&[0xff, 0xff, 0xff]);
            } else {
                raw.extend_from_slice(&[r, g, b]);
            }
        }
    }

    let mut png = Vec::new();
    png.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&WIDTH.to_be_bytes());
    ihdr.extend_from_slice(&HEIGHT.to_be_bytes());
    ihdr.push(8);
    ihdr.push(2);
    ihdr.push(0);
    ihdr.push(0);
    ihdr.push(0);
    write_chunk(&mut png, b"IHDR", &ihdr);

    let idat = zlib_store(&raw);
    write_chunk(&mut png, b"IDAT", &idat);

    write_chunk(&mut png, b"IEND", &[]);
    png
}

fn write_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(kind);
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

fn zlib_store(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(0x78);
    out.push(0x01);

    let mut pos = 0usize;
    while pos < data.len() {
        let chunk = std::cmp::min(0xffff, data.len() - pos);
        let last = pos + chunk >= data.len();
        out.push(if last { 1 } else { 0 });
        let len = chunk as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(&data[pos..pos + chunk]);
        pos += chunk;
    }
    if data.is_empty() {
        out.extend_from_slice(&[1, 0, 0, 0xff, 0xff]);
    }

    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65521;
    let mut a = 1u32;
    let mut b = 0u32;
    for &byte in data {
        a = (a + byte as u32) % MOD;
        b = (b + a) % MOD;
    }
    (b << 16) | a
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xedb8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}
