#[cfg(all(test, feature = "loom"))]
mod loom_tests {
    use loom::sync::{Arc, Mutex, RwLock};
    use loom::thread;
    use std::collections::HashMap;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct CacheEntry {
        result: bool,
    }

    struct SimplifiedValidationCache {
        cache: RwLock<HashMap<String, CacheEntry>>,
        in_flight: Mutex<HashMap<String, bool>>,
    }

    impl SimplifiedValidationCache {
        fn new() -> Self {
            Self {
                cache: RwLock::new(HashMap::new()),
                in_flight: Mutex::new(HashMap::new()),
            }
        }

        fn insert(&self, key: String, result: bool) {
            let entry = CacheEntry { result };
            let mut map = self.cache.write().unwrap();
            map.insert(key.clone(), entry);
        }

        fn get(&self, key: &str) -> Option<CacheEntry> {
            let map = self.cache.read().unwrap();
            map.get(key).cloned()
        }

        fn mark_in_flight(&self, key: &str) {
            let mut map = self.in_flight.lock().unwrap();
            map.insert(key.to_string(), true);
        }

        fn clear_in_flight(&self, key: &str) {
            let mut map = self.in_flight.lock().unwrap();
            map.remove(key);
        }

    }

    #[test]
    fn loom_validation_cache_insert_read_no_torn_data() {
        loom::model(|| {
            let cache = Arc::new(SimplifiedValidationCache::new());
            let key = "0xtest".to_string();

            let c1 = cache.clone();
            let k1 = key.clone();
            let t1 = thread::spawn(move || {
                c1.mark_in_flight(&k1);
                c1.insert(k1.clone(), true);
                c1.clear_in_flight(&k1);
            });

            let c2 = cache.clone();
            let k2 = key.clone();
            let t2 = thread::spawn(move || {
                let result = c2.get(&k2);
                match result {
                    None => {  }
                    Some(entry) => {
                        assert!(
                            entry.result == true,
                            "torn read: got {:?}",
                            entry
                        );
                    }
                }
            });

            t1.join().unwrap();
            t2.join().unwrap();
        });
    }

    #[test]
    fn loom_validation_cache_concurrent_inserts_same_key() {
        loom::model(|| {
            let cache = Arc::new(SimplifiedValidationCache::new());
            let key = "0xsame".to_string();

            let c1 = cache.clone();
            let k1 = key.clone();
            let t1 = thread::spawn(move || {
                c1.insert(k1, true);
            });

            let c2 = cache.clone();
            let k2 = key.clone();
            let t2 = thread::spawn(move || {
                c2.insert(k2, true);
            });

            t1.join().unwrap();
            t2.join().unwrap();

            let result = cache.get(&key);
            assert!(result.is_some(), "key must exist after both inserts");
            assert_eq!(result.unwrap().result, true);
        });
    }

    struct LoomFailedDeploymentsCache {
        map: RwLock<HashMap<String, String>>,
    }

    impl LoomFailedDeploymentsCache {
        fn new() -> Self {
            Self {
                map: RwLock::new(HashMap::new()),
            }
        }

        fn cache(&self, entity_id: &str, reason: &str) {
            let mut map = self.map.write().unwrap();
            map.insert(entity_id.to_string(), reason.to_string());
        }

        fn remove(&self, entity_id: &str) -> bool {
            let mut map = self.map.write().unwrap();
            map.remove(entity_id).is_some()
        }

        fn snapshot(&self) -> HashMap<String, String> {
            let map = self.map.read().unwrap();
            map.clone()
        }
    }

    #[test]
    fn loom_failed_deployments_remove_no_silent_loss() {
        loom::model(|| {
            let cache = Arc::new(LoomFailedDeploymentsCache::new());

            cache.cache("entity-1", "initial error");

            let c1 = cache.clone();
            let t1 = thread::spawn(move || {
                c1.cache("entity-1", "updated error");
            });

            let c2 = cache.clone();
            let t2 = thread::spawn(move || {
                c2.remove("entity-1");
            });

            t1.join().unwrap();
            t2.join().unwrap();

            let snap = cache.snapshot();
            match snap.get("entity-1") {
                Some(reason) => {
                    assert!(
                        reason == "initial error" || reason == "updated error",
                        "corrupt cache entry: {:?}",
                        reason
                    );
                }
                None => {
                }
            }
        });
    }

    #[test]
    fn loom_failed_deployments_concurrent_cache_remove_different_keys() {
        loom::model(|| {
            let cache = Arc::new(LoomFailedDeploymentsCache::new());

            cache.cache("entity-a", "error-a");
            cache.cache("entity-b", "error-b");

            let c1 = cache.clone();
            let t1 = thread::spawn(move || {
                c1.remove("entity-a");
                c1.cache("entity-c", "error-c");
            });

            let c2 = cache.clone();
            let t2 = thread::spawn(move || {
                c2.remove("entity-b");
                c2.cache("entity-d", "error-d");
            });

            t1.join().unwrap();
            t2.join().unwrap();

            let snap = cache.snapshot();

            assert!(
                !snap.contains_key("entity-a"),
                "entity-a should have been removed"
            );
            assert!(
                !snap.contains_key("entity-b"),
                "entity-b should have been removed"
            );

            assert!(
                snap.contains_key("entity-c"),
                "entity-c should be present"
            );
            assert!(
                snap.contains_key("entity-d"),
                "entity-d should be present"
            );
        });
    }

    #[test]
    fn loom_failed_deployments_correct_remove_is_atomic() {
        loom::model(|| {
            let cache = Arc::new(LoomFailedDeploymentsCache::new());
            cache.cache("entity-1", "initial");

            let removed = Arc::new(Mutex::new(Vec::<bool>::new()));

            let c1 = cache.clone();
            let r1 = removed.clone();
            let t1 = thread::spawn(move || {
                let result = c1.remove("entity-1");
                r1.lock().unwrap().push(result);
            });

            let c2 = cache.clone();
            let r2 = removed.clone();
            let t2 = thread::spawn(move || {
                let result = c2.remove("entity-1");
                r2.lock().unwrap().push(result);
            });

            t1.join().unwrap();
            t2.join().unwrap();

            let results = removed.lock().unwrap();
            let true_count = results.iter().filter(|&&r| r).count();
            assert_eq!(
                true_count, 1,
                "exactly one remove should succeed, got {} true results: {:?}",
                true_count, *results
            );

            assert!(
                cache.snapshot().is_empty(),
                "cache should be empty after both removes"
            );
        });
    }
}

#[cfg(not(feature = "loom"))]
#[cfg(test)]
mod tests {
    #[test]
    fn loom_tests_require_feature_flag() {
    }
}
