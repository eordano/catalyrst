use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::error::FedError;

pub type PeerId = String;

fn default_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerCert {
    #[serde(default = "default_version")]
    pub version: u32,
    pub peer_id: PeerId,
    pub catalyst_url: String,
    pub gossip_pubkey: [u8; 32],
    #[serde(default)]
    pub mtls_root_pem: String,
    #[serde(default)]
    pub dao_proposal: String,
    #[serde(default)]
    pub added_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PeerAudit {
    pub peer_id: PeerId,
    pub dao_proposal: String,
    pub added_at: String,
}

#[derive(Debug, Deserialize)]
struct PeerFile {
    #[serde(default)]
    peer: Vec<PeerCert>,
}

#[derive(Debug, Default)]
pub struct FederationRegistry {
    peers: RwLock<HashMap<PeerId, PeerCert>>,
}

impl FederationRegistry {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn load_static(peers: impl IntoIterator<Item = PeerCert>) -> Arc<Self> {
        let reg = Self::default();
        {
            let mut w = reg.peers.write();
            for p in peers {
                w.insert(p.peer_id.clone(), p);
            }
        }
        Arc::new(reg)
    }

    pub fn from_file(path: &Path) -> Result<Arc<Self>, FedError> {
        let map = Self::parse_file(path)?;
        let reg = Self::default();
        *reg.peers.write() = map;
        Ok(Arc::new(reg))
    }

    pub fn reload(&self, path: &Path) -> Result<(), FedError> {
        let map = Self::parse_file(path)?;
        *self.peers.write() = map;
        Ok(())
    }

    fn parse_file(path: &Path) -> Result<HashMap<PeerId, PeerCert>, FedError> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| FedError::Malformed(format!("peer file {}: {e}", path.display())))?;
        let parsed: PeerFile = toml::from_str(&raw)
            .map_err(|e| FedError::Malformed(format!("peer file {}: {e}", path.display())))?;

        let mut map = HashMap::with_capacity(parsed.peer.len());
        for p in parsed.peer {
            if p.peer_id.trim().is_empty() {
                return Err(FedError::Malformed("peer_id is empty".into()));
            }
            if p.catalyst_url.trim().is_empty() {
                return Err(FedError::Malformed(format!(
                    "peer {}: catalyst_url is empty",
                    p.peer_id
                )));
            }
            if p.dao_proposal.trim().is_empty() {
                return Err(FedError::Malformed(format!(
                    "peer {}: dao_proposal is required (link to snapshot.dcl.eth proposal)",
                    p.peer_id
                )));
            }
            if p.added_at.trim().is_empty() {
                return Err(FedError::Malformed(format!(
                    "peer {}: added_at is required",
                    p.peer_id
                )));
            }
            map.insert(p.peer_id.clone(), p);
        }
        Ok(map)
    }

    pub fn contains(&self, peer: &str) -> bool {
        self.peers.read().contains_key(peer)
    }

    pub fn get(&self, peer: &str) -> Option<PeerCert> {
        self.peers.read().get(peer).cloned()
    }

    pub fn all(&self) -> Vec<PeerCert> {
        self.peers.read().values().cloned().collect()
    }

    pub fn audit(&self) -> Vec<PeerAudit> {
        self.peers
            .read()
            .values()
            .map(|p| PeerAudit {
                peer_id: p.peer_id.clone(),
                dao_proposal: p.dao_proposal.clone(),
                added_at: p.added_at.clone(),
            })
            .collect()
    }
}
