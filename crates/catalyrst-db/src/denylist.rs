use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

#[derive(Debug, Clone, Default)]
pub struct DenylistConfig {
    pub file_path: Option<PathBuf>,
    pub urls: Vec<String>,
}

impl DenylistConfig {
    pub fn from_env() -> Self {
        let file_path = std::env::var("DENYLIST_FILE_NAME")
            .ok()
            .filter(|s| !s.is_empty())
            .map(PathBuf::from);

        let urls: Vec<String> = std::env::var("DENYLIST_URLS")
            .unwrap_or_default()
            .split(|c: char| c.is_whitespace())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        Self { file_path, urls }
    }
}

#[derive(Clone)]
pub struct Denylist {
    config: DenylistConfig,
    denied: Arc<RwLock<HashSet<String>>>,
}

impl Denylist {
    pub fn new(config: DenylistConfig) -> Self {
        Self {
            config,
            denied: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    pub async fn is_denylisted(&self, id: &str) -> bool {
        let denied = self.denied.read().await;
        let result = denied.contains(id);
        if result {
            info!(id, "Processing denylisted entityId");
        }
        result
    }

    pub async fn load(&self) {
        let mut new_set = HashSet::new();

        if let Some(ref path) = self.config.file_path {
            match tokio::fs::read_to_string(path).await {
                Ok(content) => {
                    for line in content.lines() {
                        let cid = line.trim();
                        if !cid.starts_with('#') && !cid.is_empty() {
                            new_set.insert(cid.to_string());
                        }
                    }
                }
                Err(e) => {
                    if e.kind() != std::io::ErrorKind::NotFound {
                        error!(?e, "Failed to read denylist file");
                    }
                }
            }
        }

        for url in &self.config.urls {
            match reqwest_fetch_entity_ids(url).await {
                Ok(ids) => {
                    for id in ids {
                        if !id.is_empty() {
                            new_set.insert(id);
                        }
                    }
                }
                Err(e) => {
                    error!(%url, %e, "Failed to fetch denylist from URL");
                }
            }
        }

        let mut denied = self.denied.write().await;
        *denied = new_set;
    }

    pub async fn start(&self) {
        self.load().await;

        let this = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(120));
            interval.tick().await;
            loop {
                interval.tick().await;
                this.load().await;
            }
        });
    }
}

#[derive(serde::Deserialize)]
struct DenylistEntry {
    entity_id: String,
}

async fn reqwest_fetch_entity_ids(url: &str) -> Result<Vec<String>, String> {
    let response = reqwest::get(url)
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "HTTP {} from denylist URL {url}",
            response.status()
        ));
    }

    let entries: Vec<DenylistEntry> = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse denylist JSON from {url}: {e}"))?;

    Ok(entries.into_iter().map(|e| e.entity_id).collect())
}
