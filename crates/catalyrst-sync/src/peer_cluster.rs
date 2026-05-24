use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::{watch, Mutex, Notify};
use tracing::{error, info};

use crate::{CatalystServerInfo, DaoClient, SyncError};

pub type SyncFinishedCallback = Box<dyn Fn(&HashSet<String>) + Send + Sync>;

pub struct PeerClusterConfig {
    pub own_content_url: String,
    pub sync_interval_ms: u64,
}

pub struct PeerCluster {
    config: PeerClusterConfig,
    dao_client: Arc<dyn DaoClient>,
    servers: Arc<Mutex<HashSet<String>>>,
    last_sync_time: Arc<Mutex<i64>>,
    server_tx: watch::Sender<HashSet<String>>,
    server_rx: watch::Receiver<HashSet<String>>,
    stop_notify: Arc<Notify>,
}

impl PeerCluster {
    pub fn new(config: PeerClusterConfig, dao_client: Arc<dyn DaoClient>) -> Self {
        let (server_tx, server_rx) = watch::channel(HashSet::new());
        PeerCluster {
            config,
            dao_client,
            servers: Arc::new(Mutex::new(HashSet::new())),
            last_sync_time: Arc::new(Mutex::new(0)),
            server_tx,
            server_rx,
            stop_notify: Arc::new(Notify::new()),
        }
    }

    pub async fn sync_with_dao(&self) -> Result<Vec<String>, SyncError> {
        let all_servers = self.dao_client.get_all_content_servers().await?;

        if all_servers.is_empty() {
            return Err(SyncError::Other("DAO returned no servers".into()));
        }

        let normalized_own = normalize_content_url(&self.config.own_content_url);
        let other_urls: Vec<String> = all_servers
            .into_iter()
            .map(|s| s.address)
            .filter(|addr| normalize_content_url(addr) != normalized_own)
            .collect();

        let mut servers = self.servers.lock().await;

        servers.retain(|url| other_urls.contains(url));

        for url in &other_urls {
            if servers.insert(url.clone()) {
                info!(server = %url, "Discovered new peer server");
            }
        }

        *self.last_sync_time.lock().await = chrono::Utc::now().timestamp_millis();

        let _ = self.server_tx.send(servers.clone());

        let result: Vec<String> = servers.iter().cloned().collect();
        Ok(result)
    }

    pub fn start(&self) -> tokio::task::JoinHandle<()> {
        let dao_client = self.dao_client.clone();
        let servers = self.servers.clone();
        let last_sync_time = self.last_sync_time.clone();
        let server_tx = self.server_tx.clone();
        let stop = self.stop_notify.clone();
        let interval_ms = self.config.sync_interval_ms;
        let normalized_own = normalize_content_url(&self.config.own_content_url);

        tokio::spawn(async move {
            info!(interval_ms, "Starting peer cluster DAO sync loop");

            loop {
                tokio::select! {
                    _ = stop.notified() => {
                        info!("Peer cluster sync loop stopped");
                        return;
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_millis(interval_ms)) => {
                        match dao_client.get_all_content_servers().await {
                            Ok(all_servers) => {
                                let other_urls: Vec<String> = all_servers
                                    .into_iter()
                                    .map(|s| s.address)
                                    .filter(|addr| normalize_content_url(addr) != normalized_own)
                                    .collect();

                                let mut srv = servers.lock().await;
                                srv.retain(|url| other_urls.contains(url));
                                for url in &other_urls {
                                    if srv.insert(url.clone()) {
                                        info!(server = %url, "Discovered new peer server");
                                    }
                                }
                                *last_sync_time.lock().await = chrono::Utc::now().timestamp_millis();
                                let _ = server_tx.send(srv.clone());
                            }
                            Err(e) => {
                                error!(error = %e, "Failed to sync with DAO");
                            }
                        }
                    }
                }
            }
        })
    }

    pub fn stop(&self) {
        self.stop_notify.notify_one();
    }

    pub async fn get_all_servers(&self) -> Vec<String> {
        self.servers.lock().await.iter().cloned().collect()
    }

    pub fn subscribe(&self) -> watch::Receiver<HashSet<String>> {
        self.server_rx.clone()
    }

    pub async fn last_sync_timestamp(&self) -> i64 {
        *self.last_sync_time.lock().await
    }
}

fn normalize_content_url(url: &str) -> String {
    url.to_lowercase().trim_end_matches('/').to_string()
}

pub struct StaticDaoClient {
    servers: Vec<CatalystServerInfo>,
}

impl StaticDaoClient {
    pub fn from_csv(csv: &str) -> Self {
        let servers = csv
            .split(',')
            .enumerate()
            .map(|(i, addr)| CatalystServerInfo {
                address: format!("{}/content", addr.trim()),
                owner: "0x0000000000000000000000000000000000000000".to_string(),
                id: format!("{:x}", i),
            })
            .collect();
        StaticDaoClient { servers }
    }
}

#[async_trait::async_trait]
impl DaoClient for StaticDaoClient {
    async fn get_all_content_servers(&self) -> Result<Vec<CatalystServerInfo>, SyncError> {
        Ok(self.servers.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_content_url() {
        assert_eq!(
            normalize_content_url("https://Peer.example.com/content/"),
            "https://peer.example.com/content"
        );
        assert_eq!(
            normalize_content_url("https://peer.example.com/content"),
            "https://peer.example.com/content"
        );
    }

    #[test]
    fn test_static_dao_client_from_csv() {
        let client = StaticDaoClient::from_csv("https://a.com, https://b.com");
        assert_eq!(client.servers.len(), 2);
        assert_eq!(client.servers[0].address, "https://a.com/content");
        assert_eq!(client.servers[1].address, "https://b.com/content");
    }
}
