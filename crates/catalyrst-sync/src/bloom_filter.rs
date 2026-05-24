use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

const EXPECTED_ELEMENTS: usize = 3_000_000;
const FPR: f64 = 0.01;

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
        let num_bits = optimal_bits(EXPECTED_ELEMENTS, FPR);
        let k = optimal_k(num_bits, EXPECTED_ELEMENTS);
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
