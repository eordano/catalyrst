use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::time::interval;

use crate::cluster::{Cluster, PeerState};
use crate::config::GossipConfig;

#[derive(Debug, Error)]
pub enum GossipError {
    #[error("gossip disabled (no hmac_key configured)")]
    Disabled,
    #[error("missing X-Archipelago-Node header")]
    MissingNode,
    #[error("missing X-Archipelago-Sig header")]
    MissingSig,
    #[error("missing X-Archipelago-Ts header")]
    MissingTs,
    #[error("invalid timestamp")]
    InvalidTs,
    #[error("timestamp skew {0}s outside window")]
    Skew(i64),
    #[error("signature mismatch")]
    BadSig,
    #[error("self-loop rejected")]
    SelfLoop,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GossipPeer {
    pub address: String,
    pub position: [f32; 3],
    pub parcel: [i32; 2],
    pub realm: String,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub last_heartbeat: DateTime<Utc>,
}

impl From<PeerState> for GossipPeer {
    fn from(p: PeerState) -> Self {
        GossipPeer {
            address: p.address,
            position: p.position,
            parcel: p.parcel,
            realm: p.realm,
            last_heartbeat: p.last_heartbeat,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GossipBatch {
    pub from_node: String,
    pub seq: u64,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub generated_at: DateTime<Utc>,
    pub peers: Vec<GossipPeer>,
}

pub struct GossipBus {
    cfg: GossipConfig,
    seq: Mutex<u64>,
    last_seen: Mutex<HashMap<String, u64>>,
    http: reqwest::Client,
}

impl GossipBus {
    pub fn new(cfg: GossipConfig, http: reqwest::Client) -> Arc<Self> {
        Arc::new(Self {
            cfg,
            seq: Mutex::new(0),
            last_seen: Mutex::new(HashMap::new()),
            http,
        })
    }

    pub fn is_armed(&self) -> bool {
        self.cfg.hmac_key.is_some() && self.cfg.node_id.is_some()
    }

    pub fn node_id(&self) -> &str {
        self.cfg.node_id.as_deref().unwrap_or("local")
    }

    pub fn peers_count(&self) -> usize {
        self.cfg.peers.len()
    }

    pub fn sign(&self, body: &[u8], ts: i64) -> Result<String, GossipError> {
        let key = self.cfg.hmac_key.as_deref().ok_or(GossipError::Disabled)?;
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key.as_bytes())
            .expect("HMAC accepts any key length");
        mac.update(ts.to_string().as_bytes());
        mac.update(b"\n");
        mac.update(body);
        Ok(STANDARD.encode(mac.finalize().into_bytes()))
    }

    pub fn verify(
        &self,
        body: &[u8],
        ts_header: &str,
        sig_header: &str,
        from_node: &str,
    ) -> Result<(), GossipError> {
        if Some(from_node) == self.cfg.node_id.as_deref() {
            return Err(GossipError::SelfLoop);
        }
        let ts: i64 = ts_header.parse().map_err(|_| GossipError::InvalidTs)?;
        let skew = (Utc::now().timestamp() - ts).abs();
        if skew > self.cfg.max_clock_skew_secs {
            return Err(GossipError::Skew(skew));
        }
        let want = self.sign(body, ts)?;
        if !constant_time_eq(want.as_bytes(), sig_header.as_bytes()) {
            return Err(GossipError::BadSig);
        }
        Ok(())
    }

    pub fn apply(&self, cluster: &Cluster, batch: GossipBatch) -> usize {
        let mut last = self.last_seen.lock();
        let prev = last.get(&batch.from_node).copied().unwrap_or(0);
        if batch.seq <= prev {
            return 0;
        }
        last.insert(batch.from_node.clone(), batch.seq);
        drop(last);
        let mut applied = 0usize;
        for p in batch.peers {
            cluster.upsert_peer_at(p.address, p.position, p.parcel, p.realm, p.last_heartbeat);
            applied += 1;
        }
        applied
    }

    pub async fn push_once(&self, cluster: Arc<Cluster>) -> usize {
        if !self.is_armed() || self.cfg.peers.is_empty() {
            return 0;
        }
        let snapshot: Vec<GossipPeer> = cluster
            .peers_snapshot()
            .into_iter()
            .map(Into::into)
            .collect();
        if snapshot.is_empty() {
            return 0;
        }
        let seq = {
            let mut s = self.seq.lock();
            *s += 1;
            *s
        };
        let batch = GossipBatch {
            from_node: self.node_id().to_string(),
            seq,
            generated_at: Utc::now(),
            peers: snapshot,
        };
        let body = match serde_json::to_vec(&batch) {
            Ok(b) => b,
            Err(_) => return 0,
        };
        let ts = Utc::now().timestamp();
        let sig = match self.sign(&body, ts) {
            Ok(s) => s,
            Err(_) => return 0,
        };
        let mut ok = 0usize;
        for peer in &self.cfg.peers {
            let url = format!("{}/gossip/heartbeat", peer.trim_end_matches('/'));
            let res = self
                .http
                .post(&url)
                .header("X-Archipelago-Node", self.node_id())
                .header("X-Archipelago-Sig", &sig)
                .header("X-Archipelago-Ts", ts.to_string())
                .header("content-type", "application/json")
                .body(body.clone())
                .send()
                .await;
            match res {
                Ok(r) if r.status().is_success() => ok += 1,
                Ok(r) => {
                    tracing::warn!(target: "gossip", url=%url, status=%r.status(), "peer rejected push")
                }
                Err(e) => tracing::warn!(target: "gossip", url=%url, error=%e, "peer push failed"),
            }
        }
        ok
    }

    pub fn spawn_periodic(
        self: Arc<Self>,
        cluster: Arc<Cluster>,
    ) -> Option<tokio::task::JoinHandle<()>> {
        if !self.is_armed() || self.cfg.peers.is_empty() {
            return None;
        }
        let secs = self.cfg.interval_secs.max(1);
        let handle = tokio::task::spawn(async move {
            let mut tick = interval(Duration::from_secs(secs));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            tick.tick().await;
            loop {
                tick.tick().await;
                let _ = self.push_once(cluster.clone()).await;
            }
        });
        Some(handle)
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn armed_cfg() -> GossipConfig {
        GossipConfig {
            node_id: Some("nodeA".into()),
            peers: vec!["http://peer1".into()],
            hmac_key: Some("supersecret".into()),
            interval_secs: 3,
            max_clock_skew_secs: 60,
        }
    }

    #[test]
    fn sign_then_verify_succeeds() {
        let a = GossipBus::new(armed_cfg(), reqwest::Client::new());
        let b = GossipBus::new(
            GossipConfig {
                node_id: Some("nodeB".into()),
                ..armed_cfg()
            },
            reqwest::Client::new(),
        );
        let body = b"{\"x\":1}";
        let ts = Utc::now().timestamp();
        let sig = a.sign(body, ts).unwrap();
        b.verify(body, &ts.to_string(), &sig, "nodeA").unwrap();
    }

    #[test]
    fn tampered_body_fails_verify() {
        let a = GossipBus::new(armed_cfg(), reqwest::Client::new());
        let b = GossipBus::new(
            GossipConfig {
                node_id: Some("nodeB".into()),
                ..armed_cfg()
            },
            reqwest::Client::new(),
        );
        let ts = Utc::now().timestamp();
        let sig = a.sign(b"{\"x\":1}", ts).unwrap();
        let err = b
            .verify(b"{\"x\":2}", &ts.to_string(), &sig, "nodeA")
            .unwrap_err();
        matches!(err, GossipError::BadSig);
    }

    #[test]
    fn self_loop_rejected() {
        let a = GossipBus::new(armed_cfg(), reqwest::Client::new());
        let ts = Utc::now().timestamp();
        let sig = a.sign(b"{}", ts).unwrap();
        let err = a.verify(b"{}", &ts.to_string(), &sig, "nodeA").unwrap_err();
        matches!(err, GossipError::SelfLoop);
    }

    #[test]
    fn skew_rejected() {
        let a = GossipBus::new(armed_cfg(), reqwest::Client::new());
        let b = GossipBus::new(
            GossipConfig {
                node_id: Some("nodeB".into()),
                ..armed_cfg()
            },
            reqwest::Client::new(),
        );
        let ts = Utc::now().timestamp() - 1_000_000;
        let sig = a.sign(b"{}", ts).unwrap();
        let err = b.verify(b"{}", &ts.to_string(), &sig, "nodeA").unwrap_err();
        matches!(err, GossipError::Skew(_));
    }
}
