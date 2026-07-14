use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::key::TileKey;

#[derive(Default, Serialize, Deserialize, Clone)]
struct QuarantineState {
    keys: HashMap<String, Entry>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Entry {
    pub failures: u32,
    pub until: u64,
}

pub struct Quarantine {
    path: PathBuf,
    max_failures: u32,
    quarantine_secs: u64,
    state: Mutex<QuarantineState>,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl Quarantine {
    pub fn load(path: PathBuf, max_failures: u32, quarantine_secs: u64) -> Self {
        let state = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            path,
            max_failures,
            quarantine_secs,
            state: Mutex::new(state),
        }
    }

    pub fn is_quarantined(&self, tile: &TileKey) -> bool {
        let state = self.state.lock().unwrap();
        state
            .keys
            .get(&tile.label())
            .map(|entry| entry.until > now_secs())
            .unwrap_or(false)
    }

    pub fn record_failure(&self, tile: &TileKey) {
        let mut state = self.state.lock().unwrap();
        {
            let entry = state.keys.entry(tile.label()).or_insert(Entry {
                failures: 0,
                until: 0,
            });
            entry.failures += 1;
            if entry.failures >= self.max_failures {
                entry.until = now_secs() + self.quarantine_secs;
            }
        }
        self.persist(&state);
    }

    pub fn record_success(&self, tile: &TileKey) {
        let mut state = self.state.lock().unwrap();
        if state.keys.remove(&tile.label()).is_some() {
            self.persist(&state);
        }
    }

    pub fn entries(&self) -> Vec<(String, Entry)> {
        let now = now_secs();
        let state = self.state.lock().unwrap();
        let mut entries: Vec<_> = state
            .keys
            .iter()
            .filter(|(_, entry)| entry.until > now)
            .map(|(key, entry)| (key.clone(), *entry))
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries
    }

    fn persist(&self, state: &QuarantineState) {
        let Ok(bytes) = serde_json::to_vec_pretty(state) else {
            return;
        };
        let tmp = self.path.with_extension("json.tmp");
        if std::fs::write(&tmp, &bytes).is_ok() {
            let _ = std::fs::rename(&tmp, &self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quarantines_after_max_failures() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("quarantine.json");
        let q = Quarantine::load(path, 3, 86400);
        let tile = TileKey::new(0, 5, -3).unwrap();
        q.record_failure(&tile);
        q.record_failure(&tile);
        assert!(!q.is_quarantined(&tile));
        q.record_failure(&tile);
        assert!(q.is_quarantined(&tile));
        let entries = q.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "0/5,-3");
        assert_eq!(entries[0].1.failures, 3);
    }

    #[test]
    fn success_clears_counter() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("quarantine.json");
        let q = Quarantine::load(path, 2, 86400);
        let tile = TileKey::new(1, 2, 4).unwrap();
        q.record_failure(&tile);
        q.record_failure(&tile);
        assert!(q.is_quarantined(&tile));
        q.record_success(&tile);
        assert!(!q.is_quarantined(&tile));
        assert!(q.entries().is_empty());
    }

    #[test]
    fn ttl_expires() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("quarantine.json");
        let q = Quarantine::load(path, 1, 0);
        let tile = TileKey::new(0, 0, 0).unwrap();
        q.record_failure(&tile);
        assert!(!q.is_quarantined(&tile));
        assert!(q.entries().is_empty());
    }

    #[test]
    fn entries_lists_only_active_quarantines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("quarantine.json");
        let q = Quarantine::load(path, 2, 86400);
        let below = TileKey::new(1, 0, 0).unwrap();
        let poisoned = TileKey::new(0, 3, 3).unwrap();
        q.record_failure(&below);
        q.record_failure(&poisoned);
        q.record_failure(&poisoned);
        let entries = q.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "0/3,3");
        assert_eq!(entries[0].1.failures, 2);
    }

    #[test]
    fn persists_across_boots() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("quarantine.json");
        let tile = TileKey::new(2, -4, 8).unwrap();
        {
            let q = Quarantine::load(path.clone(), 2, 86400);
            q.record_failure(&tile);
            q.record_failure(&tile);
            assert!(q.is_quarantined(&tile));
        }
        let q = Quarantine::load(path, 2, 86400);
        assert!(q.is_quarantined(&tile));
        assert_eq!(q.entries()[0].1.failures, 2);
    }
}
