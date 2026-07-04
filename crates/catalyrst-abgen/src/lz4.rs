use std::cell::RefCell;
use std::fmt;

const MINMATCH: i32 = 4;
const LASTLITERALS: usize = 5;
const MFLIMIT: usize = 12;
const ML_BITS: u32 = 4;
const ML_MASK: u32 = (1 << ML_BITS) - 1;
const RUN_BITS: u32 = 8 - ML_BITS;
const RUN_MASK: u32 = (1 << RUN_BITS) - 1;
const LZ4_MINLENGTH: usize = MFLIMIT + 1;
const LZ4_DISTANCE_MAX: u32 = 65535;

const LZ4HC_HASH_LOG: u32 = 15;
const LZ4HC_HASHTABLESIZE: usize = 1 << LZ4HC_HASH_LOG;
const LZ4HC_DICTIONARY_LOGSIZE: u32 = 16;
const LZ4HC_MAXD: usize = 1 << LZ4HC_DICTIONARY_LOGSIZE;

const LZ4_OPT_NUM: usize = 1 << 12;
const TRAILING_LITERALS: usize = 3;

const NB_SEARCHES: i32 = 16384;
const SUFFICIENT_LEN_INIT: usize = LZ4_OPT_NUM;

const NB_SEARCHES_FAST_SERVE: i32 = 96;
const SUFFICIENT_LEN_FAST_SERVE: usize = 64;

fn fast_serve_enabled() -> bool {
    static V: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *V.get_or_init(|| {
        std::env::var_os("ABGEN_FAST_SERVE").is_some_and(|v| !v.is_empty() && v != "0")
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Lz4Error {
    Malformed(&'static str),
}

impl fmt::Display for Lz4Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Lz4Error::Malformed(m) => write!(f, "lz4 decompress: {m}"),
        }
    }
}

impl std::error::Error for Lz4Error {}

const MAX_DECOMPRESS_BYTES: usize = 256 * 1024 * 1024;

pub fn decompress(src: &[u8], dst_size: usize) -> Result<Vec<u8>, Lz4Error> {
    if dst_size > MAX_DECOMPRESS_BYTES {
        return Err(Lz4Error::Malformed(
            "decompressed size exceeds MAX_DECOMPRESS_BYTES",
        ));
    }
    let mut dst = vec![0u8; dst_size];
    let mut sp = 0usize;
    let mut dp = 0usize;
    let slen = src.len();

    if slen == 0 {
        if dst_size == 0 {
            return Ok(dst);
        }
        return Err(Lz4Error::Malformed("empty input but non-zero output"));
    }

    loop {
        if sp >= slen {
            return Err(Lz4Error::Malformed("truncated token"));
        }
        let token = src[sp] as u32;
        sp += 1;

        let mut lit_len = (token >> ML_BITS) as usize;
        if lit_len == RUN_MASK as usize {
            loop {
                if sp >= slen {
                    return Err(Lz4Error::Malformed("truncated literal length"));
                }
                let b = src[sp];
                sp += 1;
                lit_len += b as usize;
                if b != 255 {
                    break;
                }
            }
        }

        if lit_len > 0 {
            if sp + lit_len > slen {
                return Err(Lz4Error::Malformed("literal run exceeds input"));
            }
            if dp + lit_len > dst_size {
                return Err(Lz4Error::Malformed("literal run exceeds output"));
            }
            dst[dp..dp + lit_len].copy_from_slice(&src[sp..sp + lit_len]);
            sp += lit_len;
            dp += lit_len;
        }

        if sp == slen {
            break;
        }

        if sp + 2 > slen {
            return Err(Lz4Error::Malformed("truncated offset"));
        }
        let offset = (src[sp] as usize) | ((src[sp + 1] as usize) << 8);
        sp += 2;
        if offset == 0 {
            return Err(Lz4Error::Malformed("zero offset"));
        }
        if offset > dp {
            return Err(Lz4Error::Malformed("offset before output start"));
        }

        let mut match_len = (token & ML_MASK) as usize;
        if match_len == ML_MASK as usize {
            loop {
                if sp >= slen {
                    return Err(Lz4Error::Malformed("truncated match length"));
                }
                let b = src[sp];
                sp += 1;
                match_len += b as usize;
                if b != 255 {
                    break;
                }
            }
        }
        match_len += MINMATCH as usize;

        if dp + match_len > dst_size {
            return Err(Lz4Error::Malformed("match copy exceeds output"));
        }

        let mp = dp - offset;
        for k in 0..match_len {
            dst[dp + k] = dst[mp + k];
        }
        dp += match_len;
    }

    if dp != dst_size {
        return Err(Lz4Error::Malformed("decompressed size mismatch"));
    }
    Ok(dst)
}

#[inline(always)]
fn read32(buf: &[u8], i: usize) -> u32 {
    debug_assert!(i + 4 <= buf.len());

    unsafe { (buf.as_ptr().add(i) as *const u32).read_unaligned() }.to_le()
}

#[inline(always)]
fn read16(buf: &[u8], i: usize) -> u16 {
    debug_assert!(i + 2 <= buf.len());

    unsafe { (buf.as_ptr().add(i) as *const u16).read_unaligned() }.to_le()
}

#[inline(always)]
fn read64(buf: &[u8], i: usize) -> u64 {
    debug_assert!(i + 8 <= buf.len());

    unsafe { (buf.as_ptr().add(i) as *const u64).read_unaligned() }.to_le()
}

#[inline]
fn hash_ptr(buf: &[u8], i: usize) -> u32 {
    let v = read32(buf, i);
    v.wrapping_mul(2654435761)
        .wrapping_shr((MINMATCH as u32 * 8) - LZ4HC_HASH_LOG)
}

#[inline]
fn lz4_count(buf: &[u8], mut pin: usize, mut pmatch: usize, limit: usize) -> u32 {
    let start = pin;
    const STEP: usize = 8;

    debug_assert!(limit <= buf.len());
    while pin + STEP <= limit {
        let a = read64(buf, pin);
        let b = read64(buf, pmatch);
        let diff = a ^ b;
        if diff == 0 {
            pin += STEP;
            pmatch += STEP;
            continue;
        }
        pin += (diff.trailing_zeros() / 8) as usize;
        return (pin - start) as u32;
    }

    if pin < limit {
        debug_assert!(limit >= 8);
        let q = limit - 8;
        let back = pin - q;
        if pmatch >= back {
            let diff = (read64(buf, q) ^ read64(buf, pmatch - back)) >> (8 * back);
            if diff != 0 {
                pin += (diff.trailing_zeros() / 8) as usize;
            } else {
                pin = limit;
            }
        } else {
            while pin < limit && buf[pin] == buf[pmatch] {
                pin += 1;
                pmatch += 1;
            }
        }
    }
    (pin - start) as u32
}

#[inline]
fn count_back(buf: &[u8], ip: usize, m: usize, imin: usize, mmin: usize) -> i32 {
    let mut back: i32 = 0;

    let min = std::cmp::max(imin as i64 - ip as i64, mmin as i64 - m as i64) as i32;
    while (back > min)
        && buf[(ip as i32 + back - 1) as usize] == buf[(m as i32 + back - 1) as usize]
    {
        back -= 1;
    }
    back
}

struct HcCtx {
    hash_table: Box<[u32; LZ4HC_HASHTABLESIZE]>,

    combo_table: Box<[u32; LZ4HC_MAXD]>,

    base: u32,
    next_to_update: u32,
}

impl HcCtx {
    fn new() -> Self {
        let base = 64 * 1024;
        let hash_table: Box<[u32; LZ4HC_HASHTABLESIZE]> = vec![0u32; LZ4HC_HASHTABLESIZE]
            .into_boxed_slice()
            .try_into()
            .unwrap_or_else(|_| unreachable!());

        let combo_table: Box<[u32; LZ4HC_MAXD]> = vec![0x0000FFFFu32; LZ4HC_MAXD]
            .into_boxed_slice()
            .try_into()
            .unwrap_or_else(|_| unreachable!());
        HcCtx {
            hash_table,
            combo_table,
            base,
            next_to_update: base,
        }
    }

    fn reset(&mut self) {
        self.hash_table.fill(0);
        self.combo_table.fill(0x0000FFFFu32);
        self.base = 64 * 1024;
        self.next_to_update = self.base;
    }

    fn insert(&mut self, buf: &[u8], ip: usize) {
        let prefix_idx = self.base;
        let target = ip as u32 + prefix_idx;
        let mut idx = self.next_to_update;
        let pending = target.saturating_sub(idx);

        const HASH_BATCH: usize = 8;
        if pending as usize >= HASH_BATCH && buf.len() >= 4 + HASH_BATCH {
            let mut hashes = [0u32; HASH_BATCH];
            while idx + HASH_BATCH as u32 <= target {
                let pos0 = (idx - prefix_idx) as usize;
                if pos0 + 4 + HASH_BATCH > buf.len() {
                    break;
                }
                hash_batch_8(buf, pos0, &mut hashes);
                for k in 0..HASH_BATCH {
                    let h = (hashes[k] as usize) & (LZ4HC_HASHTABLESIZE - 1);
                    let cur = idx + k as u32;
                    let mut delta = cur - self.hash_table[h];
                    if delta > LZ4_DISTANCE_MAX {
                        delta = LZ4_DISTANCE_MAX;
                    }
                    let d2 = self.combo_table[(cur.wrapping_sub(delta) & 0xFFFF) as usize] & 0xFFFF;
                    let sum = (delta + d2).min(0xFFFF);
                    self.combo_table[(cur & 0xFFFF) as usize] = delta | (sum << 16);
                    self.hash_table[h] = cur;
                }
                idx += HASH_BATCH as u32;
            }
        }

        while idx < target {
            let pos = (idx - prefix_idx) as usize;
            let h = (hash_ptr(buf, pos) as usize) & (LZ4HC_HASHTABLESIZE - 1);
            let mut delta = idx - self.hash_table[h];
            if delta > LZ4_DISTANCE_MAX {
                delta = LZ4_DISTANCE_MAX;
            }
            let d2 = self.combo_table[(idx.wrapping_sub(delta) & 0xFFFF) as usize] & 0xFFFF;
            let sum = (delta + d2).min(0xFFFF);
            self.combo_table[(idx & 0xFFFF) as usize] = delta | (sum << 16);
            self.hash_table[h] = idx;
            idx += 1;
        }
        self.next_to_update = target;
    }
}

thread_local! {
    static HC_CTX_POOL: RefCell<HcCtx> = RefCell::new(HcCtx::new());
}

#[inline(always)]
fn hash_batch_8(buf: &[u8], pos0: usize, out: &mut [u32; 8]) {
    #[cfg(all(
        any(target_arch = "x86_64", target_arch = "x86"),
        target_feature = "avx2"
    ))]
    unsafe {
        use std::arch::x86_64::*;
        let v0 = read32(buf, pos0);
        let v1 = read32(buf, pos0 + 1);
        let v2 = read32(buf, pos0 + 2);
        let v3 = read32(buf, pos0 + 3);
        let v4 = read32(buf, pos0 + 4);
        let v5 = read32(buf, pos0 + 5);
        let v6 = read32(buf, pos0 + 6);
        let v7 = read32(buf, pos0 + 7);
        let words = _mm256_set_epi32(
            v7 as i32, v6 as i32, v5 as i32, v4 as i32, v3 as i32, v2 as i32, v1 as i32, v0 as i32,
        );
        let prime = _mm256_set1_epi32(2654435761u32 as i32);
        let prod = _mm256_mullo_epi32(words, prime);
        const SHR: i32 = (MINMATCH * 8) - LZ4HC_HASH_LOG as i32;
        let hashed = _mm256_srli_epi32::<SHR>(prod);
        _mm256_storeu_si256(out.as_mut_ptr() as *mut __m256i, hashed);
    }
    #[cfg(not(all(
        any(target_arch = "x86_64", target_arch = "x86"),
        target_feature = "avx2"
    )))]
    {
        for k in 0..8 {
            out[k] = hash_ptr(buf, pos0 + k);
        }
    }
}

#[inline(always)]
fn delta_next_u16(combo: &[u32; LZ4HC_MAXD], pos: u32) -> u32 {
    combo[(pos & 0xFFFF) as usize] & 0xFFFF
}

fn count_pattern(buf: &[u8], mut ip: usize, iend: usize, pattern32: u32) -> u32 {
    let start = ip;

    let pattern64 = (pattern32 as u64) | ((pattern32 as u64) << 32);
    while ip + 8 <= iend {
        let diff = read64(buf, ip) ^ pattern64;
        if diff != 0 {
            ip += (diff.trailing_zeros() / 8) as usize;
            return (ip - start) as u32;
        }
        ip += 8;
    }
    if ip < iend {
        debug_assert!(iend >= 8);
        let q = iend - 8;
        let phase = (q.wrapping_sub(start) & 3) as u32;
        let exp32 = pattern32.rotate_right(8 * phase);
        let exp64 = (exp32 as u64) | ((exp32 as u64) << 32);
        let diff = (read64(buf, q) ^ exp64) >> (8 * (ip - q));
        if diff != 0 {
            ip += (diff.trailing_zeros() / 8) as usize;
        } else {
            ip = iend;
        }
    }
    (ip - start) as u32
}

fn reverse_count_pattern(buf: &[u8], mut ip: usize, ilow: usize, pattern: u32) -> u32 {
    let start = ip;

    let pattern64 = (pattern as u64) | ((pattern as u64) << 32);
    while ip >= ilow + 8 {
        let m = read64(buf, ip - 8) ^ pattern64;
        if m != 0 {
            return (start - ip) as u32 + m.leading_zeros() / 8;
        }
        ip -= 8;
    }
    while ip >= ilow + 4 {
        let m = read32(buf, ip - 4) ^ pattern;
        if m != 0 {
            return (start - ip) as u32 + m.leading_zeros() / 8;
        }
        ip -= 4;
    }

    let rem = ip - ilow;
    if rem > 0 {
        debug_assert!(ilow + 4 <= buf.len());
        let exp = pattern.rotate_right(8 * ((ilow.wrapping_sub(ip) & 3) as u32));
        let diff = (read32(buf, ilow) ^ exp) << (8 * (4 - rem));
        let matched = if diff == 0 {
            rem as u32
        } else {
            (diff.leading_zeros() / 8).min(rem as u32)
        };
        return (start - ip) as u32 + matched;
    }
    (start - ip) as u32
}

#[inline]
const fn protect_dict_end(dict_limit: u32, match_index: u32) -> bool {
    (dict_limit.wrapping_sub(1).wrapping_sub(match_index)) >= 3
}

#[derive(Clone, Copy, Default)]
struct Match {
    off: i32,
    len: i32,
}

#[derive(PartialEq, Clone, Copy)]
enum RepeatState {
    Untested,
    Not,
    Confirmed,
}

#[inline(never)]
fn chain_swap_scan(combo_table: &[u32; LZ4HC_MAXD], match_index: u32, longest: i32) -> (u32, u32) {
    let k_trigger = 4;
    let mut distance_to_next_match: u32 = 1;
    let mut match_chain_pos: u32 = 0;
    let end = longest - MINMATCH + 1;
    let mut step;
    let mut accel = 1i32 << k_trigger;
    let mut pos = 0i32;
    while pos < end {
        let candidate_dist = delta_next_u16(combo_table, match_index + pos as u32);
        step = accel >> k_trigger;
        accel += 1;
        if candidate_dist > distance_to_next_match {
            distance_to_next_match = candidate_dist;
            match_chain_pos = pos as u32;
            accel = 1 << k_trigger;
        }
        pos += step;
    }
    (distance_to_next_match, match_chain_pos)
}

#[derive(Default)]
struct PatternMemo {
    s: [usize; 2],
    e: [usize; 2],
}

impl PatternMemo {
    #[inline]
    fn query(&self, p: usize) -> Option<(usize, usize)> {
        for k in 0..2 {
            if self.s[k] <= p && p + 4 <= self.e[k] {
                return Some((self.e[k] - p, p - self.s[k]));
            }
        }
        None
    }
}

#[allow(clippy::too_many_arguments)]
fn insert_and_get_wider_match(
    ctx: &mut HcCtx,
    buf: &[u8],
    ip: usize,
    ilow_limit: usize,
    ihigh_limit: usize,
    mut longest: i32,
    max_nb_attempts: i32,
    pattern_analysis: bool,
    chain_swap: bool,
    memo: &mut PatternMemo,
) -> Match {
    let prefix_idx = ctx.base;
    let ip_index = ip as u32 + prefix_idx;
    let within_start_distance = (ctx.base + (LZ4_DISTANCE_MAX + 1)) > ip_index;
    let lowest_match_index = if within_start_distance {
        ctx.base
    } else {
        ip_index - LZ4_DISTANCE_MAX
    };
    let look_back_length = (ip - ilow_limit) as i32;
    let mut nb_attempts = max_nb_attempts;
    let mut match_chain_pos: u32 = 0;
    let pattern = read32(buf, ip);
    let mut repeat = RepeatState::Untested;
    let mut src_pattern_length: usize = 0;
    let mut offset = 0i32;

    ctx.insert(buf, ip);
    let mut match_index = ctx.hash_table[(hash_ptr(buf, ip) as usize) & (LZ4HC_HASHTABLESIZE - 1)];

    let mut lo16 = read16(buf, (ilow_limit as isize + longest as isize - 1) as usize);
    let mut mo_add = longest as isize - 1 - look_back_length as isize;

    let mut to_phase2 = false;

    macro_rules! probe {
        () => {{
            let mut match_length: i32 = 0;
            debug_assert!(match_index < ip_index);
            debug_assert!(match_index >= prefix_idx);
            let match_ptr = (match_index - prefix_idx) as usize;
            debug_assert!(match_ptr < ip);
            debug_assert!(longest >= 1);
            let mo = (match_ptr as isize + mo_add) as usize;
            if read16(buf, mo) == lo16 && read32(buf, match_ptr) == pattern {
                let back = if look_back_length != 0 {
                    count_back(buf, ip, match_ptr, ilow_limit, 0)
                } else {
                    0
                };
                match_length = MINMATCH
                    + lz4_count(
                        buf,
                        ip + MINMATCH as usize,
                        match_ptr + MINMATCH as usize,
                        ihigh_limit,
                    ) as i32;
                match_length -= back;
                if match_length > longest {
                    longest = match_length;
                    offset = (ip_index - match_index) as i32;
                    lo16 = read16(buf, (ilow_limit as isize + longest as isize - 1) as usize);
                    mo_add = longest as isize - 1 - look_back_length as isize;
                }
            }
            match_length
        }};
    }

    macro_rules! pattern_path {
        ($walk:lifetime) => {{
            let match_candidate_idx = match_index - 1;
            if repeat == RepeatState::Untested {
                if (pattern & 0xFFFF) == (pattern >> 16) && (pattern & 0xFF) == (pattern >> 24)
                {
                    repeat = RepeatState::Confirmed;
                    src_pattern_length = if let Some((fwd, _)) = memo.query(ip) {
                        fwd
                    } else {
                        let l = count_pattern(buf, ip + 4, ihigh_limit, pattern) as usize + 4;
                        let back = reverse_count_pattern(buf, ip, 0, pattern) as usize;
                        memo.s[0] = ip - back;
                        memo.e[0] = ip + l;
                        l
                    };
                } else {
                    repeat = RepeatState::Not;
                }
            }
            if repeat == RepeatState::Confirmed
                && match_candidate_idx >= lowest_match_index
                && protect_dict_end(prefix_idx, match_candidate_idx)
            {
                let match_ptr = (match_candidate_idx - prefix_idx) as usize;
                if read32(buf, match_ptr) == pattern {
                    let i_limit = ihigh_limit;
                    let (forward_pattern_length, back_raw) =
                        if let Some(hit) = memo.query(match_ptr) {
                            hit
                        } else {
                            let fwd = count_pattern(buf, match_ptr + 4, i_limit, pattern)
                                as usize
                                + 4;
                            let back =
                                reverse_count_pattern(buf, match_ptr, 0, pattern) as usize;
                            memo.s[1] = match_ptr - back;
                            memo.e[1] = match_ptr + fwd;
                            (fwd, back)
                        };

                    {
                        let mut back_length = back_raw;

                        let lower = std::cmp::max(
                            match_candidate_idx.wrapping_sub(back_length as u32),
                            lowest_match_index,
                        );
                        back_length = (match_candidate_idx - lower) as usize;
                        let current_segment_length = back_length + forward_pattern_length;

                        if current_segment_length >= src_pattern_length
                            && forward_pattern_length <= src_pattern_length
                        {
                            let new_match_index = match_candidate_idx
                                + forward_pattern_length as u32
                                - src_pattern_length as u32;
                            if protect_dict_end(prefix_idx, new_match_index) {
                                match_index = new_match_index;
                            } else {
                                match_index = prefix_idx;
                            }
                        } else {
                            let new_match_index = match_candidate_idx - back_length as u32;
                            if !protect_dict_end(prefix_idx, new_match_index) {
                                match_index = prefix_idx;
                            } else {
                                match_index = new_match_index;

                                if look_back_length == 0 {
                                    let max_ml = std::cmp::min(
                                        current_segment_length,
                                        src_pattern_length,
                                    );
                                    if (longest as usize) < max_ml {
                                        if (ip as u32 + prefix_idx - match_index)
                                            > LZ4_DISTANCE_MAX
                                        {
                                            break $walk;
                                        }
                                        longest = max_ml as i32;
                                        offset = (ip_index - match_index) as i32;
                                        lo16 = read16(
                                            buf,
                                            (ilow_limit as isize + longest as isize - 1)
                                                as usize,
                                        );
                                        mo_add =
                                            longest as isize - 1 - look_back_length as isize;
                                    }
                                    let dist_to_next_pattern =
                                        delta_next_u16(&ctx.combo_table, match_index);
                                    if dist_to_next_pattern > match_index {
                                        break $walk;
                                    }
                                    match_index -= dist_to_next_pattern;
                                }
                            }
                        }
                    }
                    continue $walk;
                }
            }
        }};
    }

    'walk: while match_index >= lowest_match_index && nb_attempts > 0 {
        let packed = ctx.combo_table[(match_index & 0xFFFF) as usize];
        let d1 = packed & 0xFFFF;
        let sum = packed >> 16;

        nb_attempts -= 1;
        let ml_a = probe!();
        if chain_swap && ml_a == longest {
            debug_assert!(look_back_length == 0);
            if match_index + longest as u32 <= ip_index {
                let (distance_to_next_match, scan_pos) =
                    chain_swap_scan(&ctx.combo_table, match_index, longest);
                if distance_to_next_match > 1 {
                    match_chain_pos = scan_pos;
                    if distance_to_next_match > match_index {
                        break 'walk;
                    }
                    match_index -= distance_to_next_match;
                    if match_chain_pos == 0 {
                        continue 'walk;
                    }
                    to_phase2 = true;
                    break 'walk;
                }
            }
        }
        if pattern_analysis && d1 == 1 {
            pattern_path!('walk);
        }

        match_index -= d1;
        if match_index < lowest_match_index || nb_attempts <= 0 {
            break 'walk;
        }

        nb_attempts -= 1;
        let d2 = if sum == 0xFFFF {
            delta_next_u16(&ctx.combo_table, match_index)
        } else {
            sum - d1
        };
        let ml_b = probe!();
        if chain_swap && ml_b == longest {
            debug_assert!(look_back_length == 0);
            if match_index + longest as u32 <= ip_index {
                let (distance_to_next_match, scan_pos) =
                    chain_swap_scan(&ctx.combo_table, match_index, longest);
                if distance_to_next_match > 1 {
                    match_chain_pos = scan_pos;
                    if distance_to_next_match > match_index {
                        break 'walk;
                    }
                    match_index -= distance_to_next_match;
                    if match_chain_pos == 0 {
                        continue 'walk;
                    }
                    to_phase2 = true;
                    break 'walk;
                }
            }
        }
        if pattern_analysis && d2 == 1 {
            pattern_path!('walk);
        }
        match_index -= d2;
    }

    if to_phase2 {
        'p2: while match_index >= lowest_match_index && nb_attempts > 0 {
            nb_attempts -= 1;
            let match_length = probe!();

            if chain_swap && match_length == longest {
                debug_assert!(look_back_length == 0);
                if match_index + longest as u32 <= ip_index {
                    let (distance_to_next_match, scan_pos) =
                        chain_swap_scan(&ctx.combo_table, match_index, longest);
                    if distance_to_next_match > 1 {
                        match_chain_pos = scan_pos;
                        if distance_to_next_match > match_index {
                            break 'p2;
                        }
                        match_index -= distance_to_next_match;
                        continue 'p2;
                    }
                }
            }

            let dist_next_match = delta_next_u16(&ctx.combo_table, match_index + match_chain_pos);
            if pattern_analysis && match_chain_pos == 0 && dist_next_match == 1 {
                pattern_path!('p2);
            }
            match_index -= dist_next_match;
        }
    }

    Match {
        len: longest,
        off: offset,
    }
}

fn find_longer_match(
    ctx: &mut HcCtx,
    buf: &[u8],
    ip: usize,
    ihigh_limit: usize,
    min_len: i32,
    nb_searches: i32,
    memo: &mut PatternMemo,
) -> Match {
    let md = insert_and_get_wider_match(
        ctx,
        buf,
        ip,
        ip,
        ihigh_limit,
        min_len,
        nb_searches,
        true,
        true,
        memo,
    );
    if md.len <= min_len {
        return Match::default();
    }

    md
}

#[inline]
const fn literals_price(litlen: i32) -> i32 {
    let mut price = litlen;
    if litlen >= RUN_MASK as i32 {
        price += 1 + (litlen - RUN_MASK as i32) / 255;
    }
    price
}

#[inline]
fn sequence_price(litlen: i32, mlen: i32) -> i32 {
    let mut price = 1 + 2;
    price += literals_price(litlen);
    if mlen >= (ML_MASK as i32 + MINMATCH) {
        price += 1 + (mlen - (ML_MASK as i32 + MINMATCH)) / 255;
    }
    price
}

fn encode_sequence(
    buf: &[u8],
    op: &mut Vec<u8>,
    ip: &mut usize,
    anchor: &mut usize,
    match_length: i32,
    offset: i32,
) {
    let length = *ip - *anchor;
    let token_pos = op.len();
    op.push(0);
    if length >= RUN_MASK as usize {
        let mut len = length - RUN_MASK as usize;
        op[token_pos] = (RUN_MASK << ML_BITS) as u8;
        while len >= 255 {
            op.push(255);
            len -= 255;
        }
        op.push(len as u8);
    } else {
        op[token_pos] = (length << ML_BITS) as u8;
    }

    op.extend_from_slice(&buf[*anchor..*anchor + length]);

    op.push((offset & 0xFF) as u8);
    op.push(((offset >> 8) & 0xFF) as u8);

    let mlen = (match_length - MINMATCH) as usize;
    if mlen >= ML_MASK as usize {
        op[token_pos] += ML_MASK as u8;
        let mut length = mlen - ML_MASK as usize;
        while length >= 510 {
            op.push(255);
            op.push(255);
            length -= 510;
        }
        if length >= 255 {
            length -= 255;
            op.push(255);
        }
        op.push(length as u8);
    } else {
        op[token_pos] += mlen as u8;
    }

    *ip += match_length as usize;
    *anchor = *ip;
}

#[derive(Clone, Copy, Default)]
struct Opt {
    price: i32,
    off: i32,
    mlen: i32,
    litlen: i32,
}

fn compress_optimal(buf: &[u8]) -> Vec<u8> {
    HC_CTX_POOL.with(|cell| {
        let mut ctx = cell.borrow_mut();
        ctx.reset();
        compress_optimal_with_ctx(buf, &mut ctx)
    })
}

fn compress_optimal_with_ctx(buf: &[u8], ctx: &mut HcCtx) -> Vec<u8> {
    let mut memo = PatternMemo::default();
    let src_size = buf.len();
    let mut op: Vec<u8> = Vec::with_capacity(src_size + src_size / 255 + 16);

    let mut opt = vec![Opt::default(); LZ4_OPT_NUM + TRAILING_LITERALS];

    let mut ip = 0usize;
    let mut anchor = 0usize;
    let iend = src_size;

    let mflimit = iend.wrapping_sub(MFLIMIT);
    let matchlimit = iend.wrapping_sub(LASTLITERALS);

    let (nb_searches, mut sufficient_len) = if fast_serve_enabled() {
        (NB_SEARCHES_FAST_SERVE, SUFFICIENT_LEN_FAST_SERVE)
    } else {
        (NB_SEARCHES, SUFFICIENT_LEN_INIT)
    };
    if sufficient_len >= LZ4_OPT_NUM {
        sufficient_len = LZ4_OPT_NUM - 1;
    }

    if src_size < LZ4_MINLENGTH {
        emit_last_literals(buf, &mut op, anchor, iend);
        return op;
    }

    while ip <= mflimit {
        let llen = (ip - anchor) as i32;
        let mut best_mlen;
        let mut best_off;
        let mut cur;
        let mut last_match_pos: i32;

        let first_match = find_longer_match(
            ctx,
            buf,
            ip,
            matchlimit,
            MINMATCH - 1,
            nb_searches,
            &mut memo,
        );
        if first_match.len == 0 {
            ip += 1;
            continue;
        }

        if first_match.len as usize > sufficient_len {
            let first_ml = first_match.len;
            encode_sequence(
                buf,
                &mut op,
                &mut ip,
                &mut anchor,
                first_ml,
                first_match.off,
            );
            continue;
        }

        for r_pos in 0..MINMATCH as usize {
            let cost = literals_price(llen + r_pos as i32);
            opt[r_pos].mlen = 1;
            opt[r_pos].off = 0;
            opt[r_pos].litlen = llen + r_pos as i32;
            opt[r_pos].price = cost;
        }

        {
            let match_ml = first_match.len;
            let offset = first_match.off;

            let mut cost = sequence_price(llen, MINMATCH);
            let mut next_bump = ML_MASK as i32 + MINMATCH;
            let mut mlen = MINMATCH;
            while mlen <= match_ml {
                let m = mlen as usize;
                opt[m].mlen = mlen;
                opt[m].off = offset;
                opt[m].litlen = llen;
                opt[m].price = cost;
                mlen += 1;
                if mlen == next_bump {
                    cost += 1;
                    next_bump += 255;
                }
            }
        }
        last_match_pos = first_match.len;
        for add_lit in 1..=TRAILING_LITERALS {
            let p = last_match_pos as usize + add_lit;
            opt[p].mlen = 1;
            opt[p].off = 0;
            opt[p].litlen = add_lit as i32;
            opt[p].price = opt[last_match_pos as usize].price + literals_price(add_lit as i32);
        }

        cur = 1i32;
        let mut goto_encode = false;
        while cur < last_match_pos {
            let cur_pos = ip + cur as usize;
            if cur_pos > mflimit {
                break;
            }

            let c = cur as usize;
            if opt[c + 1].price <= opt[c].price
                && opt[c + MINMATCH as usize].price < opt[c].price + 3
            {
                cur += 1;
                continue;
            }

            let new_match = find_longer_match(
                ctx,
                buf,
                cur_pos,
                matchlimit,
                MINMATCH - 1,
                nb_searches,
                &mut memo,
            );
            if new_match.len == 0 {
                cur += 1;
                continue;
            }

            if new_match.len as usize > sufficient_len
                || (new_match.len + cur) as usize >= LZ4_OPT_NUM
            {
                best_mlen = new_match.len;
                best_off = new_match.off;
                last_match_pos = cur + 1;

                encode_path(
                    buf,
                    &mut op,
                    &mut ip,
                    &mut anchor,
                    &mut opt,
                    cur,
                    last_match_pos,
                    best_mlen,
                    best_off,
                );
                goto_encode = true;
                break;
            }

            {
                let base_litlen = opt[c].litlen;
                for litlen in 1..MINMATCH {
                    let price = opt[c].price - literals_price(base_litlen)
                        + literals_price(base_litlen + litlen);
                    let pos = cur + litlen;
                    let p = pos as usize;
                    if price < opt[p].price {
                        opt[p].mlen = 1;
                        opt[p].off = 0;
                        opt[p].litlen = base_litlen + litlen;
                        opt[p].price = price;
                    }
                }
            }

            {
                let match_ml = new_match.len;
                let offset = new_match.off;

                let (ll, base_price) = if opt[c].mlen == 1 {
                    let ll = opt[c].litlen;
                    let prev = if cur > ll {
                        opt[(cur - ll) as usize].price
                    } else {
                        0
                    };
                    (ll, prev)
                } else {
                    (0, opt[c].price)
                };
                let mut price = base_price + sequence_price(ll, MINMATCH);
                let mut next_bump = ML_MASK as i32 + MINMATCH;
                let mut ml = MINMATCH;
                while ml <= match_ml {
                    let pos = cur + ml;
                    let p = pos as usize;
                    if pos > last_match_pos + TRAILING_LITERALS as i32 || price <= opt[p].price {
                        if ml == match_ml && last_match_pos < pos {
                            last_match_pos = pos;
                        }
                        opt[p].mlen = ml;
                        opt[p].off = offset;
                        opt[p].litlen = ll;
                        opt[p].price = price;
                    }
                    ml += 1;
                    if ml == next_bump {
                        price += 1;
                        next_bump += 255;
                    }
                }
            }

            for add_lit in 1..=TRAILING_LITERALS {
                let p = last_match_pos as usize + add_lit;
                opt[p].mlen = 1;
                opt[p].off = 0;
                opt[p].litlen = add_lit as i32;
                opt[p].price = opt[last_match_pos as usize].price + literals_price(add_lit as i32);
            }

            cur += 1;
        }

        if goto_encode {
            continue;
        }

        best_mlen = opt[last_match_pos as usize].mlen;
        best_off = opt[last_match_pos as usize].off;
        cur = last_match_pos - best_mlen;

        encode_path(
            buf,
            &mut op,
            &mut ip,
            &mut anchor,
            &mut opt,
            cur,
            last_match_pos,
            best_mlen,
            best_off,
        );
    }

    emit_last_literals(buf, &mut op, anchor, iend);
    op
}

#[allow(clippy::too_many_arguments)]
fn encode_path(
    buf: &[u8],
    op: &mut Vec<u8>,
    ip: &mut usize,
    anchor: &mut usize,
    opt: &mut [Opt],
    cur: i32,
    last_match_pos: i32,
    best_mlen: i32,
    best_off: i32,
) {
    {
        let mut candidate_pos = cur;
        let mut selected_matchlength = best_mlen;
        let mut selected_offset = best_off;
        loop {
            let cp = candidate_pos as usize;
            let next_matchlength = opt[cp].mlen;
            let next_offset = opt[cp].off;
            opt[cp].mlen = selected_matchlength;
            opt[cp].off = selected_offset;
            selected_matchlength = next_matchlength;
            selected_offset = next_offset;
            if next_matchlength > candidate_pos {
                break;
            }
            candidate_pos -= next_matchlength;
        }
    }

    let mut r_pos = 0i32;
    while r_pos < last_match_pos {
        let ml = opt[r_pos as usize].mlen;
        let offset = opt[r_pos as usize].off;
        if ml == 1 {
            *ip += 1;
            r_pos += 1;
            continue;
        }
        r_pos += ml;
        encode_sequence(buf, op, ip, anchor, ml, offset);
    }
}

fn emit_last_literals(buf: &[u8], op: &mut Vec<u8>, anchor: usize, iend: usize) {
    let last_run_size = iend - anchor;
    if last_run_size >= RUN_MASK as usize {
        let mut accumulator = last_run_size - RUN_MASK as usize;
        op.push((RUN_MASK << ML_BITS) as u8);
        while accumulator >= 255 {
            op.push(255);
            accumulator -= 255;
        }
        op.push(accumulator as u8);
    } else {
        op.push((last_run_size << ML_BITS) as u8);
    }
    op.extend_from_slice(&buf[anchor..anchor + last_run_size]);
}

pub fn compress_hc(src: &[u8]) -> Vec<u8> {
    compress_optimal(src)
}
