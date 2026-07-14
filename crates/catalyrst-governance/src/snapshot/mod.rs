pub mod client;
pub mod eip712;
pub mod templates;

use std::time::Duration;

use serde_json::{json, Value};

use crate::config::SnapshotConfig;
use client::{SnapshotClient, SnapshotError};
use eip712::{ProposalMessage, APP_NAME, VOTING_TYPE};
use templates::{ProposalKind, RenderContext, DEFAULT_CHOICES};

const RPC_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug)]
pub enum SubmitError {
    BadRequest(String),
    Upstream(SnapshotError),
}

#[derive(Debug, Clone)]
pub struct Submission {
    pub id: String,
    pub ipfs: String,
    pub space: String,
    pub title: String,
    pub pending: bool,
}

pub struct SnapshotSubmitter {
    client: SnapshotClient,
    space: String,
    space_council: Option<String>,
    governance_url: String,
    snapshot_web_url: String,
    block_rpc_url: String,
    cfg: SnapshotConfig,
    http: reqwest::Client,
}

pub enum SnapshotGate {
    Ready(Box<SnapshotSubmitter>),
    Unconfigured(String),
}

impl SnapshotGate {
    pub fn build(cfg: SnapshotConfig) -> Self {
        let mut missing: Vec<&'static str> = Vec::new();
        if blank(&cfg.private_key) {
            missing.push("SNAPSHOT_PRIVATE_KEY");
        }
        if blank(&cfg.space) {
            missing.push("SNAPSHOT_SPACE");
        }
        if blank(&cfg.api_url) {
            missing.push("SNAPSHOT_API");
        }
        if blank(&cfg.block_rpc_url) {
            missing.push("SNAPSHOT_BLOCK_RPC_URL");
        }

        if !missing.is_empty() {
            return Self::Unconfigured(unconfigured_message(&missing));
        }

        let client = match SnapshotClient::new(
            cfg.api_url.as_deref().unwrap_or_default(),
            cfg.private_key.as_deref().unwrap_or_default(),
            cfg.address.as_deref(),
        ) {
            Ok(client) => client,
            Err(detail) => {
                return Self::Unconfigured(format!(
                    "governance proposal submission is not configured: {detail}"
                ))
            }
        };

        let http = match reqwest::Client::builder().timeout(RPC_TIMEOUT).build() {
            Ok(http) => http,
            Err(e) => {
                return Self::Unconfigured(format!(
                    "governance proposal submission is not configured: could not build the rpc client: {e}"
                ))
            }
        };

        Self::Ready(Box::new(SnapshotSubmitter {
            space: cfg.space.clone().unwrap_or_default(),
            space_council: cfg.space_council.clone(),
            governance_url: cfg.governance_url.clone(),
            snapshot_web_url: cfg.snapshot_web_url.clone(),
            block_rpc_url: cfg.block_rpc_url.clone().unwrap_or_default(),
            client,
            cfg,
            http,
        }))
    }

    pub fn startup_log(&self) {
        match self {
            Self::Ready(submitter) => tracing::info!(
                space = %submitter.space,
                sequencer = %submitter.client.endpoint(),
                poster = %submitter.client.from_address().to_checksum(None),
                "snapshot proposal submission enabled"
            ),
            Self::Unconfigured(message) => tracing::warn!("{message}"),
        }
    }
}

fn blank(value: &Option<String>) -> bool {
    value.as_deref().map(str::trim).unwrap_or("").is_empty()
}

fn unconfigured_message(missing: &[&'static str]) -> String {
    format!(
        "{} unset; governance proposal submission is not configured and POST /proposals/{{catalyst,hiring,tender,linked-wearables,governance,council-decision-veto}} rejects every submission with 503 (no snapshot proposal is ever signed or sent)",
        missing.join(", ")
    )
}

impl SnapshotSubmitter {
    pub async fn submit(
        &self,
        kind: ProposalKind,
        payload: &Value,
        author: &str,
    ) -> Result<Submission, SubmitError> {
        let ctx = RenderContext {
            author,
            governance_url: &self.governance_url,
            snapshot_web_url: &self.snapshot_web_url,
            space_council: self.space_council.as_deref(),
        };
        let rendered = templates::render(kind, payload, &ctx).map_err(SubmitError::BadRequest)?;

        let block = self.latest_block().await.map_err(SubmitError::Upstream)?;
        let now = floor_to_minute(chrono::Utc::now().timestamp().max(0) as u64);
        let (start, end) = self.lifespan(kind, now);

        let message = ProposalMessage {
            from: self.client.from_address(),
            space: self.space.clone(),
            timestamp: now,
            voting_type: VOTING_TYPE.to_string(),
            title: rendered.title.clone(),
            body: rendered.body,
            discussion: String::new(),
            choices: DEFAULT_CHOICES.iter().map(|c| c.to_string()).collect(),
            start,
            end,
            snapshot: block,
            plugins: "{}".to_string(),
            app: APP_NAME.to_string(),
        };

        let receipt = self
            .client
            .submit_proposal(&message)
            .await
            .map_err(SubmitError::Upstream)?;

        tracing::info!(
            snapshot_id = %receipt.id,
            space = %self.space,
            kind = kind.as_path(),
            %author,
            "snapshot proposal created"
        );

        Ok(Submission {
            id: receipt.id,
            ipfs: receipt.ipfs,
            space: self.space.clone(),
            title: rendered.title,
            pending: kind.is_pending_on_creation(),
        })
    }

    fn lifespan(&self, kind: ProposalKind, now: u64) -> (u64, u64) {
        match kind {
            ProposalKind::Tender => {
                let start = now + self.cfg.tender_submission_window_seconds;
                (start, start + self.cfg.duration_for(kind))
            }
            _ => (now, now + self.cfg.duration_for(kind)),
        }
    }

    async fn latest_block(&self) -> Result<u64, SnapshotError> {
        let response = self
            .http
            .post(&self.block_rpc_url)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "eth_blockNumber",
                "params": [],
            }))
            .send()
            .await
            .map_err(|e| SnapshotError::Transport(format!("eth_blockNumber failed: {e}")))?;

        let status = response.status();
        let body: Value = response
            .json()
            .await
            .map_err(|e| SnapshotError::Malformed(format!("eth_blockNumber response: {e}")))?;

        if !status.is_success() {
            return Err(SnapshotError::Transport(format!(
                "eth_blockNumber returned {}",
                status.as_u16()
            )));
        }

        let raw = body
            .get("result")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SnapshotError::Malformed("eth_blockNumber returned no result".into()))?;

        u64::from_str_radix(raw.trim_start_matches("0x"), 16).map_err(|e| {
            SnapshotError::Malformed(format!("eth_blockNumber returned {raw}, unparsable: {e}"))
        })
    }
}

fn floor_to_minute(seconds: u64) -> u64 {
    seconds - (seconds % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_missing_var_is_named_in_the_startup_warning() {
        let gate = SnapshotGate::build(SnapshotConfig::default());
        let SnapshotGate::Unconfigured(message) = gate else {
            panic!("an empty config must not produce a ready submitter");
        };
        for var in [
            "SNAPSHOT_PRIVATE_KEY",
            "SNAPSHOT_SPACE",
            "SNAPSHOT_API",
            "SNAPSHOT_BLOCK_RPC_URL",
        ] {
            assert!(message.contains(var), "{var} missing from: {message}");
        }
        assert!(message.contains("not configured"), "got: {message}");
    }

    #[test]
    fn a_key_without_a_space_still_fails_closed() {
        let cfg = SnapshotConfig {
            private_key: Some(
                "0x0101010101010101010101010101010101010101010101010101010101010101".to_string(),
            ),
            api_url: Some("http://127.0.0.1:1/msg".to_string()),
            block_rpc_url: Some("http://127.0.0.1:1/rpc".to_string()),
            ..SnapshotConfig::default()
        };
        let SnapshotGate::Unconfigured(message) = SnapshotGate::build(cfg) else {
            panic!("a missing space must not produce a ready submitter");
        };
        assert!(message.contains("SNAPSHOT_SPACE"), "got: {message}");
        assert!(!message.contains("SNAPSHOT_PRIVATE_KEY"), "got: {message}");
    }

    #[test]
    fn an_unparsable_private_key_fails_closed_rather_than_panicking() {
        let cfg = SnapshotConfig {
            private_key: Some("not-a-key".to_string()),
            space: Some("snapshot.example.eth".to_string()),
            api_url: Some("http://127.0.0.1:1/msg".to_string()),
            block_rpc_url: Some("http://127.0.0.1:1/rpc".to_string()),
            ..SnapshotConfig::default()
        };
        let SnapshotGate::Unconfigured(message) = SnapshotGate::build(cfg) else {
            panic!("an invalid key must not produce a ready submitter");
        };
        assert!(message.contains("SNAPSHOT_PRIVATE_KEY"), "got: {message}");
    }

    #[test]
    fn timestamps_are_floored_to_the_minute() {
        assert_eq!(floor_to_minute(1_700_000_059), 1_700_000_040);
        assert_eq!(floor_to_minute(1_700_000_040), 1_700_000_040);
    }
}
