const WIDTH: u32 = 256;
const CHUNKS: u32 = 6;
const MASK: u32 = WIDTH - 1;

const STARTDENOM: f64 = 281_474_976_710_656.0;
const SIGNIFICANCE: f64 = 4_503_599_627_370_496.0;
const OVERFLOW: f64 = 9_007_199_254_740_992.0;

fn to_int32(v: i64) -> i64 {
    (v as i32) as i64
}

fn mixkey(seed: &str) -> Vec<u8> {
    let units: Vec<u16> = seed.encode_utf16().collect();
    let mut key: Vec<Option<i64>> = vec![None; 256];
    let mut smear: i64 = 0;
    let mut max_written: i64 = -1;
    for (j, unit) in units.iter().enumerate() {
        let idx = (MASK as usize) & j;

        let cur = key[idx].unwrap_or(0);
        smear = to_int32(smear ^ to_int32(cur * 19));
        let mixed = to_int32(smear + *unit as i64);
        let byte = (MASK as i64) & mixed;
        key[idx] = Some(byte);
        if idx as i64 > max_written {
            max_written = idx as i64;
        }
    }

    if max_written < 0 {
        return Vec::new();
    }
    (0..=max_written as usize)
        .map(|i| key[i].unwrap_or(0) as u8)
        .collect()
}

struct Arc4 {
    s: [u32; 256],
    i: u32,
    j: u32,
}

impl Arc4 {
    fn new(key: &[u8]) -> Self {
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

        arc4.g(WIDTH);
        arc4
    }

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

        x = ((x as u32) >> 1) as f64;
    }
    (n + x) / d
}

pub fn det_shuffle<T, F>(items: &mut [T], id: F)
where
    F: Fn(&T) -> &str,
{
    let n = items.len();
    if n < 2 {
        return;
    }

    let cmp = |a: &T, b: &T| -> std::cmp::Ordering {
        let seed = format!("{}{}", id(a), id(b));
        if first_double(&seed) > 0.5 {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Less
        }
    };

    for start in 1..n {
        let mut left = 0usize;
        let mut right = start;
        while left < right {
            let mid = (left + right) >> 1;

            if cmp(&items[start], &items[mid]) == std::cmp::Ordering::Less {
                right = mid;
            } else {
                left = mid + 1;
            }
        }

        items[left..=start].rotate_right(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
