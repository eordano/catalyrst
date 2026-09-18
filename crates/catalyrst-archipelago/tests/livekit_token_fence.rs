use std::sync::Arc;

use alloy::signers::{local::PrivateKeySigner, SignerSync};
use catalyrst_archipelago::config::{Config, LivekitConfig, ServerConfig};
use catalyrst_archipelago::control_v4::{AssignmentAuthority, LaneKey, V4Owner};
use catalyrst_archipelago::nats::RecordingPublisher;
use catalyrst_archipelago::{api_router, build_state_with, AppState};
use serde_json::{json, Value};

#[path = "support/control_v4.rs"]
mod support;
use support::Fixture;

const ROOM: &str = "I7";

struct Server {
    base: String,
    state: AppState,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Server {
    async fn start(fixture: &Fixture) -> Self {
        let config = Config {
            http_host: "127.0.0.1".into(),
            http_port: 0,
            cluster: Default::default(),
            server: ServerConfig {
                control_v4_audience: "test-realm".into(),
                ..Default::default()
            },
            auth: Default::default(),
            livekit: LivekitConfig {
                api_key: Some("APIkey".into()),
                api_secret: Some("secret".into()),
                ..Default::default()
            },
            nats: Default::default(),
            content_database_url: None,
            control_database_url: None,
            content_base_url: String::new(),
            commit_hash: String::new(),
        };
        let mut state = build_state_with(&config, RecordingPublisher::new())
            .await
            .unwrap();
        Arc::get_mut(&mut state).unwrap().control_v4 =
            AssignmentAuthority::pg_with_namespace(fixture.pool.clone(), "test-realm")
                .await
                .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = api_router().with_state(state.clone());
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            base: format!("http://{address}"),
            state,
            task,
        }
    }

    async fn mint(&self, wallet: &PrivateKeySigner, device: Option<&PrivateKeySigner>) -> u16 {
        let address = format!("{:#x}", wallet.address());
        let client = reqwest::Client::new();
        let issued: Value = client
            .post(format!("{}/auth/challenge", self.base))
            .json(&json!({ "address": address }))
            .send()
            .await
            .expect("challenge request")
            .json()
            .await
            .expect("challenge json");
        let challenge = issued["challenge"].as_str().expect("challenge").to_string();
        let digest = alloy::primitives::eip191_hash_message(challenge.as_bytes());
        let chain = match device {
            None => json!([
                { "type": "SIGNER", "payload": address, "signature": "" },
                {
                    "type": "ECDSA_SIGNED_ENTITY",
                    "payload": challenge,
                    "signature": wallet.sign_hash_sync(&digest).expect("sign").to_string()
                }
            ]),
            Some(device) => {
                let mandate = format!(
                    "Decentraland Login\nEphemeral address: {}\nExpiration: 2099-12-31T00:00:00.000Z",
                    session_of(device)
                );
                let mandate_signature = wallet
                    .sign_hash_sync(&alloy::primitives::eip191_hash_message(mandate.as_bytes()))
                    .expect("sign mandate");
                json!([
                    { "type": "SIGNER", "payload": address, "signature": "" },
                    {
                        "type": "ECDSA_EPHEMERAL",
                        "payload": mandate,
                        "signature": mandate_signature.to_string()
                    },
                    {
                        "type": "ECDSA_SIGNED_ENTITY",
                        "payload": challenge,
                        "signature": device.sign_hash_sync(&digest).expect("sign").to_string()
                    }
                ])
            }
        };
        client
            .post(format!("{}/auth/livekit-token", self.base))
            .json(&json!({
                "address": address,
                "challenge": challenge,
                "room": "",
                "auth_chain": chain
            }))
            .send()
            .await
            .expect("token request")
            .status()
            .as_u16()
    }
}

fn session_of(device: &PrivateKeySigner) -> String {
    format!("{:#x}", device.address())
}

#[tokio::test]
async fn only_the_session_that_holds_a_wallets_realm_lane_is_minted_a_token() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let server = Server::start(&fixture).await;
    let wallet = PrivateKeySigner::random();
    let address = format!("{:#x}", wallet.address());
    let holder = PrivateKeySigner::random();
    let displaced = PrivateKeySigner::random();
    server.state.peers.set_island(&address, ROOM);

    assert_eq!(
        server.mint(&wallet, Some(&displaced)).await,
        200,
        "a wallet no socket claimed keeps the legacy mint"
    );

    let owner = V4Owner {
        address: address.clone(),
        session: session_of(&holder),
        epoch: 1,
    };
    server
        .state
        .control_v4
        .claim_lanes(&owner, &[LaneKey::parse("realm").unwrap()])
        .await
        .unwrap();

    assert_eq!(server.mint(&wallet, Some(&holder)).await, 200);
    assert_eq!(server.mint(&wallet, Some(&displaced)).await, 403);
    assert_eq!(server.mint(&wallet, None).await, 403);

    server.state.control_v4.release(&owner).await.unwrap();
    assert_eq!(
        server.mint(&wallet, Some(&displaced)).await,
        200,
        "a released lane no longer fences the wallet"
    );

    drop(server);
    fixture.finish().await;
}

#[tokio::test]
async fn an_unreachable_authority_mints_nothing() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let server = Server::start(&fixture).await;
    let wallet = PrivateKeySigner::random();
    server
        .state
        .peers
        .set_island(&format!("{:#x}", wallet.address()), ROOM);
    fixture.pool.close().await;

    assert_eq!(server.mint(&wallet, None).await, 503);

    drop(server);
    fixture.finish().await;
}
