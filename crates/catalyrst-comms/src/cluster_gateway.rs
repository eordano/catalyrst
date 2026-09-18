//! The production [`ClusterGateway`]: this service's own ban store and LiveKit server API.

use async_trait::async_trait;

use crate::assignment_fence::TokenFence;
use crate::cluster_subscriber::{ClusterGateway, GatewayError, IslandOccupant};
use crate::livekit::Removal;
use crate::AppState;

mod island;
pub use island::{island_occupant, mint_island_connection_string};

pub struct StateGateway {
    state: AppState,
}

impl StateGateway {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[async_trait]
impl ClusterGateway for StateGateway {
    async fn is_denied(&self, wallet: &str) -> Result<bool, GatewayError> {
        crate::access_gate::is_connection_banned(&self.state, wallet, None)
            .await
            .map_err(|e| GatewayError::new(e.to_string()))
    }

    async fn mint_connection_string(
        &self,
        wallet: &str,
        session: Option<&str>,
        room: &str,
        ttl_seconds: u64,
        not_before_unix: Option<u64>,
    ) -> Result<String, GatewayError> {
        mint_island_connection_string(
            &self.state.livekit_api_key,
            &self.state.livekit_api_secret,
            &self.state.livekit_ws_url,
            wallet,
            session,
            room,
            ttl_seconds,
            not_before_unix,
        )
    }

    async fn mint_fenced_connection_string(
        &self,
        wallet: &str,
        room: &str,
        ttl_seconds: u64,
        fence: &TokenFence,
    ) -> Result<String, GatewayError> {
        island::mint_fenced_island_connection_string(
            &self.state.livekit_api_key,
            &self.state.livekit_api_secret,
            &self.state.livekit_ws_url,
            wallet,
            room,
            ttl_seconds,
            fence,
        )
    }

    async fn remove_participant(
        &self,
        room: &str,
        identity: &str,
        revoke_tokens_minted_before: Option<i64>,
    ) -> Result<Removal, GatewayError> {
        self.state
            .room_service()
            .remove_participant_with_cutoff(room, identity, revoke_tokens_minted_before)
            .await
            .map_err(|e| GatewayError::new(e.to_string()))
    }

    async fn holds_participant(&self, room: &str, identity: &str) -> Result<bool, GatewayError> {
        self.state
            .room_service()
            .holds_participant(room, identity)
            .await
            .map_err(|e| GatewayError::new(e.to_string()))
    }

    async fn island_occupant(
        &self,
        room: &str,
        wallet: &str,
    ) -> Result<IslandOccupant, GatewayError> {
        island_occupant(&self.state.room_service(), room, wallet).await
    }
}
