use std::fmt;

#[derive(Debug)]
#[non_exhaustive]
pub enum PngError {
    BadSignature,
    Truncated,
    BadChunk(&'static str),
    Unsupported(String),
    Inflate(String),
}

impl fmt::Display for PngError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PngError::BadSignature => write!(f, "bad PNG signature"),
            PngError::Truncated => write!(f, "truncated PNG data"),
            PngError::BadChunk(s) => write!(f, "bad/invalid chunk: {s}"),
            PngError::Unsupported(s) => write!(f, "unsupported PNG feature: {s}"),
            PngError::Inflate(s) => write!(f, "inflate error: {s}"),
        }
    }
}

impl std::error::Error for PngError {}

type Result<T> = std::result::Result<T, PngError>;

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bitbuf: u32,
    bitcnt: u32,
}

impl<'a> BitReader<'a> {
    const fn new(data: &'a [u8]) -> Self {
        BitReader {
            data,
            pos: 0,
            bitbuf: 0,
            bitcnt: 0,
        }
    }

    #[inline]
    fn ensure(&mut self, need: u32) -> Result<()> {
        while self.bitcnt < need {
            if self.pos >= self.data.len() {
                return Err(PngError::Inflate("unexpected end of stream".into()));
            }
            self.bitbuf |= (self.data[self.pos] as u32) << self.bitcnt;
            self.pos += 1;
            self.bitcnt += 8;
        }
        Ok(())
    }

    #[inline]
    fn get_bits(&mut self, n: u32) -> Result<u32> {
        if n == 0 {
            return Ok(0);
        }
        self.ensure(n)?;
        let v = self.bitbuf & ((1u32 << n) - 1);
        self.bitbuf >>= n;
        self.bitcnt -= n;
        Ok(v)
    }

    #[inline]
    const fn align_byte(&mut self) {
        let drop = self.bitcnt & 7;
        self.bitbuf >>= drop;
        self.bitcnt -= drop;
    }
}

struct Huffman {
    counts: [u16; 16],

    symbols: Vec<u16>,
}

impl Huffman {
    fn new(lengths: &[u8]) -> Huffman {
        let mut counts = [0u16; 16];
        for &l in lengths {
            counts[l as usize] += 1;
        }
        counts[0] = 0;

        let mut offsets = [0u16; 16];
        let mut sum = 0u16;
        for len in 1..16 {
            offsets[len] = sum;
            sum += counts[len];
        }
        let mut symbols = vec![0u16; sum as usize];
        for (sym, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbols[offsets[l as usize] as usize] = sym as u16;
                offsets[l as usize] += 1;
            }
        }
        Huffman { counts, symbols }
    }

    #[inline]
    fn decode(&self, br: &mut BitReader) -> Result<u16> {
        let mut code: i32 = 0;
        let mut first: i32 = 0;
        let mut index: i32 = 0;
        for len in 1..16 {
            code |= br.get_bits(1)? as i32;
            let count = self.counts[len] as i32;
            if code - first < count {
                return Ok(self.symbols[(index + (code - first)) as usize]);
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        Err(PngError::Inflate("invalid huffman code".into()))
    }
}

const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

fn inflate_block_huffman(
    br: &mut BitReader,
    out: &mut Vec<u8>,
    lit: &Huffman,
    dist: &Huffman,
) -> Result<()> {
    loop {
        let sym = lit.decode(br)?;
        if sym == 256 {
            break;
        }
        if sym < 256 {
            out.push(sym as u8);
        } else {
            let s = (sym - 257) as usize;
            if s >= 29 {
                return Err(PngError::Inflate("invalid length symbol".into()));
            }
            let length = LEN_BASE[s] as usize + br.get_bits(LEN_EXTRA[s] as u32)? as usize;
            let dsym = dist.decode(br)? as usize;
            if dsym >= 30 {
                return Err(PngError::Inflate("invalid distance symbol".into()));
            }
            let distance =
                DIST_BASE[dsym] as usize + br.get_bits(DIST_EXTRA[dsym] as u32)? as usize;
            if distance > out.len() {
                return Err(PngError::Inflate("distance too far back".into()));
            }
            let start = out.len() - distance;
            for i in 0..length {
                let b = out[start + i];
                out.push(b);
            }
        }
    }
    Ok(())
}

fn build_fixed_huffman() -> (Huffman, Huffman) {
    let mut lit_lengths = [0u8; 288];
    for i in 0..144 {
        lit_lengths[i] = 8;
    }
    for i in 144..256 {
        lit_lengths[i] = 9;
    }
    for i in 256..280 {
        lit_lengths[i] = 7;
    }
    for i in 280..288 {
        lit_lengths[i] = 8;
    }
    let dist_lengths = [5u8; 30];
    (Huffman::new(&lit_lengths), Huffman::new(&dist_lengths))
}

const CLEN_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

fn read_dynamic_huffman(br: &mut BitReader) -> Result<(Huffman, Huffman)> {
    let hlit = br.get_bits(5)? as usize + 257;
    let hdist = br.get_bits(5)? as usize + 1;
    let hclen = br.get_bits(4)? as usize + 4;

    let mut cl_lengths = [0u8; 19];
    for i in 0..hclen {
        cl_lengths[CLEN_ORDER[i]] = br.get_bits(3)? as u8;
    }
    let cl_huff = Huffman::new(&cl_lengths);

    let total = hlit + hdist;
    let mut lengths = vec![0u8; total];
    let mut i = 0;
    while i < total {
        let sym = cl_huff.decode(br)?;
        match sym {
            0..=15 => {
                lengths[i] = sym as u8;
                i += 1;
            }
            16 => {
                if i == 0 {
                    return Err(PngError::Inflate("repeat with no previous length".into()));
                }
                let prev = lengths[i - 1];
                let rep = br.get_bits(2)? as usize + 3;
                for _ in 0..rep {
                    if i >= total {
                        return Err(PngError::Inflate("length repeat overflow".into()));
                    }
                    lengths[i] = prev;
                    i += 1;
                }
            }
            17 => {
                let rep = br.get_bits(3)? as usize + 3;
                for _ in 0..rep {
                    if i >= total {
                        return Err(PngError::Inflate("zero repeat overflow".into()));
                    }
                    lengths[i] = 0;
                    i += 1;
                }
            }
            18 => {
                let rep = br.get_bits(7)? as usize + 11;
                for _ in 0..rep {
                    if i >= total {
                        return Err(PngError::Inflate("zero repeat overflow".into()));
                    }
                    lengths[i] = 0;
                    i += 1;
                }
            }
            _ => return Err(PngError::Inflate("invalid code-length symbol".into())),
        }
    }

    let lit = Huffman::new(&lengths[..hlit]);
    let dist = Huffman::new(&lengths[hlit..]);
    Ok((lit, dist))
}

fn inflate(data: &[u8], expected: usize) -> Result<Vec<u8>> {
    let cap = expected.saturating_add(expected / 16).max(expected + 4096);
    let mut br = BitReader::new(data);
    let mut out: Vec<u8> = Vec::with_capacity(expected);
    loop {
        if out.len() > cap {
            return Err(PngError::Inflate(format!(
                "inflated size exceeds expected by >6% (cap {cap}, hostile/malformed input)"
            )));
        }
        let bfinal = br.get_bits(1)?;
        let btype = br.get_bits(2)?;
        match btype {
            0 => {
                br.align_byte();
                br.ensure(0)?;

                let len = read_u16_le(&mut br)?;
                let nlen = read_u16_le(&mut br)?;
                if len != !nlen {
                    return Err(PngError::Inflate("stored block LEN/NLEN mismatch".into()));
                }
                for _ in 0..len {
                    out.push(read_byte(&mut br)?);
                }
            }
            1 => {
                let (lit, dist) = build_fixed_huffman();
                inflate_block_huffman(&mut br, &mut out, &lit, &dist)?;
            }
            2 => {
                let (lit, dist) = read_dynamic_huffman(&mut br)?;
                inflate_block_huffman(&mut br, &mut out, &lit, &dist)?;
            }
            _ => return Err(PngError::Inflate("invalid block type".into())),
        }
        if bfinal == 1 {
            break;
        }
    }
    Ok(out)
}

#[inline]
fn read_byte(br: &mut BitReader) -> Result<u8> {
    Ok(br.get_bits(8)? as u8)
}

#[inline]
fn read_u16_le(br: &mut BitReader) -> Result<u16> {
    let lo = read_byte(br)? as u16;
    let hi = read_byte(br)? as u16;
    Ok(lo | (hi << 8))
}

fn zlib_inflate(data: &[u8], expected: usize) -> Result<Vec<u8>> {
    if data.len() < 2 {
        return Err(PngError::Inflate("zlib stream too short".into()));
    }
    let cmf = data[0];
    let flg = data[1];
    let cm = cmf & 0x0f;
    if cm != 8 {
        return Err(PngError::Inflate(format!(
            "unsupported compression method {cm}"
        )));
    }
    if !((cmf as u16) << 8 | flg as u16).is_multiple_of(31) {
        return Err(PngError::Inflate("bad zlib header check".into()));
    }
    let fdict = (flg & 0x20) != 0;
    let mut start = 2;
    if fdict {
        start += 4;
    }
    if data.len() < start {
        return Err(PngError::Inflate(
            "truncated zlib stream (header overruns input)".into(),
        ));
    }
    inflate(&data[start..], expected)
}

const PNG_SIG: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];

struct Ihdr {
    width: u32,
    height: u32,
    bit_depth: u8,
    color_type: u8,
    interlace: u8,
}

#[inline]
fn be_u32(b: &[u8]) -> u32 {
    ((b[0] as u32) << 24) | ((b[1] as u32) << 16) | ((b[2] as u32) << 8) | (b[3] as u32)
}

pub fn decode_rgba8(png_bytes: &[u8]) -> Result<(u32, u32, Vec<u8>)> {
    if png_bytes.len() < 8 || png_bytes[..8] != PNG_SIG {
        return Err(PngError::BadSignature);
    }

    let mut pos = 8usize;
    let mut ihdr: Option<Ihdr> = None;
    let mut idat: Vec<u8> = Vec::new();
    let mut plte: Option<Vec<u8>> = None;
    let mut trns: Option<Vec<u8>> = None;

    while pos + 8 <= png_bytes.len() {
        let len = be_u32(&png_bytes[pos..pos + 4]) as usize;
        let ctype = &png_bytes[pos + 4..pos + 8];
        let dstart = pos + 8;
        if dstart + len + 4 > png_bytes.len() {
            return Err(PngError::Truncated);
        }
        let data = &png_bytes[dstart..dstart + len];

        match ctype {
            b"IHDR" => {
                if len != 13 {
                    return Err(PngError::BadChunk("IHDR length"));
                }
                let width = be_u32(&data[0..4]);
                let height = be_u32(&data[4..8]);
                let bit_depth = data[8];
                let color_type = data[9];
                let compression = data[10];
                let filter = data[11];
                let interlace = data[12];
                if compression != 0 {
                    return Err(PngError::Unsupported("compression method".into()));
                }
                if filter != 0 {
                    return Err(PngError::Unsupported("filter method".into()));
                }
                if interlace > 1 {
                    return Err(PngError::Unsupported("interlace method".into()));
                }
                ihdr = Some(Ihdr {
                    width,
                    height,
                    bit_depth,
                    color_type,
                    interlace,
                });
            }
            b"PLTE" => {
                plte = Some(data.to_vec());
            }
            b"tRNS" => {
                trns = Some(data.to_vec());
            }
            b"IDAT" => {
                idat.extend_from_slice(data);
            }
            b"IEND" => break,
            _ => {}
        }

        pos = dstart + len + 4;
    }

    let ihdr = ihdr.ok_or(PngError::BadChunk("missing IHDR"))?;
    if ihdr.width == 0 || ihdr.height == 0 {
        return Err(PngError::BadChunk("zero dimension"));
    }

    const MAX_PIXELS: u64 = 64 * 1024 * 1024;
    let pixels = ihdr.width as u64 * ihdr.height as u64;
    if pixels > MAX_PIXELS {
        return Err(PngError::Unsupported(format!(
            "image dimensions {}x{} exceed MAX_PIXELS ({MAX_PIXELS})",
            ihdr.width, ihdr.height
        )));
    }

    let channels = match ihdr.color_type {
        0 => 1,
        2 => 3,
        3 => 1,
        4 => 2,
        6 => 4,
        _ => {
            return Err(PngError::Unsupported(format!(
                "color type {}",
                ihdr.color_type
            )))
        }
    };
    match ihdr.bit_depth {
        1 | 2 | 4 | 8 | 16 => {}
        _ => {
            return Err(PngError::Unsupported(format!(
                "bit depth {}",
                ihdr.bit_depth
            )))
        }
    }

    match ihdr.color_type {
        0 => {}
        3 => {
            if ihdr.bit_depth == 16 {
                return Err(PngError::Unsupported("palette 16-bit".into()));
            }
        }
        2 | 4 | 6 => {
            if ihdr.bit_depth != 8 && ihdr.bit_depth != 16 {
                return Err(PngError::Unsupported("color type requires 8/16-bit".into()));
            }
        }
        _ => unreachable!(),
    }

    let expected = estimate_raw_size(&ihdr, channels);
    let raw = zlib_inflate(&idat, expected)?;

    let rgba = if ihdr.interlace == 0 {
        let bpp = bytes_per_pixel(ihdr.bit_depth, channels);
        let rowbytes = row_bytes(ihdr.width, ihdr.bit_depth, channels);
        let unfiltered = unfilter(&raw, ihdr.height as usize, rowbytes, bpp)?;
        to_rgba8(
            &unfiltered,
            ihdr.width,
            ihdr.height,
            rowbytes,
            &ihdr,
            plte.as_deref(),
            trns.as_deref(),
        )?
    } else {
        decode_adam7(&raw, &ihdr, channels, plte.as_deref(), trns.as_deref())?
    };

    Ok((ihdr.width, ihdr.height, rgba))
}

#[inline]
fn bytes_per_pixel(bit_depth: u8, channels: usize) -> usize {
    let bits = bit_depth as usize * channels;
    bits.div_ceil(8).max(1)
}

#[inline]
const fn row_bytes(width: u32, bit_depth: u8, channels: usize) -> usize {
    let bits = width as usize * channels * bit_depth as usize;
    bits.div_ceil(8)
}

fn estimate_raw_size(ihdr: &Ihdr, channels: usize) -> usize {
    if ihdr.interlace == 0 {
        let rb = row_bytes(ihdr.width, ihdr.bit_depth, channels);
        (rb + 1) * ihdr.height as usize
    } else {
        let mut total = 0usize;
        for p in 0..7 {
            let (pw, ph) = adam7_pass_dims(ihdr.width, ihdr.height, p);
            if pw == 0 || ph == 0 {
                continue;
            }
            let rb = row_bytes(pw, ihdr.bit_depth, channels);
            total += (rb + 1) * ph as usize;
        }
        total
    }
}

#[inline]
const fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = a as i32 + b as i32 - c as i32;
    let pa = (p - a as i32).abs();
    let pb = (p - b as i32).abs();
    let pc = (p - c as i32).abs();
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

fn unfilter(raw: &[u8], height: usize, rowbytes: usize, bpp: usize) -> Result<Vec<u8>> {
    let stride = rowbytes + 1;
    if raw.len() < stride * height {
        return Err(PngError::Inflate(
            "inflated data shorter than expected".into(),
        ));
    }
    let mut out = vec![0u8; rowbytes * height];
    for y in 0..height {
        let filter = raw[y * stride];
        let src = &raw[y * stride + 1..y * stride + 1 + rowbytes];

        let (prev_rows, cur_rows) = out.split_at_mut(y * rowbytes);
        let cur = &mut cur_rows[..rowbytes];
        let prev: &[u8] = if y == 0 {
            &[]
        } else {
            &prev_rows[(y - 1) * rowbytes..y * rowbytes]
        };
        match filter {
            0 => {
                cur.copy_from_slice(src);
            }
            1 => {
                for i in 0..rowbytes {
                    let a = if i >= bpp { cur[i - bpp] } else { 0 };
                    cur[i] = src[i].wrapping_add(a);
                }
            }
            2 => {
                for i in 0..rowbytes {
                    let b = if y == 0 { 0 } else { prev[i] };
                    cur[i] = src[i].wrapping_add(b);
                }
            }
            3 => {
                for i in 0..rowbytes {
                    let a = if i >= bpp { cur[i - bpp] as u16 } else { 0 };
                    let b = if y == 0 { 0u16 } else { prev[i] as u16 };
                    cur[i] = src[i].wrapping_add(((a + b) / 2) as u8);
                }
            }
            4 => {
                for i in 0..rowbytes {
                    let a = if i >= bpp { cur[i - bpp] } else { 0 };
                    let b = if y == 0 { 0 } else { prev[i] };
                    let c = if y == 0 || i < bpp { 0 } else { prev[i - bpp] };
                    cur[i] = src[i].wrapping_add(paeth(a, b, c));
                }
            }
            _ => return Err(PngError::BadChunk("invalid filter type")),
        }
    }
    Ok(out)
}

#[inline]
fn get_sample(row: &[u8], i: usize, bit_depth: u8) -> u32 {
    match bit_depth {
        8 => row[i] as u32,
        16 => ((row[i * 2] as u32) << 8) | row[i * 2 + 1] as u32,
        1 | 2 | 4 => {
            let bd = bit_depth as usize;
            let per_byte = 8 / bd;
            let byte = row[i / per_byte];
            let shift = 8 - bd - (i % per_byte) * bd;
            let mask = (1u32 << bd) - 1;
            (byte as u32 >> shift) & mask
        }
        _ => 0,
    }
}

#[inline]
const fn scale_to_8(sample: u32, bit_depth: u8) -> u8 {
    match bit_depth {
        16 => ((sample + 128) / 257) as u8,
        8 => sample as u8,
        1 => {
            if sample != 0 {
                255
            } else {
                0
            }
        }
        2 => (sample * 85) as u8,
        4 => (sample * 17) as u8,
        _ => sample as u8,
    }
}

#[allow(clippy::too_many_arguments)]
fn to_rgba8(
    unfiltered: &[u8],
    width: u32,
    height: u32,
    rowbytes: usize,
    ihdr: &Ihdr,
    plte: Option<&[u8]>,
    trns: Option<&[u8]>,
) -> Result<Vec<u8>> {
    let w = width as usize;
    let h = height as usize;
    let mut out = vec![0u8; w * h * 4];
    write_pixels_into(
        unfiltered,
        rowbytes,
        ihdr,
        plte,
        trns,
        |x, y, rgba| {
            let idx = (y * w + x) * 4;
            out[idx..idx + 4].copy_from_slice(&rgba);
        },
        w,
        h,
    )?;
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn write_pixels_into<F: FnMut(usize, usize, [u8; 4])>(
    unfiltered: &[u8],
    rowbytes: usize,
    ihdr: &Ihdr,
    plte: Option<&[u8]>,
    trns: Option<&[u8]>,
    mut emit: F,
    w: usize,
    h: usize,
) -> Result<()> {
    let bd = ihdr.bit_depth;

    let trns_grey: Option<u32> = if ihdr.color_type == 0 {
        trns.and_then(|t| {
            if t.len() >= 2 {
                Some(((t[0] as u32) << 8) | t[1] as u32)
            } else {
                None
            }
        })
    } else {
        None
    };
    let trns_rgb: Option<(u32, u32, u32)> = if ihdr.color_type == 2 {
        trns.and_then(|t| {
            if t.len() >= 6 {
                Some((
                    ((t[0] as u32) << 8) | t[1] as u32,
                    ((t[2] as u32) << 8) | t[3] as u32,
                    ((t[4] as u32) << 8) | t[5] as u32,
                ))
            } else {
                None
            }
        })
    } else {
        None
    };

    for y in 0..h {
        let row = &unfiltered[y * rowbytes..(y + 1) * rowbytes];
        for x in 0..w {
            let rgba: [u8; 4] = match ihdr.color_type {
                0 => {
                    let s = get_sample(row, x, bd);
                    let g = scale_to_8(s, bd);
                    let a = match trns_grey {
                        Some(tv) if tv == s => 0,
                        _ => 255,
                    };
                    [g, g, g, a]
                }
                4 => {
                    let s = get_sample(row, x * 2, bd);
                    let av = get_sample(row, x * 2 + 1, bd);
                    let g = scale_to_8(s, bd);
                    let a = scale_to_8(av, bd);
                    [g, g, g, a]
                }
                2 => {
                    let r = get_sample(row, x * 3, bd);
                    let g = get_sample(row, x * 3 + 1, bd);
                    let b = get_sample(row, x * 3 + 2, bd);
                    let a = match trns_rgb {
                        Some((tr, tg, tb)) if tr == r && tg == g && tb == b => 0,
                        _ => 255,
                    };
                    [scale_to_8(r, bd), scale_to_8(g, bd), scale_to_8(b, bd), a]
                }
                6 => {
                    let r = get_sample(row, x * 4, bd);
                    let g = get_sample(row, x * 4 + 1, bd);
                    let b = get_sample(row, x * 4 + 2, bd);
                    let a = get_sample(row, x * 4 + 3, bd);
                    [
                        scale_to_8(r, bd),
                        scale_to_8(g, bd),
                        scale_to_8(b, bd),
                        scale_to_8(a, bd),
                    ]
                }
                3 => {
                    let idx = get_sample(row, x, bd) as usize;
                    let pal = plte.ok_or(PngError::BadChunk("palette image without PLTE"))?;
                    let off = idx * 3;
                    if off + 2 >= pal.len() {
                        return Err(PngError::BadChunk("palette index out of range"));
                    }
                    let r = pal[off];
                    let g = pal[off + 1];
                    let b = pal[off + 2];
                    let a = match trns {
                        Some(t) if idx < t.len() => t[idx],
                        _ => 255,
                    };
                    [r, g, b, a]
                }
                _ => unreachable!(),
            };
            emit(x, y, rgba);
        }
    }
    Ok(())
}

const ADAM7_X_START: [u32; 7] = [0, 4, 0, 2, 0, 1, 0];
const ADAM7_Y_START: [u32; 7] = [0, 0, 4, 0, 2, 0, 1];
const ADAM7_X_STEP: [u32; 7] = [8, 8, 4, 4, 2, 2, 1];
const ADAM7_Y_STEP: [u32; 7] = [8, 8, 8, 4, 4, 2, 2];

const fn adam7_pass_dims(width: u32, height: u32, pass: usize) -> (u32, u32) {
    let xs = ADAM7_X_START[pass];
    let ys = ADAM7_Y_START[pass];
    let xstep = ADAM7_X_STEP[pass];
    let ystep = ADAM7_Y_STEP[pass];
    let pw = if width > xs {
        (width - xs).div_ceil(xstep)
    } else {
        0
    };
    let ph = if height > ys {
        (height - ys).div_ceil(ystep)
    } else {
        0
    };
    (pw, ph)
}

fn decode_adam7(
    raw: &[u8],
    ihdr: &Ihdr,
    channels: usize,
    plte: Option<&[u8]>,
    trns: Option<&[u8]>,
) -> Result<Vec<u8>> {
    let w = ihdr.width as usize;
    let h = ihdr.height as usize;
    let mut out = vec![0u8; w * h * 4];
    let bpp = bytes_per_pixel(ihdr.bit_depth, channels);

    let mut offset = 0usize;
    for pass in 0..7 {
        let (pw, ph) = adam7_pass_dims(ihdr.width, ihdr.height, pass);
        if pw == 0 || ph == 0 {
            continue;
        }
        let rowbytes = row_bytes(pw, ihdr.bit_depth, channels);
        let stride = rowbytes + 1;
        let passlen = stride * ph as usize;
        if offset + passlen > raw.len() {
            return Err(PngError::Inflate("interlaced data short".into()));
        }
        let passraw = &raw[offset..offset + passlen];
        offset += passlen;

        let unfiltered = unfilter(passraw, ph as usize, rowbytes, bpp)?;

        let xs = ADAM7_X_START[pass] as usize;
        let ys = ADAM7_Y_START[pass] as usize;
        let xstep = ADAM7_X_STEP[pass] as usize;
        let ystep = ADAM7_Y_STEP[pass] as usize;

        write_pixels_into(
            &unfiltered,
            rowbytes,
            ihdr,
            plte,
            trns,
            |px, py, rgba| {
                let real_x = xs + px * xstep;
                let real_y = ys + py * ystep;
                if real_x < w && real_y < h {
                    let idx = (real_y * w + real_x) * 4;
                    out[idx..idx + 4].copy_from_slice(&rgba);
                }
            },
            pw as usize,
            ph as usize,
        )?;
    }
    Ok(out)
}
