use super::wire::Wire;
use catalyrst_crypto::Wallet;
use catalyrst_pulse::{
    decentraland::{common, pulse as p},
    handshake::build_signed_fetch_payload,
    v4,
};
use prost::Message;

pub fn packet(message: p::client_message::Message) -> Vec<u8> {
    p::ClientMessage {
        message: Some(message),
    }
    .encode_to_vec()
}

pub struct Identity {
    pub address: String,
    ephemeral: Wallet,
    delegation: String,
    signature: String,
}
impl Identity {
    pub fn new(index: usize) -> Self {
        let root = Wallet::from_hex(&format!("{:064x}", index + 1)).unwrap();
        let ephemeral = Wallet::from_hex(&format!("{:064x}", index + 1001)).unwrap();
        let delegation = format!(
            "Decentraland Login\nEphemeral address: {}\nExpiration: 2099-01-01T00:00:00.000Z",
            ephemeral.address()
        );
        let signature = root.sign_message(delegation.as_bytes()).unwrap();
        Self {
            address: root.address(),
            ephemeral,
            delegation,
            signature,
        }
    }
    fn links(&self, payload: &str) -> common::AuthChain {
        common::AuthChain {
            links: vec![
                common::AuthLink {
                    r#type: common::AuthLinkType::Signer as i32,
                    payload: self.address.clone(),
                    signature: None,
                },
                common::AuthLink {
                    r#type: common::AuthLinkType::EcdsaEphemeral as i32,
                    payload: self.delegation.clone(),
                    signature: Some(self.signature.clone()),
                },
                common::AuthLink {
                    r#type: common::AuthLinkType::EcdsaSignedEntity as i32,
                    payload: payload.into(),
                    signature: Some(self.ephemeral.sign_message(payload.as_bytes()).unwrap()),
                },
            ],
        }
    }
    pub async fn authenticate(&self, wire: &mut Wire, mode: &str, state: p::PlayerState) {
        let initial = Some(p::PlayerInitialState {
            state: Some(state),
            realm: "walking-benchmark".into(),
            ..Default::default()
        });
        if mode == "v4" {
            let hello = p::PulseV4Hello {
                request_id: vec![1; 16],
                session_id: vec![2; 16],
                connection_epoch: 1,
                optional_capabilities: vec![
                    v4::CAPABILITY_DELTA_BATCH_BASELINE.into(),
                    v4::CAPABILITY_DELTA_BATCH_DICTIONARY.into(),
                ],
                role: p::PulseV4Role::Player as i32,
                initial_state: initial,
                ..Default::default()
            };
            wire.send(
                true,
                packet(p::client_message::Message::V4Hello(hello.clone())),
            );
            let (_, bytes) = wire.receive().await;
            let Some(p::server_message::Message::V4Challenge(challenge)) =
                p::ServerMessage::decode(bytes.as_slice()).unwrap().message
            else {
                panic!("missing challenge");
            };
            assert_eq!(challenge.audience, "walking-benchmark");
            assert_eq!(
                challenge.negotiated_capabilities,
                hello.optional_capabilities
            );
            let mut auth = p::PulseV4Auth {
                request_id: hello.request_id.clone(),
                session_id: hello.session_id.clone(),
                connection_epoch: 1,
                challenge_id: challenge.challenge_id.clone(),
                idempotency_key: vec![3; 16],
                wallet: self.address.clone(),
                auth_session: self.ephemeral.address(),
                auth_chain: None,
            };
            let payload = v4::signing_payload(&v4::transcript(&hello, &challenge, &auth));
            auth.auth_chain = Some(self.links(&payload));
            wire.send(true, packet(p::client_message::Message::V4Auth(auth)));
            let (_, bytes) = wire.receive().await;
            let Some(p::server_message::Message::V4Result(result)) =
                p::ServerMessage::decode(bytes.as_slice()).unwrap().message
            else {
                panic!("missing result");
            };
            assert!(result.success, "v4 rejected: {:?}", result.error_code);
            assert_eq!(
                result.negotiated_capabilities,
                challenge.negotiated_capabilities
            );
        } else {
            let timestamp = chrono::Utc::now().timestamp_millis().to_string();
            let metadata = "{\"signer\":\"dcl:explorer\"}";
            let payload = build_signed_fetch_payload("connect", "/", &timestamp, metadata);
            let mut bag = serde_json::Map::new();
            for (index, link) in self.links(&payload).links.into_iter().enumerate() {
                let kind = ["SIGNER", "ECDSA_EPHEMERAL", "ECDSA_SIGNED_ENTITY"][index];
                bag.insert(format!("x-identity-auth-chain-{index}"), serde_json::Value::String(serde_json::json!({ "type":kind, "payload":link.payload, "signature":link.signature.unwrap_or_default() }).to_string()));
            }
            bag.insert("x-identity-timestamp".into(), timestamp.into());
            bag.insert("x-identity-metadata".into(), metadata.into());
            wire.send(
                true,
                packet(p::client_message::Message::Handshake(p::HandshakeRequest {
                    auth_chain: serde_json::to_vec(&bag).unwrap(),
                    profile_version: 0,
                    initial_state: initial,
                    protocol_features: if mode == "negotiated" { 6 } else { 0 },
                })),
            );
            let (_, bytes) = wire.receive().await;
            let Some(p::server_message::Message::Handshake(result)) =
                p::ServerMessage::decode(bytes.as_slice()).unwrap().message
            else {
                panic!("missing legacy result");
            };
            assert!(result.success, "legacy rejected: {:?}", result.error);
            if mode == "negotiated" {
                assert_ne!(result.protocol_features & 2, 0);
            }
        }
    }
}
