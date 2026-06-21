//! Byte-faithful port of `seedrandom` v3.0.5 (David Bau), the npm package
//! upstream marketplace-server uses to deterministically shuffle the trendings
//! result (`ports/trendings/component.ts:92`:
//! `sort((a, b) => seedrandom(a.id + b.id)() > 0.5 ? 1 : -1)`).
//!
//! Only the default string-seeded ARC4 generator's first `prng()` output is
//! reproduced — that is all the trendings comparator consumes. The float
//! arithmetic mirrors the JS reference (doubles in both), so `first_double(seed)`
//! returns the identical IEEE-754 value `seedrandom(seed)()` produces.

const WIDTH: u32 = 256;
const CHUNKS: u32 = 6;
const MASK: u32 = WIDTH - 1; // 255
                             // startdenom = 256^6 = 2^48, significance = 2^52, overflow = 2^53. All exact in f64.
const STARTDENOM: f64 = 281_474_976_710_656.0; // 256^6
const SIGNIFICANCE: f64 = 4_503_599_627_370_496.0; // 2^52
const OVERFLOW: f64 = 9_007_199_254_740_992.0; // 2^53

/// JS `ToInt32` then re-widened: `^`/`*` in the seed mixer run in 32-bit signed
/// integer space. We carry values as `i64` and truncate to the low 32 bits with
/// signed interpretation, exactly as ECMAScript ToInt32 does.
fn to_int32(v: i64) -> i64 {
    (v as i32) as i64
}

/// `mixkey(seed, key)` — folds the UTF-16 code units of `seed` into `key`
/// (an array of bytes index by `mask & j`). `smear` starts as JS `undefined`,
/// which `ToInt32`s to 0 on the first `^=`.
fn mixkey(seed: &str) -> Vec<u8> {
    // key is a sparse array in JS; index range is [0, 256). We model it as a
    // 256-slot map of Option<i64> to reproduce the `undefined * 19` (-> 0 under
    // ToInt32) behavior for not-yet-written slots, then collect the written
    // prefix into the ARC4 key exactly as `tostring(key)` -> charCodes would.
    let units: Vec<u16> = seed.encode_utf16().collect();
    let mut key: Vec<Option<i64>> = vec![None; 256];
    let mut smear: i64 = 0; // ToInt32(undefined) == 0
    let mut max_written: i64 = -1;
    for (j, unit) in units.iter().enumerate() {
        let idx = (MASK as usize) & j;
        // key[mask&j] (undefined -> 0 under ToInt32) * 19
        let cur = key[idx].unwrap_or(0);
        smear = to_int32(smear ^ to_int32(cur * 19));
        let mixed = to_int32(smear + *unit as i64);
        let byte = (MASK as i64) & mixed; // mask & (...) -> 0..=255
        key[idx] = Some(byte);
        if idx as i64 > max_written {
            max_written = idx as i64;
        }
    }
    // `tostring(key)` walks key[0..key.length]; key.length is max written index
    // + 1. Unwritten holes inside that range stringify as charCode 0 — but the
    // ARC4 key only ever indexes by `i % keylen`, and an empty seed yields an
    // empty key (handled by ARC4). Collect the dense prefix.
    if max_written < 0 {
        return Vec::new();
    }
    (0..=max_written as usize)
        .map(|i| key[i].unwrap_or(0) as u8)
        .collect()
}

/// ARC4 state (the `S` permutation plus the `i`/`j` cursors).
struct Arc4 {
    s: [u32; 256],
    i: u32,
    j: u32,
}

impl Arc4 {
    fn new(key: &[u8]) -> Self {
        // Empty key [] is treated as [0].
        let key: Vec<u32> = if key.is_empty() {
            vec![0]
        } else {
            key.iter().map(|&b| b as u32).collect()
        };
        let keylen = key.len() as u32;

        let mut s = [0u32; 256];
        for i in 0..256u32 {
            s[i as usize] = i;
        }
        let mut j: u32 = 0;
        for i in 0..256u32 {
            let t = s[i as usize];
            j = MASK & (j.wrapping_add(key[(i % keylen) as usize]).wrapping_add(t));
            s[i as usize] = s[j as usize];
            s[j as usize] = t;
        }
        let mut arc4 = Arc4 { s, i: 0, j: 0 };
        // RC4-drop[256]: the constructor immediately discards `width` outputs.
        arc4.g(WIDTH);
        arc4
    }

    /// `g(count)` — concatenate the next `count` ARC4 bytes into one number
    /// `0 <= r < 256^count`. Returned as f64 (exact for count<=6: 256^6=2^48).
    fn g(&mut self, count: u32) -> f64 {
        let mut r: f64 = 0.0;
        let mut i = self.i;
        let mut j = self.j;
        for _ in 0..count {
            i = MASK & (i.wrapping_add(1));
            let t = self.s[i as usize];
            j = MASK & (j.wrapping_add(t));
            self.s[i as usize] = self.s[j as usize];
            self.s[j as usize] = t;
            let idx = MASK & (self.s[i as usize].wrapping_add(self.s[j as usize]));
            r = r * (WIDTH as f64) + self.s[idx as usize] as f64;
        }
        self.i = i;
        self.j = j;
        r
    }
}

/// The default `prng()` double in `[0, 1)` for the first call after seeding,
/// reproducing `seedrandom(seed)()`.
pub fn first_double(seed: &str) -> f64 {
    let key = mixkey(seed);
    let mut arc4 = Arc4::new(&key);

    let mut n = arc4.g(CHUNKS);
    let mut d = STARTDENOM;
    let mut x: f64 = 0.0;
    while n < SIGNIFICANCE {
        n = (n + x) * (WIDTH as f64);
        d *= WIDTH as f64;
        x = arc4.g(1);
    }
    while n >= OVERFLOW {
        n /= 2.0;
        d /= 2.0;
        // x >>>= 1 — JS unsigned 32-bit shift. x is a single byte (0..=255) here.
        x = ((x as u32) >> 1) as f64;
    }
    (n + x) / d
}

/// Deterministically reorder `items` to match upstream's
/// `array.sort((a, b) => seedrandom(id(a) + id(b))() > 0.5 ? 1 : -1)`.
///
/// The comparator is inconsistent, so the result is defined by V8's sort
/// internals. For arrays shorter than V8's `MIN_MERGE` (32) — the trendings
/// array is at most `floor(size*0.6) + floor(size*0.4)` ≈ 20 — TimSort degrades
/// to a single binary-insertion-sort pass, which we port verbatim here so the
/// served order is byte-identical to Node's.
pub fn det_shuffle<T, F>(items: &mut [T], id: F)
where
    F: Fn(&T) -> &str,
{
    let n = items.len();
    if n < 2 {
        return;
    }
    // V8 `comparefn(a, b)` -> Ordering for our pivot-vs-array[mid] calls.
    let cmp = |a: &T, b: &T| -> std::cmp::Ordering {
        let seed = format!("{}{}", id(a), id(b));
        if first_double(&seed) > 0.5 {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Less
        }
    };
    // V8 BinaryInsertionSort over [0, n) with start = 1 (low == 0, so start++).
    for start in 1..n {
        // Binary search for the insertion point of `items[start]` in [0, start).
        let mut left = 0usize;
        let mut right = start;
        while left < right {
            let mid = (left + right) >> 1;
            // comparefn(pivot, array[mid]) < 0  ->  right = mid
            if cmp(&items[start], &items[mid]) == std::cmp::Ordering::Less {
                right = mid;
            } else {
                left = mid + 1;
            }
        }
        // Rotate items[left..=start] right by one (pivot lands at `left`),
        // matching V8's element shift.
        items[left..=start].rotate_right(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reference values produced by node:
    //   node -e 'console.log(require("seedrandom")("<seed>")())'
    // (seedrandom@3.0.5). These pin the ARC4 + prng pipeline byte-for-byte.
    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-12, "expected {b}, got {a}");
    }

    #[test]
    fn known_vectors() {
        approx(first_double("hello"), 0.5463663768140734);
        approx(first_double(""), 0.23144008215179881);
        approx(first_double("a"), 0.43449421599986604);
        approx(first_double("0xabc0xdef"), 0.1646894869769447);
        approx(first_double("💀emoji"), 0.6852670915645748);
        approx(first_double("x"), 0.9080614664401105);
        approx(first_double("0x1"), 0.4480108581306423);
        approx(first_double("long-seed-value-123"), 0.9333971911046186);
    }

    #[test]
    fn deterministic() {
        assert_eq!(first_double("seed-xyz"), first_double("seed-xyz"));
    }

    #[test]
    fn in_unit_interval() {
        for s in ["", "x", "0x1", "long-seed-value-123", "💀emoji"] {
            let v = first_double(s);
            assert!((0.0..1.0).contains(&v), "{s} -> {v} out of [0,1)");
        }
    }

    // Reference outputs from Node (seedrandom@3.0.5, V8 Array.sort):
    //   arr.sort((a,b)=> seedrandom(a.id+b.id)() > 0.5 ? 1 : -1)
    fn shuffle_ids(ids: &[&str]) -> Vec<String> {
        let mut v: Vec<String> = ids.iter().map(|s| s.to_string()).collect();
        det_shuffle(&mut v, |s| s.as_str());
        v
    }

    #[test]
    fn det_shuffle_matches_v8_sort() {
        let ids = [
            "0xaaa-1", "0xbbb-2", "0xccc-3", "0xddd-4", "0xeee-5", "0xfff-6", "0xggg-7", "0xhhh-8",
            "0xiii-9", "0xjjj-10", "0xkkk-11", "0xlll-12",
        ];
        assert_eq!(
            shuffle_ids(&ids),
            vec![
                "0xjjj-10", "0xddd-4", "0xccc-3", "0xaaa-1", "0xkkk-11", "0xeee-5", "0xhhh-8",
                "0xggg-7", "0xlll-12", "0xfff-6", "0xbbb-2", "0xiii-9",
            ]
        );
        assert_eq!(shuffle_ids(&ids[..2]), vec!["0xaaa-1", "0xbbb-2"]);
        assert_eq!(
            shuffle_ids(&ids[..3]),
            vec!["0xccc-3", "0xaaa-1", "0xbbb-2"]
        );
        assert_eq!(
            shuffle_ids(&ids[..5]),
            vec!["0xddd-4", "0xccc-3", "0xaaa-1", "0xeee-5", "0xbbb-2"]
        );
        assert_eq!(
            shuffle_ids(&ids[..8]),
            vec![
                "0xddd-4", "0xccc-3", "0xaaa-1", "0xeee-5", "0xhhh-8", "0xggg-7", "0xfff-6",
                "0xbbb-2"
            ]
        );
    }
}
