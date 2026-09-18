//! The production [`ClusterGateway`]: this service's own ban store and LiveKit server API.

use std::time::Duration;

use async_trait::async_trait;

use crate::cluster_subscriber::{ClusterGateway, GatewayError};
use crate::livekit::{build_adapter_url, join_grants, with_not_before, AccessToken, Removal};
use crate::AppState;

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
            .map_err(|e| GatewayError(e.to_string()))
    }

    async fn mint_connection_string(
        &self,
        wallet: &str,
        room: &str,
        ttl_seconds: u64,
        not_before_unix: Option<u64>,
    ) -> Result<String, GatewayError> {
        let token = AccessToken::new(
            &self.state.livekit_api_key,
            &self.state.livekit_api_secret,
            wallet,
            join_grants(room),
        )
        .with_ttl(Duration::from_secs(ttl_seconds))
        .to_jwt()
        .map_err(|e| GatewayError(e.to_string()))?;

        let token = match not_before_unix {
            Some(not_before) => with_not_before(&token, &self.state.livekit_api_secret, not_before)
                .map_err(|e| GatewayError(e.to_string()))?,
            None => token,
        };

        Ok(build_adapter_url(&self.state.livekit_ws_url, &token))
    }

    async fn remove_participant(
        &self,
        room: &str,
        identity: &str,
        revoke_tokens_minted_before: Option<i64>,
    ) -> Result<Removal, GatewayError> {
        self.state
            .room_service()
            .remove_participant_revoking(room, identity, revoke_tokens_minted_before)
            .await
            .map_err(|e| GatewayError(e.to_string()))
    }

    async fn holds_participant(&self, room: &str, identity: &str) -> Result<bool, GatewayError> {
        self.state
            .room_service()
            .holds_participant(room, identity)
            .await
            .map_err(|e| GatewayError(e.to_string()))
    }
}
