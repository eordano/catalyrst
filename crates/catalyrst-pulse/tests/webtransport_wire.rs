use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use prost::Message as _;
use sha2::{Digest, Sha256};

use catalyrst_pulse::decentraland::pulse::{
    client_message, server_message, ClientMessage, HandshakeRequest, PlayerState, PlayerStateInput,
    ServerMessage,
};
use catalyrst_pulse::server::ENET_CAPACITY;
use catalyrst_pulse::transport::webtransport::framing::StreamFrameReader;
use catalyrst_pulse::transport::webtransport::framing::{datagram_frame, stream_frame};
use catalyrst_pulse::transport::webtransport::{WtConfig, WtHost};
use catalyrst_pulse::transport::{Host, HostConfig, Transports};
use catalyrst_pulse::PulseServer;

use web_transport::client::{Client, ClientConfig, ClientEvent};

const ROOT_KEY: &str = "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
const EPH_KEY: &str = "59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d";

fn dev_cert() -> (String, String, Vec<u8>) {
    let certified = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    let hash = Sha256::digest(certified.cert.der().as_ref()).to_vec();
    (
        certified.cert.pem(),
        certified.key_pair.serialize_pem(),
        hash,
    )
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

async fn signed_handshake(ts_ms: i64) -> (Vec<u8>, String) {
    use alloy::signers::{local::PrivateKeySigner, Signer};
    use catalyrst_pulse::handshake::build_signed_fetch_payload;
    use catalyrst_types::{AuthLink, AuthLinkType};

    let root: PrivateKeySigner = ROOT_KEY.parse().unwrap();
    let root_addr = format!("{:#x}", root.address());
    let ephemeral: PrivateKeySigner = EPH_KEY.parse().unwrap();
    let eph_addr = format!("{:#x}", ephemeral.address());

    let ts = ts_ms.to_string();
    let metadata = "{\"signer\":\"dcl:explorer\"}";
    let connect_payload = build_signed_fetch_payload("connect", "/", &ts, metadata);
    let eph_payload = format!(
        "Decentraland Login\nEphemeral address: {eph_addr}\nExpiration: 2099-01-01T00:00:00.000Z"
    );
    let eph_sig = root
        .sign_message(eph_payload.as_bytes())
        .await
        .unwrap()
        .to_string();
    let final_sig = ephemeral
        .sign_message(connect_payload.as_bytes())
        .await
        .unwrap()
        .to_string();

    let chain = [
        AuthLink {
            link_type: AuthLinkType::SIGNER,
            payload: root_addr.clone(),
            signature: None,
        },
        AuthLink {
            link_type: AuthLinkType::EcdsaEphemeral,
            payload: eph_payload,
            signature: Some(eph_sig),
        },
        AuthLink {
            link_type: AuthLinkType::EcdsaSignedEntity,
            payload: connect_payload,
            signature: Some(final_sig),
        },
    ];
    let mut map = serde_json::Map::new();
    for (i, link) in chain.iter().enumerate() {
        map.insert(
            format!("x-identity-auth-chain-{i}"),
            serde_json::Value::String(serde_json::to_string(link).unwrap()),
        );
    }
    map.insert("x-identity-timestamp".into(), serde_json::Value::String(ts));
    map.insert(
        "x-identity-metadata".into(),
        serde_json::Value::String(metadata.into()),
    );
    let bag = serde_json::to_string(&serde_json::Value::Object(map)).unwrap();

    let msg = ClientMessage {
        message: Some(client_message::Message::Handshake(HandshakeRequest {
            auth_chain: bag.into_bytes(),
            profile_version: 0,
            initial_state: None,
            protocol_features: 0,
        })),
    };
    (msg.encode_to_vec(), root_addr.to_lowercase())
}

#[test]
fn full_handshake_and_gameplay_over_the_wire() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_test_writer()
        .try_init();
    let (cert_pem, key_pem, cert_hash) = dev_cert();

    let rt = tokio::runtime::Runtime::new().unwrap();

    let (wt, events) = WtHost::start(WtConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        cert_pem,
        key_pem,
        slot_base: ENET_CAPACITY as u32,
        slot_capacity: 64,
        max_datagram_bytes: 1200,
        max_message_bytes: 4096,
        service_timeout_ms: 1,
        server_full_reason: 6,
    })
    .unwrap();
    let wt_addr = wt.local_addr();

    rt.block_on(async {
        let enet = Host::bind(HostConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            max_peers: ENET_CAPACITY,
            channel_limit: 8,
        })
        .await
        .unwrap();
        let transports = Transports::with_webtransport(enet, ENET_CAPACITY as u32, wt, events);
        tokio::spawn(PulseServer::new().serve(transports, 50));
    });

    let url = format!("https://127.0.0.1:{}/", wt_addr.port());
    let mut client = Client::connect(ClientConfig {
        url,
        server_cert_hash: Some(cert_hash),
    })
    .expect("client connects to the real server over QUIC");

    let (handshake, wallet) = rt.block_on(signed_handshake(now_ms()));
    assert!(client.send_stream(&stream_frame(&handshake)));

    let mut reader = StreamFrameReader::new(4096);
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut authed = false;
    while Instant::now() < deadline && !authed {
        match client.service(Duration::from_millis(200)) {
            Some(ClientEvent::StreamData { data }) => {
                reader.append(&data);
                while let Ok(Some(frame)) = reader.try_read() {
                    let msg = ServerMessage::decode(&frame[..]).expect("valid ServerMessage");
                    if let Some(server_message::Message::Handshake(h)) = msg.message {
                        assert!(h.success, "handshake rejected on the wire: {:?}", h.error);
                        authed = true;
                    }
                }
            }
            Some(ClientEvent::Disconnected { reason }) => {
                panic!("server closed the session before authenticating (reason {reason})");
            }
            _ => {}
        }
    }
    assert!(
        authed,
        "no successful HandshakeResponse received over the wire (wallet {wallet})"
    );

    let mut state = PlayerState {
        parcel_index: 5,
        ..Default::default()
    };
    state.set_position_x_f(8.0);
    state.set_position_z_f(8.0);
    let input = ClientMessage {
        message: Some(client_message::Message::Input(PlayerStateInput {
            state: Some(state),
        })),
    }
    .encode_to_vec();
    assert!(client.send_datagram(&datagram_frame(1, 0, &input)));

    let watch = Instant::now() + Duration::from_millis(600);
    while Instant::now() < watch {
        if let Some(ClientEvent::Disconnected { reason }) =
            client.service(Duration::from_millis(100))
        {
            panic!("session dropped after a valid movement update (reason {reason})");
        }
    }

    client.disconnect(0);
    drop(rt);
}
