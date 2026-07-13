use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct PointerLockManager {
    inner: Arc<Mutex<HashMap<String, HashSet<String>>>>,
}

impl Default for PointerLockManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PointerLockManager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn try_acquire(&self, entity_type: &str, pointers: &[String]) -> Vec<String> {
        let mut map = self.inner.lock().await;
        let in_flight = map.entry(entity_type.to_string()).or_default();

        let conflicts: Vec<String> = pointers
            .iter()
            .filter(|p| in_flight.contains(p.as_str()))
            .cloned()
            .collect();

        if !conflicts.is_empty() {
            return conflicts;
        }

        for p in pointers {
            in_flight.insert(p.clone());
        }

        Vec::new()
    }

    pub async fn release(&self, entity_type: &str, pointers: &[String]) {
        let mut map = self.inner.lock().await;
        if let Some(in_flight) = map.get_mut(entity_type) {
            for p in pointers {
                in_flight.remove(p.as_str());
            }
            if in_flight.is_empty() {
                map.remove(entity_type);
            }
        }
    }
}
