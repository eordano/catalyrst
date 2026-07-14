use std::str::FromStr;
use std::time::Duration;

use alloy::primitives::Address;
use alloy::signers::local::PrivateKeySigner;
use alloy::signers::SignerSync;
use serde::Deserialize;
use serde_json::{json, Value};

use super::eip712::{domain_json, types_json, ProposalMessage};

const HUB_TO_SEQUENCER: [(&str, &str); 3] = [
    ("https://hub.snapshot.org", "https://seq.snapshot.org"),
    (
        "https://testnet.hub.snapshot.org",
        "https://testnet.seq.snapshot.org",
    ),
    ("http://localhost:3000", "http://localhost:3001"),
];

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug)]
pub enum SnapshotError {
    Signing(String),
    Transport(String),
    Rejected { status: u16, message: String },
    Malformed(String),
}

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Signing(d) => write!(f, "could not sign the snapshot proposal: {d}"),
            Self::Transport(d) => write!(f, "snapshot sequencer unreachable: {d}"),
            Self::Rejected { status, message } => {
                write!(
                    f,
                    "snapshot sequencer rejected the proposal ({status}): {message}"
                )
            }
            Self::Malformed(d) => {
                write!(f, "snapshot sequencer returned an unusable response: {d}")
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SnapshotRelayer {
    pub address: String,
    pub receipt: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SnapshotReceipt {
    pub id: String,
    pub ipfs: String,
    pub relayer: Option<SnapshotRelayer>,
}

pub fn sequencer_url(api_url: &str) -> String {
    let mut url = api_url.trim().trim_end_matches('/').to_string();
    for (hub, sequencer) in HUB_TO_SEQUENCER {
        url = url.replace(hub, sequencer);
    }
    url
}

pub struct SnapshotClient {
    endpoint: String,
    signer: PrivateKeySigner,
    from: Address,
    http: reqwest::Client,
}

impl SnapshotClient {
    pub fn new(api_url: &str, private_key: &str, address: Option<&str>) -> Result<Self, String> {
        let signer = PrivateKeySigner::from_str(private_key.trim().trim_start_matches("0x"))
            .map_err(|e| format!("invalid SNAPSHOT_PRIVATE_KEY: {e}"))?;

        let from = match address.map(str::trim).filter(|s| !s.is_empty()) {
            Some(raw) => {
                Address::from_str(raw).map_err(|e| format!("invalid SNAPSHOT_ADDRESS: {e}"))?
            }
            None => signer.address(),
        };

        let http = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| format!("could not build the snapshot http client: {e}"))?;

        Ok(Self {
            endpoint: sequencer_url(api_url),
            signer,
            from,
            http,
        })
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn from_address(&self) -> Address {
        self.from
    }

    pub fn envelope(&self, message: &ProposalMessage) -> Result<Value, SnapshotError> {
        let signature = self
            .signer
            .sign_hash_sync(&message.digest())
            .map_err(|e| SnapshotError::Signing(e.to_string()))?;

        Ok(json!({
            "address": self.from.to_checksum(None),
            "sig": format!("0x{}", alloy::hex::encode(signature.as_bytes())),
            "data": {
                "domain": domain_json(),
                "types": types_json(),
                "message": message.message_json(),
            },
        }))
    }

    pub async fn submit_proposal(
        &self,
        message: &ProposalMessage,
    ) -> Result<SnapshotReceipt, SnapshotError> {
        let envelope = self.envelope(message)?;

        let response = self
            .http
            .post(&self.endpoint)
            .header(reqwest::header::ACCEPT, "application/json")
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .json(&envelope)
            .send()
            .await
            .map_err(|e| SnapshotError::Transport(e.to_string()))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| SnapshotError::Transport(e.to_string()))?;

        if !status.is_success() {
            return Err(SnapshotError::Rejected {
                status: status.as_u16(),
                message: sequencer_message(&body),
            });
        }

        let receipt: SnapshotReceipt = serde_json::from_str(&body)
            .map_err(|e| SnapshotError::Malformed(format!("{e}: {}", truncate(&body))))?;

        if receipt.id.trim().is_empty() {
            return Err(SnapshotError::Malformed(
                "sequencer accepted the proposal but returned no id".to_string(),
            ));
        }

        Ok(receipt)
    }
}

fn sequencer_message(body: &str) -> String {
    let parsed: Result<Value, _> = serde_json::from_str(body);
    if let Ok(value) = parsed {
        for key in ["error_description", "error", "message"] {
            if let Some(text) = value.get(key).and_then(|v| v.as_str()) {
                if !text.trim().is_empty() {
                    return text.to_string();
                }
            }
        }
    }
    truncate(body)
}

fn truncate(body: &str) -> String {
    body.chars().take(300).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hub_urls_are_rewritten_to_their_sequencer() {
        assert_eq!(
            sequencer_url("https://hub.snapshot.org"),
            "https://seq.snapshot.org"
        );
        assert_eq!(
            sequencer_url("https://testnet.hub.snapshot.org/"),
            "https://testnet.seq.snapshot.org"
        );
        assert_eq!(
            sequencer_url("http://localhost:3000"),
            "http://localhost:3001"
        );
    }

    #[test]
    fn a_non_hub_url_is_left_alone() {
        assert_eq!(
            sequencer_url("http://127.0.0.1:5199/api/msg/"),
            "http://127.0.0.1:5199/api/msg"
        );
    }

    #[test]
    fn the_signature_matches_what_ethers_produces_for_the_same_message() {
        use super::super::eip712::{ProposalMessage, APP_NAME, VOTING_TYPE};

        let client = SnapshotClient::new(
            "http://127.0.0.1:1/msg",
            "0x0101010101010101010101010101010101010101010101010101010101010101",
            None,
        )
        .expect("client");

        let message = ProposalMessage {
            from: client.from_address(),
            space: "gate.dcl.eth".to_string(),
            timestamp: 1_700_000_040,
            voting_type: VOTING_TYPE.to_string(),
            title: "Add catalyst node with domain peer.example.org to the catalyst network"
                .to_string(),
            body: "> by 0x1111111111111111111111111111111111111111\n\nShould the catalyst node with the domain peer.example.org and owner 0x3333333333333333333333333333333333333333 be added to Decentraland's Catalyst Network?\n\n## Description\n\nA new node for the network."
                .to_string(),
            discussion: String::new(),
            choices: vec!["yes".to_string(), "no".to_string(), "abstain".to_string()],
            start: 1_700_000_040,
            end: 1_700_000_640,
            snapshot: 22_000_000,
            plugins: "{}".to_string(),
            app: APP_NAME.to_string(),
        };

        let envelope = client.envelope(&message).expect("envelope");
        assert_eq!(
            envelope["address"],
            "0x1a642f0E3c3aF545E7AcBD38b07251B3990914F1"
        );
        assert_eq!(
            envelope["sig"],
            "0xb4d0c1b48df1aa1b45c1e64677e93d9e82d2852b7d67c936f6085bf30baa67db30067c5ddd985c3e7832be133933f7e6f4ec0fc0415d08a8f7c3439d3768ed7c1c"
        );
    }

    #[test]
    fn a_rejection_body_surfaces_the_sequencer_reason() {
        assert_eq!(
            sequencer_message(r#"{"error":"unauthorized","error_description":"not authorized"}"#),
            "not authorized"
        );
        assert_eq!(
            sequencer_message("plain text failure"),
            "plain text failure"
        );
    }
}
