use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

const DEFAULT_EXPECTED_ELEMENTS: usize = 10_000_000;
const FPR: f64 = 0.01;

fn expected_elements_from_env() -> usize {
    std::env::var("BLOOM_FILTER_EXPECTED_ELEMENTS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(DEFAULT_EXPECTED_ELEMENTS)
}

fn optimal_bits(n: usize, p: f64) -> usize {
    let m = -(n as f64 * p.ln()) / (2.0_f64.ln().powi(2));
    m.ceil() as usize
}

fn optimal_k(m: usize, n: usize) -> usize {
    let k = (m as f64 / n as f64) * 2.0_f64.ln();
    k.ceil() as usize
}

pub struct BloomFilter {
    bits: Vec<u8>,
    num_bits: usize,
    k: usize,
}

impl Default for BloomFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl BloomFilter {
    pub fn new() -> Self {
        Self::with_capacity(expected_elements_from_env())
    }

    pub fn with_capacity(expected_elements: usize) -> Self {
        let expected_elements = expected_elements.max(1);
        let num_bits = optimal_bits(expected_elements, FPR);
        let k = optimal_k(num_bits, expected_elements);
        let bytes = num_bits.div_ceil(8);
        Self {
            bits: vec![0u8; bytes],
            num_bits,
            k,
        }
    }

    pub fn add(&mut self, item: &str) {
        for i in 0..self.k {
            let idx = self.hash_index(item, i);
            self.bits[idx / 8] |= 1 << (idx % 8);
        }
    }

    pub fn maybe_contains(&self, item: &str) -> bool {
        for i in 0..self.k {
            let idx = self.hash_index(item, i);
            if self.bits[idx / 8] & (1 << (idx % 8)) == 0 {
                return false;
            }
        }
        true
    }

    fn hash_index(&self, item: &str, seed: usize) -> usize {
        let mut hasher = DefaultHasher::new();
        item.hash(&mut hasher);
        seed.hash(&mut hasher);
        (hasher.finish() as usize) % self.num_bits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_capacity_membership() {
        let mut bf = BloomFilter::with_capacity(1000);
        bf.add("entity-a");
        bf.add("entity-b");
        assert!(bf.maybe_contains("entity-a"));
        assert!(bf.maybe_contains("entity-b"));

        assert!(!bf.maybe_contains("entity-never-added"));
    }

    #[test]
    fn with_capacity_zero_is_safe() {
        let mut bf = BloomFilter::with_capacity(0);
        bf.add("x");
        assert!(bf.maybe_contains("x"));
    }

    #[test]
    fn larger_capacity_allocates_more_bits() {
        let small = BloomFilter::with_capacity(1_000);
        let large = BloomFilter::with_capacity(10_000_000);
        assert!(large.num_bits > small.num_bits);
    }
}
