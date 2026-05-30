use dashmap::DashMap;

#[derive(Default)]
pub struct PeersRegistry {
    wallet_to_world: DashMap<String, String>,
}

impl PeersRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_peer_world(&self, wallet: &str, world: &str) {
        self.wallet_to_world
            .insert(wallet.to_lowercase(), world.to_lowercase());
    }

    pub fn remove_peer(&self, wallet: &str) {
        self.wallet_to_world.remove(&wallet.to_lowercase());
    }

    pub fn get_peer_world(&self, wallet: &str) -> Option<String> {
        self.wallet_to_world
            .get(&wallet.to_lowercase())
            .map(|w| w.clone())
    }

    pub fn world_counts(&self) -> Vec<(String, i64)> {
        let mut counts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        for entry in self.wallet_to_world.iter() {
            *counts.entry(entry.value().clone()).or_insert(0) += 1;
        }
        let mut out: Vec<(String, i64)> = counts.into_iter().collect();
        out.sort();
        out
    }

    pub fn world_participant_count(&self, world: &str) -> i64 {
        let needle = world.to_lowercase();
        self.wallet_to_world
            .iter()
            .filter(|e| *e.value() == needle)
            .count() as i64
    }
}
