use super::identity::UpstreamIdentity;
use crate::rpc::proto::v2::*;
use catalyrst_drpc::client::{RpcClient, RpcClientModule, ServiceClient};
use catalyrst_drpc::transports::web_sockets::tungstenite::{TungsteniteWebSocket, WebSocketClient};
use catalyrst_drpc::transports::web_sockets::{
    Message as WsMessage, WebSocket, WebSocketTransport,
};
use catalyrst_drpc::transports::Transport;
use std::time::Duration;

pub const CALL_TIMEOUT: Duration = Duration::from_secs(15);
pub const LIVE_TEST_VAR: &str = "UPSTREAM_SOCIAL_LIVE_TEST";
const PORT_NAME: &str = "social";
const MODULE_NAME: &str = "SocialService";

type UpstreamTransport = WebSocketTransport<TungsteniteWebSocket, ()>;

#[derive(Clone, PartialEq, prost::Message)]
struct EmptyPayload {}

struct SocialModule<T: Transport + 'static>(RpcClientModule<T>);

impl<T: Transport + 'static> ServiceClient<T> for SocialModule<T> {
    fn set_client_module(client_module: RpcClientModule<T>) -> Self {
        Self(client_module)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FriendshipMutation {
    Request {
        peer: String,
        message: Option<String>,
    },
    Accept {
        peer: String,
    },
    Reject {
        peer: String,
    },
    Cancel {
        peer: String,
    },
    Delete {
        peer: String,
    },
}

impl FriendshipMutation {
    pub fn into_payload(self) -> UpsertFriendshipPayload {
        use upsert_friendship_payload::*;
        let user = |address: String| Some(User { address });
        let action = match self {
            Self::Request { peer, message } => Action::Request(RequestPayload {
                user: user(peer),
                message,
            }),
            Self::Accept { peer } => Action::Accept(AcceptPayload { user: user(peer) }),
            Self::Reject { peer } => Action::Reject(RejectPayload { user: user(peer) }),
            Self::Cancel { peer } => Action::Cancel(CancelPayload { user: user(peer) }),
            Self::Delete { peer } => Action::Delete(DeletePayload { user: user(peer) }),
        };
        UpsertFriendshipPayload {
            action: Some(action),
        }
    }
}

pub struct UpstreamSession {
    _client: RpcClient<UpstreamTransport>,
    module: SocialModule<UpstreamTransport>,
    address: String,
}

impl UpstreamSession {
    pub async fn connect(url: &str, identity: &UpstreamIdentity) -> anyhow::Result<Self> {
        let uri: axum::http::Uri = url.parse()?;
        let sign_path = uri.path().to_string();
        Self::connect_signing(url, &sign_path, identity).await
    }

    pub async fn connect_signing(
        url: &str,
        sign_path: &str,
        identity: &UpstreamIdentity,
    ) -> anyhow::Result<Self> {
        let ws = WebSocketClient::connect(url)
            .await
            .map_err(|e| anyhow::anyhow!("connect: {e:?}"))?;
        ws.send(WsMessage::Text(identity.ws_auth_frame(sign_path)?))
            .await
            .map_err(|e| anyhow::anyhow!("auth frame send: {e:?}"))?;
        let transport = WebSocketTransport::new(ws);
        let mut client = tokio::time::timeout(CALL_TIMEOUT, RpcClient::new(transport))
            .await
            .map_err(|_| anyhow::anyhow!("handshake timeout (auth likely rejected)"))?
            .map_err(|e| anyhow::anyhow!("handshake: {e:?}"))?;
        let module = {
            let port = client
                .create_port(PORT_NAME)
                .await
                .map_err(|e| anyhow::anyhow!("create_port: {e:?}"))?;
            port.load_module::<SocialModule<_>>(MODULE_NAME)
                .await
                .map_err(|e| anyhow::anyhow!("load_module: {e:?}"))?
        };
        Ok(Self {
            _client: client,
            module,
            address: identity.address(),
        })
    }

    pub fn address(&self) -> &str {
        &self.address
    }

    async fn call<Req, Resp>(&self, name: &str, payload: Req) -> anyhow::Result<Resp>
    where
        Req: prost::Message + Default,
        Resp: prost::Message + Default,
    {
        match tokio::time::timeout(
            CALL_TIMEOUT,
            self.module
                .0
                .call_unary_procedure::<Resp, Req>(name, payload),
        )
        .await
        {
            Ok(Ok(resp)) => Ok(resp),
            Ok(Err(e)) => Err(anyhow::anyhow!("{name}: {e:?}")),
            Err(_) => Err(anyhow::anyhow!("{name}: timeout")),
        }
    }

    pub async fn get_friends(
        &self,
        pagination: Option<Pagination>,
    ) -> anyhow::Result<PaginatedFriendsProfilesResponse> {
        self.call("GetFriends", GetFriendsPayload { pagination })
            .await
    }

    pub async fn get_pending_friendship_requests(
        &self,
        pagination: Option<Pagination>,
    ) -> anyhow::Result<PaginatedFriendshipRequestsResponse> {
        self.call(
            "GetPendingFriendshipRequests",
            GetFriendshipRequestsPayload { pagination },
        )
        .await
    }

    pub async fn get_sent_friendship_requests(
        &self,
        pagination: Option<Pagination>,
    ) -> anyhow::Result<PaginatedFriendshipRequestsResponse> {
        self.call(
            "GetSentFriendshipRequests",
            GetFriendshipRequestsPayload { pagination },
        )
        .await
    }

    pub async fn get_blocked_users(
        &self,
        pagination: Option<Pagination>,
    ) -> anyhow::Result<GetBlockedUsersResponse> {
        self.call("GetBlockedUsers", GetBlockedUsersPayload { pagination })
            .await
    }

    pub async fn get_social_settings(&self) -> anyhow::Result<GetSocialSettingsResponse> {
        self.call("GetSocialSettings", EmptyPayload {}).await
    }

    pub async fn get_friendship_status(
        &self,
        peer: &str,
    ) -> anyhow::Result<GetFriendshipStatusResponse> {
        self.call(
            "GetFriendshipStatus",
            GetFriendshipStatusPayload {
                user: Some(User {
                    address: peer.to_string(),
                }),
            },
        )
        .await
    }

    pub async fn write_through(
        &self,
        mutation: FriendshipMutation,
    ) -> anyhow::Result<UpsertFriendshipResponse> {
        self.write_through_payload(mutation.into_payload()).await
    }

    pub async fn write_through_payload(
        &self,
        payload: UpsertFriendshipPayload,
    ) -> anyhow::Result<UpsertFriendshipResponse> {
        self.call("UpsertFriendship", payload).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message;

    fn peer() -> String {
        "0x000000000000000000000000000000000000beef".to_string()
    }

    fn mutations() -> Vec<FriendshipMutation> {
        vec![
            FriendshipMutation::Request {
                peer: peer(),
                message: Some("hi".into()),
            },
            FriendshipMutation::Request {
                peer: peer(),
                message: None,
            },
            FriendshipMutation::Accept { peer: peer() },
            FriendshipMutation::Reject { peer: peer() },
            FriendshipMutation::Cancel { peer: peer() },
            FriendshipMutation::Delete { peer: peer() },
        ]
    }

    #[test]
    fn write_through_payloads_roundtrip_over_the_wire_encoding() {
        for mutation in mutations() {
            let payload = mutation.into_payload();
            let bytes = payload.encode_to_vec();
            let decoded = UpsertFriendshipPayload::decode(bytes.as_slice()).unwrap();
            assert_eq!(decoded, payload);
        }
    }

    #[test]
    fn each_mutation_maps_to_its_action_variant() {
        use upsert_friendship_payload::Action;
        let actions: Vec<Action> = mutations()
            .into_iter()
            .map(|m| m.into_payload().action.unwrap())
            .collect();
        assert!(matches!(
            &actions[0],
            Action::Request(r) if r.message.as_deref() == Some("hi")
                && r.user.as_ref().unwrap().address == peer()
        ));
        assert!(matches!(&actions[1], Action::Request(r) if r.message.is_none()));
        assert!(
            matches!(&actions[2], Action::Accept(a) if a.user.as_ref().unwrap().address == peer())
        );
        assert!(matches!(&actions[3], Action::Reject(_)));
        assert!(matches!(&actions[4], Action::Cancel(_)));
        assert!(matches!(&actions[5], Action::Delete(_)));
    }

    #[test]
    fn bootstrap_read_payloads_roundtrip() {
        let friends = GetFriendsPayload {
            pagination: Some(Pagination {
                limit: 100,
                offset: 0,
            }),
        };
        let bytes = friends.encode_to_vec();
        assert_eq!(
            GetFriendsPayload::decode(bytes.as_slice()).unwrap(),
            friends
        );
        let status = GetFriendshipStatusPayload {
            user: Some(User { address: peer() }),
        };
        let bytes = status.encode_to_vec();
        assert_eq!(
            GetFriendshipStatusPayload::decode(bytes.as_slice()).unwrap(),
            status
        );
    }

    #[tokio::test]
    async fn live_upstream_ws_bootstrap_reads() {
        if catalyrst_testgate::env_value(LIVE_TEST_VAR).is_none() {
            return;
        }
        let cfg = super::super::UpstreamConfig::from_env()
            .expect("UPSTREAM_SOCIAL_LIVE_TEST is set but the UPSTREAM_SOCIAL_* config is not");
        let url = cfg.social_rpc_url.clone().expect("UPSTREAM_SOCIAL_RPC_URL");
        let identity = cfg
            .identity()
            .expect("identity from UPSTREAM_SOCIAL_KEY_FILE");
        let session = UpstreamSession::connect(&url, &identity)
            .await
            .expect("upstream ws session");
        session
            .get_friends(Some(Pagination {
                limit: 10,
                offset: 0,
            }))
            .await
            .expect("GetFriends");
        session
            .get_social_settings()
            .await
            .expect("GetSocialSettings");
    }
}
