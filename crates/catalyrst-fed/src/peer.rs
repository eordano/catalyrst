use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::error::FedError;

/// A peer id in **canonical** form: trimmed and ASCII-lowercased.
///
/// Still a `String` alias, because the registry is shared with scopes that only ever
/// echo the id back. What changed is that every id reaching it has passed through
/// [`canonical_peer_id`] exactly once, at parse time, so there is one spelling of a
/// peer per process and a lookup cannot miss on case.
pub type PeerId = String;

/// The **one** definition of what a peer id is, for the whole workspace.
///
/// Peer ids are host names. Host names are case-insensitive (RFC 4343), so
/// `Peer.Example.ORG` and `peer.example.org` name one peer, and a peer file listing
/// both is listing one peer twice -- see [`FedError::DuplicatePeerId`], which is how
/// that is answered.
///
/// This function exists because the alternative was measured and it failed: the
/// registry keyed its map on the raw string while `catalyrst-worlds` lowercased the
/// same string before minting its own id type. Two callers each holding a private
/// idea of "the id" is how two file entries -- two DAO proposals, two pinned roots,
/// two hosts -- came to share one mirror namespace, with the second poll silently
/// erasing the first's rows. Call this; do not write `to_ascii_lowercase` at a call
/// site, because that is the divergence, re-introduced.
///
/// ASCII-only on purpose. A Unicode fold is locale-shaped and not idempotent for
/// every input, and the `remote_worlds` CHECK constraints assert `peer_id =
/// lower(peer_id)` against Postgres's ASCII-for-ASCII `lower()`. Matching the
/// database exactly is worth more here than folding ids nobody will ever write.
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
    /// As written in the file when the struct is built by hand; **canonical** in every
    /// `PeerCert` that came out of [`FederationRegistry::parse_file`], which rewrites
    /// it through [`canonical_peer_id`] before storing it. The registry therefore has
    /// no raw ids in it at all, and no consumer has to remember to fold one.
    pub peer_id: PeerId,
    pub catalyst_url: String,
    /// Base URL of this peer's worlds server, if it runs one.
    ///
    /// Distinct from `catalyst_url`, which is a *content* server base. A peer may
    /// federate communities or places and run no worlds server at all; an empty
    /// value means exactly that, and worlds federation omits the peer rather than
    /// guessing a URL from `catalyst_url`.
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

    /// Parse and adjudicate the file into a map keyed by [`canonical_peer_id`].
    ///
    /// Two entries whose ids canonicalise to the same value are a **refusal naming
    /// both**, never a merge. `HashMap::insert` returns the displaced value and this
    /// loop used to drop it on the floor, so a peer file could contain two complete,
    /// differing entries -- two DAO proposals, two pinned roots, two hosts -- and boot a
    /// server that had silently kept one of them. Which one depended on TOML document
    /// order, which nobody was choosing deliberately.
    ///
    /// Refusing is the fail-closed direction and it is the only one that stays true:
    /// keeping either entry would make "we federate with X" and "we federate with the
    /// *other* X" the same observable state, and there is no later moment at which
    /// anyone finds out which happened.
    fn parse_file(path: &Path) -> Result<HashMap<PeerId, PeerCert>, FedError> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| FedError::Malformed(format!("peer file {}: {e}", path.display())))?;
        let parsed: PeerFile = toml::from_str(&raw)
            .map_err(|e| FedError::Malformed(format!("peer file {}: {e}", path.display())))?;

        let mut map = HashMap::with_capacity(parsed.peer.len());
        // canonical id -> the id exactly as the operator wrote it, kept only so a
        // collision can be reported in the spelling they will find in the file.
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
            // Canonicalise in place, so nothing downstream ever sees the raw spelling
            // and no consumer can re-derive a second, differing idea of the id.
            p.peer_id = canonical.clone();
            map.insert(canonical, p);
        }

        // A file that names nobody is deliberately NOT refused here.
        //
        // `PeerFile.peer` is `#[serde(default)]`, so `[[peers]]` instead of
        // `[[peer]]` - one character, still valid TOML - parses to zero entries, as
        // does an empty or truncated file. That is a real hazard, but refusing it
        // here would also destroy the one legitimate way to say "federation is on
        // and we currently trust nobody", which is the distinction
        // `WorldsFederationPeers`'s two-state enum exists to preserve.
        //
        // The danger was never the empty set itself; it was that an empty admitted
        // set made the reconcile sweep delete every mirrored row, because
        // `peer_id <> ALL('{}')` is true for all of them. That is guarded at the
        // sweep instead - see `revoke_peers_no_longer_admitted`, which refuses to
        // run when the admitted set is empty. A delete-everything sweep is exactly
        // the case that should stop and ask rather than proceed silently.
        Ok(map)
    }

    /// Case-insensitive, because [`canonical_peer_id`] is applied to both the needle
    /// and the key. A caller holding an id from a request path, a log line, or another
    /// peer file cannot miss a peer it does hold by spelling it differently.
    pub fn contains(&self, peer: &str) -> bool {
        self.peers.read().contains_key(&canonical_peer_id(peer))
    }

    /// See [`Self::contains`] on canonicalisation of the needle.
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
