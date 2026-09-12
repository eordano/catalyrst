use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::error::FedError;

/// Canonical form: trimmed and ASCII-lowercased. Every id reaching the registry has passed
/// through [`canonical_peer_id`] once, at parse time, so there is one spelling of a peer per
/// process and a lookup cannot miss on case.
pub type PeerId = String;

/// The one definition of what a peer id is, for the whole workspace. Never write
/// `to_ascii_lowercase` at a call site: two callers each holding a private idea of "the id"
/// is how two peer-file entries came to share one mirror namespace, the second poll erasing
/// the first's rows.
///
/// Peer ids are host names, hence case-insensitive (RFC 4343); a file listing both spellings
/// is listing one peer twice -- see [`FedError::DuplicatePeerId`]. ASCII-only on purpose: a
/// Unicode fold is locale-shaped and not idempotent, and the `remote_worlds` CHECK
/// constraints assert `peer_id = lower(peer_id)` against Postgres's ASCII-for-ASCII
/// `lower()`.
pub fn canonical_peer_id(raw: &str) -> PeerId {
    raw.trim().to_ascii_lowercase()
}

fn default_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerCert {
    #[serde(default = "default_version")]
    pub version: u32,
    /// As written in the file when the struct is built by hand; canonical in every `PeerCert`
    /// from [`FederationRegistry::parse_file`], so the registry holds no raw ids at all.
    pub peer_id: PeerId,
    pub catalyst_url: String,
    /// Distinct from `catalyst_url`, which is a *content* server base. Empty means the peer
    /// runs no worlds server; worlds federation then omits it rather than guessing a URL from
    /// `catalyst_url`.
    #[serde(default)]
    pub worlds_url: String,
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

    /// Two entries whose ids canonicalise to the same value are a refusal naming both, never
    /// a merge: keeping either would make "we federate with X" and "we federate with the
    /// *other* X" the same observable state, settled by TOML document order, with no later
    /// moment at which anyone finds out which happened.
    fn parse_file(path: &Path) -> Result<HashMap<PeerId, PeerCert>, FedError> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| FedError::Malformed(format!("peer file {}: {e}", path.display())))?;
        let parsed: PeerFile = toml::from_str(&raw)
            .map_err(|e| FedError::Malformed(format!("peer file {}: {e}", path.display())))?;

        let mut map = HashMap::with_capacity(parsed.peer.len());
        let mut as_written: HashMap<PeerId, String> = HashMap::with_capacity(parsed.peer.len());
        for mut p in parsed.peer {
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
            let canonical = canonical_peer_id(&p.peer_id);
            if let Some(first) = as_written.get(&canonical) {
                return Err(FedError::DuplicatePeerId {
                    canonical,
                    first: first.clone(),
                    second: p.peer_id.clone(),
                });
            }
            as_written.insert(canonical.clone(), p.peer_id.clone());
            p.peer_id = canonical.clone();
            map.insert(canonical, p);
        }

        Ok(map)
    }

    /// Case-insensitive: [`canonical_peer_id`] is applied to both the needle and the key.
    pub fn contains(&self, peer: &str) -> bool {
        self.peers.read().contains_key(&canonical_peer_id(peer))
    }

    pub fn get(&self, peer: &str) -> Option<PeerCert> {
        self.peers.read().get(&canonical_peer_id(peer)).cloned()
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
