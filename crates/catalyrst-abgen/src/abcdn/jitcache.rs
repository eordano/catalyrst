use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

static EVICT_SEQ: AtomicU64 = AtomicU64::new(0);

struct Entry {
    bytes: u64,
    refs: usize,
    tick: u64,
    path: PathBuf,
}

struct Inner {
    entries: HashMap<String, Entry>,
    order: BTreeMap<u64, String>,
    total: u64,
    clock: u64,
}

pub struct JitDiskCache {
    inner: Mutex<Inner>,
    budget: u64,
}

pub struct PinGuard {
    cache: Arc<JitDiskCache>,
    key: String,
}

impl Drop for PinGuard {
    fn drop(&mut self) {
        let mut inner = self.cache.lock();
        if let Some(e) = inner.entries.get_mut(&self.key) {
            e.refs = e.refs.saturating_sub(1);
            if e.refs == 0 && e.bytes == 0 {
                let tick = e.tick;
                inner.entries.remove(&self.key);
                inner.order.remove(&tick);
            }
        }
    }
}

fn bump(inner: &mut Inner, key: &str) {
    inner.clock += 1;
    let tick = inner.clock;
    if let Some(e) = inner.entries.get_mut(key) {
        inner.order.remove(&e.tick);
        e.tick = tick;
        inner.order.insert(tick, key.to_string());
    }
}

impl JitDiskCache {
    pub fn new(budget: u64) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner {
                entries: HashMap::new(),
                order: BTreeMap::new(),
                total: 0,
                clock: 0,
            }),
            budget,
        })
    }

    pub fn enabled(&self) -> bool {
        self.budget > 0
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn total_bytes(&self) -> u64 {
        self.lock().total
    }

    pub fn pin(self: &Arc<Self>, key: &str) -> Option<PinGuard> {
        if !self.enabled() {
            return None;
        }
        {
            let mut inner = self.lock();
            inner.clock += 1;
            let tick = inner.clock;
            let e = inner.entries.entry(key.to_string()).or_insert(Entry {
                bytes: 0,
                refs: 0,
                tick,
                path: PathBuf::new(),
            });
            e.refs += 1;
            let old = e.tick;
            e.tick = tick;
            inner.order.remove(&old);
            inner.order.insert(tick, key.to_string());
        }
        Some(PinGuard {
            cache: self.clone(),
            key: key.to_string(),
        })
    }

    pub fn record(&self, key: &str, path: PathBuf, bytes: u64) {
        if !self.enabled() {
            return;
        }
        let victims = {
            let mut inner = self.lock();
            let old = inner.entries.get(key).map(|e| (e.bytes, e.refs));
            match old {
                Some((old_bytes, refs)) => {
                    let new_bytes = if refs > 0 {
                        bytes.max(old_bytes)
                    } else {
                        bytes
                    };
                    {
                        let e = inner.entries.get_mut(key).unwrap();
                        e.bytes = new_bytes;
                        e.path = path;
                    }
                    inner.total = inner.total.saturating_sub(old_bytes) + new_bytes;
                }
                None => {
                    inner.clock += 1;
                    let tick = inner.clock;
                    inner.entries.insert(
                        key.to_string(),
                        Entry {
                            bytes,
                            refs: 0,
                            tick,
                            path,
                        },
                    );
                    inner.order.insert(tick, key.to_string());
                    inner.total += bytes;
                }
            }
            bump(&mut inner, key);
            evict_locked(&mut inner, self.budget)
        };
        delete(victims);
    }

    pub fn touch(&self, key: &str) {
        if !self.enabled() {
            return;
        }
        let mut inner = self.lock();
        if inner.entries.contains_key(key) {
            bump(&mut inner, key);
        }
    }

    pub fn seed_many(&self, items: impl IntoIterator<Item = (String, PathBuf, u64)>) {
        if !self.enabled() {
            return;
        }
        let victims = {
            let mut inner = self.lock();
            for (key, path, bytes) in items {
                if bytes == 0 || inner.entries.contains_key(&key) {
                    continue;
                }
                inner.clock += 1;
                let tick = inner.clock;
                inner.total += bytes;
                inner.entries.insert(
                    key.clone(),
                    Entry {
                        bytes,
                        refs: 0,
                        tick,
                        path,
                    },
                );
                inner.order.insert(tick, key);
            }
            evict_locked(&mut inner, self.budget)
        };
        delete(victims);
    }
}

fn evict_locked(inner: &mut Inner, budget: u64) -> Vec<PathBuf> {
    let mut victims = Vec::new();
    while inner.total > budget {
        let mut chosen: Option<(u64, String)> = None;
        for (tick, key) in inner.order.iter() {
            if let Some(e) = inner.entries.get(key) {
                if e.refs == 0 && e.bytes > 0 {
                    chosen = Some((*tick, key.clone()));
                    break;
                }
            }
        }
        let Some((tick, key)) = chosen else { break };
        if let Some(e) = inner.entries.remove(&key) {
            inner.total = inner.total.saturating_sub(e.bytes);
            if let Some(quarantined) = quarantine_rename(&e.path, inner.clock) {
                victims.push(quarantined);
            }
        }
        inner.order.remove(&tick);
    }
    victims
}

pub const EVICTED_SUFFIX: &str = ".evicted.";

fn quarantine_rename(path: &Path, clock: u64) -> Option<PathBuf> {
    let seq = EVICT_SEQ.fetch_add(1, Ordering::Relaxed);
    let mut os = path.as_os_str().to_owned();
    os.push(format!("{EVICTED_SUFFIX}{clock}.{seq}"));
    let dst = PathBuf::from(os);
    match std::fs::rename(path, &dst) {
        Ok(()) => Some(dst),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => Some(path.to_path_buf()),
    }
}

fn deleter() -> &'static std::sync::mpsc::Sender<PathBuf> {
    static TX: std::sync::OnceLock<std::sync::mpsc::Sender<PathBuf>> = std::sync::OnceLock::new();
    TX.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<PathBuf>();
        std::thread::spawn(move || {
            for p in rx {
                let _ = std::fs::remove_dir_all(&p);
                let _ = std::fs::remove_file(&p);
            }
        });
        tx
    })
}

fn delete(victims: Vec<PathBuf>) {
    let tx = deleter();
    for p in victims {
        let _ = tx.send(p);
    }
}

pub fn dir_size(path: &Path) -> u64 {
    let mut total = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for ent in rd.flatten() {
            let Ok(ft) = ent.file_type() else { continue };
            if ft.is_dir() {
                stack.push(ent.path());
                continue;
            }
            if ent
                .file_name()
                .to_str()
                .is_some_and(|n| n.contains(".tmp."))
            {
                continue;
            }
            if let Ok(md) = ent.metadata() {
                total += md.len();
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touchfile(dir: &Path, name: &str, bytes: usize) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(name), vec![0u8; bytes]).unwrap();
    }

    #[test]
    fn evicts_least_recently_used_over_budget() {
        let base = std::env::temp_dir().join(format!("jitcache-lru-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        for k in ["a", "b", "c"] {
            touchfile(&base.join(k), "f", 100);
        }
        let cache = JitDiskCache::new(250);
        cache.record("a", base.join("a"), 100);
        cache.record("b", base.join("b"), 100);
        cache.touch("a");
        cache.record("c", base.join("c"), 100);
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(base.join("a").exists(), "a was touched, must survive");
        assert!(base.join("c").exists(), "c is newest");
        assert!(!base.join("b").exists(), "b was LRU, must be evicted");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn evicts_bare_file_victim_over_budget() {
        let base = std::env::temp_dir().join(format!("jitcache-file-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(base.join("a"), vec![0u8; 100]).unwrap();
        std::fs::write(base.join("b"), vec![0u8; 100]).unwrap();
        let cache = JitDiskCache::new(150);
        cache.record("a", base.join("a"), 100);
        cache.record("b", base.join("b"), 100);
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(
            !base.join("a").exists(),
            "a is LRU bare-file victim, must be removed via remove_file"
        );
        assert!(base.join("b").is_file(), "b is newest bare file, survives");
        assert_eq!(
            cache.total_bytes(),
            100,
            "only b remains after file eviction"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn victim_original_path_renamed_away_synchronously() {
        let base = std::env::temp_dir().join(format!("jitcache-quar-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        for k in ["a", "b"] {
            touchfile(&base.join(k), "f", 100);
        }
        let cache = JitDiskCache::new(150);
        cache.record("a", base.join("a"), 100);
        cache.record("b", base.join("b"), 100);
        assert!(
            !base.join("a").exists(),
            "victim original path must be renamed to a quarantine sibling before record() returns, \
             so a same-path rebuild is never wiped by the async delete"
        );
        assert!(base.join("b").exists(), "b is newest, survives");
        std::thread::sleep(std::time::Duration::from_millis(50));
        let leftover: Vec<_> = std::fs::read_dir(&base)
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|n| n.contains(EVICTED_SUFFIX))
            })
            .collect();
        assert!(
            leftover.is_empty(),
            "quarantine sibling must be deleted off-thread"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn pinned_entry_is_not_evicted() {
        let base = std::env::temp_dir().join(format!("jitcache-pin-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        for k in ["a", "b"] {
            touchfile(&base.join(k), "f", 100);
        }
        let cache = JitDiskCache::new(100);
        cache.record("a", base.join("a"), 100);
        let guard = cache.pin("a");
        cache.record("b", base.join("b"), 100);
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(
            base.join("a").exists(),
            "a is pinned, must survive over budget"
        );
        assert!(!base.join("b").exists(), "b is unpinned LRU, evicted");
        drop(guard);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn disabled_budget_never_evicts() {
        let base = std::env::temp_dir().join(format!("jitcache-off-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        touchfile(&base.join("a"), "f", 100);
        let cache = JitDiskCache::new(0);
        cache.record("a", base.join("a"), 100);
        cache.record("b", base.join("b"), 100);
        std::thread::sleep(std::time::Duration::from_millis(30));
        assert!(base.join("a").exists());
        assert_eq!(cache.total_bytes(), 0);
        let _ = std::fs::remove_dir_all(&base);
    }
}

#[cfg(test)]
mod robustness_tests {
    use super::*;
    use std::thread;

    fn sum_bytes(c: &JitDiskCache) -> u64 {
        c.lock().entries.values().map(|e| e.bytes).sum()
    }

    fn mkdir(base: &Path, key: &str, bytes: usize) -> PathBuf {
        let d = base.join(key);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("f"), vec![0u8; bytes]).unwrap();
        d
    }

    #[test]
    fn re_record_shrink_then_grow_keeps_total_consistent() {
        let base = std::env::temp_dir().join(format!("jc-rerec-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let c = JitDiskCache::new(1_000_000);
        let p = mkdir(&base, "a", 10);
        c.record("a", p.clone(), 500);
        c.record("a", p.clone(), 50);
        c.record("a", p.clone(), 900);
        assert_eq!(c.total_bytes(), 900);
        assert_eq!(c.total_bytes(), sum_bytes(&c));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn record_after_eviction_no_underflow() {
        let base = std::env::temp_dir().join(format!("jc-evrec-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let c = JitDiskCache::new(250);
        for k in ["a", "b", "c", "d", "e"] {
            c.record(k, mkdir(&base, k, 10), 100);
        }
        assert!(c.total_bytes() <= 250);
        assert_eq!(c.total_bytes(), sum_bytes(&c));
        c.record("a", mkdir(&base, "a", 10), 100);
        assert!(c.total_bytes() <= 250);
        assert_eq!(c.total_bytes(), sum_bytes(&c));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn concurrent_hammer_stays_consistent_no_deadlock() {
        let base = std::env::temp_dir().join(format!("jc-conc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let c = JitDiskCache::new(50_000);
        let threads: Vec<_> = (0..8)
            .map(|t| {
                let c = c.clone();
                let base = base.clone();
                thread::spawn(move || {
                    for i in 0..500usize {
                        let key = format!("k{}", (t * 7 + i) % 40);
                        let d = base.join(&key);
                        let _ = std::fs::create_dir_all(&d);
                        let _ = std::fs::write(d.join("f"), vec![0u8; 100]);
                        let bytes = (100 + (i % 50)) as u64;
                        match i % 4 {
                            0 => c.record(&key, d, bytes),
                            1 => c.touch(&key),
                            2 => {
                                let g = c.pin(&key);
                                c.record(&key, d, bytes);
                                drop(g);
                            }
                            _ => c.record(&key, d, bytes),
                        }
                    }
                })
            })
            .collect();
        for h in threads {
            h.join().expect("worker thread must not panic");
        }
        assert_eq!(
            c.total_bytes(),
            sum_bytes(&c),
            "accounting invariant: total must equal sum of live entry bytes"
        );
        c.record("__flush__", base.join("__flush__"), 1);
        assert!(
            c.total_bytes() <= 50_000,
            "budget must hold after a flush record with no pins: {}",
            c.total_bytes()
        );
        thread::sleep(std::time::Duration::from_millis(150));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn record_takes_max_while_pinned_regardless_of_order() {
        let base = std::env::temp_dir().join(format!("jc-maxpin-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        for (dir, small_first) in [("small_first", true), ("large_first", false)] {
            let c = JitDiskCache::new(1_000_000);
            let p = mkdir(&base, dir, 10);
            let g = c.pin("b:cid");
            let (a, b) = if small_first { (100, 500) } else { (500, 100) };
            c.record("b:cid", p.clone(), a);
            c.record("b:cid", p.clone(), b);
            assert_eq!(
                c.total_bytes(),
                500,
                "pinned entry must converge to the full size ({dir})"
            );
            assert_eq!(c.total_bytes(), sum_bytes(&c));
            drop(g);
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn record_overwrites_when_unpinned() {
        let base = std::env::temp_dir().join(format!("jc-unpin-ovw-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let c = JitDiskCache::new(1_000_000);
        let p = mkdir(&base, "a", 10);
        c.record("a", p.clone(), 500);
        c.record("a", p.clone(), 100);
        assert_eq!(
            c.total_bytes(),
            100,
            "unpinned entry keeps last-writer size"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn seed_skips_zero_byte_entries() {
        let base = std::env::temp_dir().join(format!("jc-zero-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let c = JitDiskCache::new(1000);
        c.seed_many(vec![("empty".to_string(), base.join("empty"), 0)]);
        assert_eq!(c.lock().entries.len(), 0, "zero-byte seed must be skipped");
        c.seed_many(vec![("real".to_string(), base.join("real"), 200)]);
        assert_eq!(c.lock().entries.len(), 1);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn poisoned_lock_does_not_cascade() {
        let c = JitDiskCache::new(1000);
        let c2 = c.clone();
        let _ = thread::spawn(move || {
            let _g = c2.lock();
            panic!("poison the mutex on purpose");
        })
        .join();
        c.record("a", std::env::temp_dir().join("jc-poison-a"), 100);
        assert_eq!(c.total_bytes(), 100, "cache must survive a poisoned lock");
    }
}
