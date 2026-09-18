use std::time::Duration;

use tokio::sync::{mpsc, oneshot};

use super::PulseServer;
use crate::simulation::PeerConnectionState;

#[derive(Debug, Clone, PartialEq)]
pub struct PeerInspection {
    pub wallet: String,
    pub session: String,
    pub peer: u32,
    pub sequence: Option<u32>,
    pub server_tick: Option<u32>,
    pub inspected_at_tick: u32,
    pub position: Option<[f32; 3]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectionError {
    InvalidWallet,
    Unavailable,
}

#[derive(Clone)]
pub struct Handle(mpsc::Sender<Request>);

pub struct Requests(pub(super) mpsc::Receiver<Request>);

pub(super) struct Request {
    pub wallet: String,
    pub reply: oneshot::Sender<Option<PeerInspection>>,
}

pub fn channel() -> (Handle, Requests) {
    let (sender, receiver) = mpsc::channel(16);
    (Handle(sender), Requests(receiver))
}

impl Handle {
    pub async fn inspect(&self, wallet: &str) -> Result<Option<PeerInspection>, InspectionError> {
        if wallet.len() != 42
            || !wallet.starts_with("0x")
            || !wallet.as_bytes()[2..].iter().all(u8::is_ascii_hexdigit)
        {
            return Err(InspectionError::InvalidWallet);
        }
        let (reply, response) = oneshot::channel();
        self.0
            .try_send(Request {
                wallet: wallet.to_ascii_lowercase(),
                reply,
            })
            .map_err(|_| InspectionError::Unavailable)?;
        tokio::time::timeout(Duration::from_millis(500), response)
            .await
            .map_err(|_| InspectionError::Unavailable)?
            .map_err(|_| InspectionError::Unavailable)
    }
}

pub(super) fn snapshot(server: &PulseServer, wallet: &str) -> Option<PeerInspection> {
    let peer = server.identity.peer_by_wallet(wallet)?;
    let state = server.peers.get(&peer)?;
    if state.connection_state != PeerConnectionState::Authenticated
        || state.scene_listener.is_some()
        || !state.wallet_id.as_deref()?.eq_ignore_ascii_case(wallet)
        || !server
            .identity
            .wallet_by_peer(peer)?
            .eq_ignore_ascii_case(wallet)
    {
        return None;
    }
    let session = server.identity.session_by_peer(peer)?.to_owned();
    let state = server.board.try_read(peer);
    Some(PeerInspection {
        wallet: wallet.to_owned(),
        session,
        peer,
        sequence: state.map(|state| state.seq),
        server_tick: state.map(|state| state.server_tick),
        inspected_at_tick: 0,
        position: state.map(|state| {
            [
                state.global_position.x,
                state.global_position.y,
                state.global_position.z,
            ]
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{simulation::PeerState, snapshot::PeerSnapshot};

    #[test]
    fn inspection_reads_current_binding_without_mutating_and_rejects_retired_peers() {
        let wallet = format!("0x{}", "12".repeat(20));
        let session = format!("0x{}", "34".repeat(20));
        let mut server = PulseServer::new();
        assert_eq!(snapshot(&server, &wallet), None);
        let mut peer = PeerState::new(PeerConnectionState::Authenticated, 1);
        peer.wallet_id = Some(wallet.clone());
        server.peers.insert(0, peer);
        server
            .identity
            .set_with_session(0, wallet.clone(), session.clone());
        server.board.set_active(0);
        server.board.publish(
            0,
            PeerSnapshot {
                seq: 9,
                server_tick: 10,
                ..Default::default()
            },
        );
        let before = server.board.try_read(0).cloned();
        let found = snapshot(&server, &wallet).unwrap();
        assert_eq!(found.session, session);
        assert_eq!(found.sequence, Some(9));
        assert_eq!(server.board.try_read(0), before.as_ref());
        assert_eq!(server.peers.len(), 1);
        server.peers.get_mut(&0).unwrap().connection_state = PeerConnectionState::PendingDisconnect;
        assert_eq!(snapshot(&server, &wallet), None);
        server.peers.get_mut(&0).unwrap().connection_state = PeerConnectionState::Authenticated;
        server.peers.get_mut(&0).unwrap().wallet_id = Some("stale binding".into());
        assert_eq!(snapshot(&server, &wallet), None);
        server.identity.remove(0);
        assert_eq!(snapshot(&server, &wallet), None);
    }

    #[tokio::test]
    async fn inspection_is_bounded_and_closed_handles_fail() {
        let (handle, requests) = channel();
        assert_eq!(
            handle.inspect("invalid").await,
            Err(InspectionError::InvalidWallet)
        );
        drop(requests);
        assert_eq!(
            handle.inspect(&format!("0x{}", "12".repeat(20))).await,
            Err(InspectionError::Unavailable)
        );
        let (handle, _requests) = channel();
        for _ in 0..16 {
            let (reply, _) = oneshot::channel();
            handle
                .0
                .try_send(Request {
                    wallet: String::new(),
                    reply,
                })
                .unwrap();
        }
        assert_eq!(
            handle.inspect(&format!("0x{}", "12".repeat(20))).await,
            Err(InspectionError::Unavailable)
        );
        let (handle, _requests) = channel();
        let started = std::time::Instant::now();
        assert_eq!(
            handle.inspect(&format!("0x{}", "12".repeat(20))).await,
            Err(InspectionError::Unavailable)
        );
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
