use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::deployments_repository;
use crate::snapshots_repository::TimeRange;
use sqlx::PgPool;

const EXPECTED_ELEMENTS: usize = 5_000_000;
const FPR: f64 = 0.001;

fn optimal_bits(n: usize, p: f64) -> usize {
    let m = -(n as f64 * p.ln()) / (2.0_f64.ln().powi(2));
    m.ceil() as usize
}

fn optimal_k(m: usize, n: usize) -> usize {
    let k = (m as f64 / n as f64) * 2.0_f64.ln();
    k.ceil() as usize
}

struct BloomFilterInner {
    bits: Vec<u8>,
    num_bits: usize,
    k: usize,
}

impl BloomFilterInner {
    fn new(expected: usize, fpr: f64) -> Self {
        let num_bits = optimal_bits(expected, fpr);
        let k = optimal_k(num_bits, expected);
        let bytes = num_bits.div_ceil(8);
        Self {
            bits: vec![0u8; bytes],
            num_bits,
            k,
        }
    }

    fn add(&mut self, item: &str) {
        for i in 0..self.k {
            let idx = self.hash_index(item, i);
            self.bits[idx / 8] |= 1 << (idx % 8);
        }
    }

    fn has(&self, item: &str) -> bool {
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

struct Inner {
    filter: BloomFilterInner,
    loaded_time_ranges: Vec<TimeRange>,
    started_timestamp: Option<f64>,
}

impl Inner {
    fn is_time_range_loaded(&self, tr: &TimeRange) -> bool {
        self.loaded_time_ranges.iter().any(|loaded| {
            loaded.init_timestamp <= tr.init_timestamp && loaded.end_timestamp >= tr.end_timestamp
        })
    }

    fn add_time_range_loaded(&mut self, tr: TimeRange) {
        self.loaded_time_ranges.push(tr);
        self.loaded_time_ranges = join_overlapped_time_ranges(&self.loaded_time_ranges);
    }
}

#[derive(Clone)]
pub struct DeployedEntitiesBloomFilter {
    pool: PgPool,
    inner: Arc<RwLock<Inner>>,
}

impl DeployedEntitiesBloomFilter {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            inner: Arc::new(RwLock::new(Inner {
                filter: BloomFilterInner::new(EXPECTED_ELEMENTS, FPR),
                loaded_time_ranges: Vec::new(),
                started_timestamp: None,
            })),
        }
    }

    pub async fn add(&self, entity_id: &str) {
        let mut inner = self.inner.write().await;
        inner.filter.add(entity_id);
    }

    pub async fn is_probably_deployed(&self, entity_id: &str, entity_timestamp_ms: f64) -> bool {
        let inner = self.inner.read().await;

        let is_timestamp_loaded = inner
            .started_timestamp
            .map(|st| entity_timestamp_ms > st)
            .unwrap_or(false)
            || inner.loaded_time_ranges.iter().any(|tr| {
                tr.init_timestamp <= entity_timestamp_ms && tr.end_timestamp >= entity_timestamp_ms
            });

        if is_timestamp_loaded {
            return inner.filter.has(entity_id);
        }

        info!(
            entity_timestamp_ms,
            "Entity timestamp not loaded in bloom filter"
        );
        true
    }

    pub async fn add_all_in_time_range(&self, time_range: TimeRange) -> Result<(), sqlx::Error> {
        {
            let mut inner = self.inner.write().await;
            if inner.is_time_range_loaded(&time_range) {
                return Ok(());
            }
            inner.add_time_range_loaded(time_range);
        }

        let start = std::time::Instant::now();
        info!(
            init = time_range.init_timestamp,
            end = time_range.end_timestamp,
            "Loading bloom filter"
        );

        let entity_ids = deployments_repository::stream_all_entity_ids_in_time_range(
            &self.pool,
            time_range.init_timestamp,
            time_range.end_timestamp,
        )
        .await?;

        let elements = entity_ids.len();
        {
            let mut inner = self.inner.write().await;
            for eid in &entity_ids {
                inner.filter.add(eid);
            }
        }

        info!(
            elapsed_ms = start.elapsed().as_millis() as u64,
            elements, "Bloom filter loaded"
        );
        Ok(())
    }

    pub async fn start(&self) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis() as f64;
        let fifteen_min_ago = now - 15.0 * 60.0 * 1000.0;

        self.add_all_in_time_range(TimeRange {
            init_timestamp: fifteen_min_ago,
            end_timestamp: now,
        })
        .await?;

        {
            let mut inner = self.inner.write().await;
            inner.started_timestamp = Some(now);
        }

        Ok(())
    }
}

fn join_overlapped_time_ranges(ranges: &[TimeRange]) -> Vec<TimeRange> {
    if ranges.is_empty() {
        return Vec::new();
    }

    let mut sorted: Vec<TimeRange> = ranges.to_vec();
    sorted.sort_by(|a, b| {
        a.init_timestamp
            .partial_cmp(&b.init_timestamp)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut merged: Vec<TimeRange> = vec![sorted[0]];
    for tr in &sorted[1..] {
        let last = merged.last_mut().unwrap();
        if tr.init_timestamp <= last.end_timestamp {
            if tr.end_timestamp > last.end_timestamp {
                last.end_timestamp = tr.end_timestamp;
            }
        } else {
            merged.push(*tr);
        }
    }

    merged
}
