use super::*;
use crate::control_v4::{AssignmentPayload, LaneKey, V4Error, V4Owner};

pub(super) const POLL_INTERVAL: Duration = Duration::from_secs(2);

pub(super) struct LegacyFence {
    pub owner: V4Owner,
    last_revision: u64,
    delivered: bool,
}

fn realm() -> LaneKey {
    LaneKey::parse("realm").expect("static realm lane")
}

pub(super) async fn reserve(state: &AppState) -> Result<Option<u64>, V4Error> {
    if state.control_v4.is_available() || state.cfg.control_database_url.is_some() {
        state.control_v4.next_connection_epoch().await.map(Some)
    } else {
        Ok(None)
    }
}

impl LegacyFence {
    pub async fn claim(
        state: &AppState,
        address: &str,
        session: &str,
        epoch: u64,
    ) -> Result<(Self, PeerLink, SocketEvents), V4Error> {
        let owner = V4Owner {
            address: address.to_owned(),
            session: session.to_owned(),
            epoch,
        };
        state.control_v4.claim_lanes(&owner, &[realm()]).await?;
        let (link, events) = state.registry.on_v4_peer_connected_with_realm_epoch(
            address,
            session,
            &uuid::Uuid::new_v4().to_string(),
            true,
            epoch,
        );
        Ok((
            Self {
                owner,
                last_revision: 0,
                delivered: false,
            },
            link,
            events,
        ))
    }

    pub async fn poll(
        &mut self,
        state: &AppState,
        socket: &mut WebSocket,
        link: &PeerLink,
    ) -> bool {
        let snapshot = match state.control_v4.snapshot(&self.owner, &realm()).await {
            Ok(snapshot) => snapshot,
            Err(V4Error::AuthorityConflict) => {
                let _ = send_packet(socket, kicked_packet(), Some(link), &state.feed).await;
                return false;
            }
            Err(_) => return false,
        };
        if snapshot.assignment_revision <= self.last_revision {
            return true;
        }
        self.last_revision = snapshot.assignment_revision;
        let Some(assignment) = snapshot.assignment else {
            link.assignment_cleared();
            return !self.delivered;
        };
        if !send_packet(
            socket,
            server_packet::Message::IslandChanged(project(assignment)),
            Some(link),
            &state.feed,
        )
        .await
        {
            return false;
        }
        self.delivered = true;
        link.assignment_sent();
        state.feed.on_socket_assignment_written();
        true
    }
}

fn project(assignment: AssignmentPayload) -> IslandChangedMessage {
    IslandChangedMessage {
        island_id: assignment.island_id,
        conn_str: assignment.connection_string,
        from_island_id: assignment.from_island_id,
        peers: assignment
            .peers
            .into_iter()
            .filter_map(|(address, value)| v4::position_from_value(value).map(|p| (address, p)))
            .collect(),
        assignment_context: None,
    }
}
