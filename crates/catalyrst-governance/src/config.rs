use anyhow::{Context, Result};
use catalyrst_envcfg::{get_port, required, required_endpoint};
use std::env;

use crate::snapshot::templates::ProposalKind;

pub struct Config {
    pub http_host: String,
    pub http_port: u16,

    pub database_url: String,

    pub api_url: String,

    pub poll_enabled: bool,

    pub sync_window_hours: u32,

    pub snapshot: SnapshotConfig,
}

pub const DEFAULT_SYNC_WINDOW_HOURS: u32 = 48;
pub const DEFAULT_GOVERNANCE_URL: &str = "https://decentraland.org/governance";
pub const DEFAULT_SNAPSHOT_WEB_URL: &str = "https://snapshot.org";
pub const DEFAULT_PROPOSAL_DURATION_SECONDS: u64 = 7 * 24 * 3600;

#[derive(Debug, Clone)]
pub struct SnapshotConfig {
    pub private_key: Option<String>,
    pub address: Option<String>,
    pub space: Option<String>,
    pub space_council: Option<String>,
    pub api_url: Option<String>,
    pub block_rpc_url: Option<String>,

    pub governance_url: String,
    pub snapshot_web_url: String,

    pub duration_seconds: u64,
    pub duration_governance_seconds: Option<u64>,
    pub duration_hiring_seconds: Option<u64>,
    pub duration_tender_seconds: Option<u64>,
    pub duration_council_veto_seconds: Option<u64>,
    pub tender_submission_window_seconds: u64,
}

impl Default for SnapshotConfig {
    fn default() -> Self {
        Self {
            private_key: None,
            address: None,
            space: None,
            space_council: None,
            api_url: None,
            block_rpc_url: None,
            governance_url: DEFAULT_GOVERNANCE_URL.to_string(),
            snapshot_web_url: DEFAULT_SNAPSHOT_WEB_URL.to_string(),
            duration_seconds: DEFAULT_PROPOSAL_DURATION_SECONDS,
            duration_governance_seconds: None,
            duration_hiring_seconds: None,
            duration_tender_seconds: None,
            duration_council_veto_seconds: None,
            tender_submission_window_seconds: 0,
        }
    }
}

impl SnapshotConfig {
    pub fn from_env() -> Result<Self> {
        let base = Self::default();
        Ok(Self {
            private_key: opt_env("SNAPSHOT_PRIVATE_KEY"),
            address: opt_env("SNAPSHOT_ADDRESS"),
            space: opt_env("SNAPSHOT_SPACE").or_else(|| opt_env("GATSBY_SNAPSHOT_SPACE")),
            space_council: opt_env("SNAPSHOT_SPACE_COUNCIL"),
            api_url: opt_env("SNAPSHOT_API").or_else(|| opt_env("GATSBY_SNAPSHOT_API")),
            block_rpc_url: opt_env("SNAPSHOT_BLOCK_RPC_URL"),
            governance_url: opt_env("GOVERNANCE_PUBLIC_URL").unwrap_or(base.governance_url),
            snapshot_web_url: opt_env("SNAPSHOT_WEB_URL").unwrap_or(base.snapshot_web_url),
            duration_seconds: get_u64("GATSBY_SNAPSHOT_DURATION", base.duration_seconds)?,
            duration_governance_seconds: opt_u64("GATSBY_DURATION_GOVERNANCE")?,
            duration_hiring_seconds: opt_u64("GATSBY_DURATION_HIRING")?,
            duration_tender_seconds: opt_u64("DURATION_TENDER")?,
            duration_council_veto_seconds: opt_u64("DURATION_COUNCIL_DECISION_VETO")?,
            tender_submission_window_seconds: get_u64("SUBMISSION_WINDOW_DURATION_TENDER", 0)?,
        })
    }

    pub fn duration_for(&self, kind: ProposalKind) -> u64 {
        let override_seconds = match kind {
            ProposalKind::Governance => self.duration_governance_seconds,
            ProposalKind::Hiring => self.duration_hiring_seconds,
            ProposalKind::Tender => self.duration_tender_seconds,
            ProposalKind::CouncilDecisionVeto => self.duration_council_veto_seconds,
            _ => None,
        };
        override_seconds
            .filter(|s| *s > 0)
            .unwrap_or(self.duration_seconds)
    }
}

fn opt_env(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn opt_u64(key: &str) -> Result<Option<u64>> {
    match opt_env(key) {
        Some(raw) => raw
            .parse::<u64>()
            .map(Some)
            .with_context(|| format!("invalid {key}")),
        None => Ok(None),
    }
}

fn get_u64(key: &str, default: u64) -> Result<u64> {
    Ok(opt_u64(key)?.unwrap_or(default))
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port: get_port("HTTP_SERVER_PORT", 5151)?,
            database_url: required("GOVERNANCE_PG_COMPONENT_PSQL_CONNECTION_STRING")?,
            api_url: required_endpoint("GOVERNANCE_API_URL")?
                .trim_end_matches('/')
                .to_string(),
            poll_enabled: parse_bool_env("GOVERNANCE_POLL_ENABLED", false),
            sync_window_hours: get_u32("GOVERNANCE_SYNC_WINDOW_HOURS", DEFAULT_SYNC_WINDOW_HOURS)?,
            snapshot: SnapshotConfig::from_env()?,
        })
    }
}

fn get_u32(key: &str, default: u32) -> Result<u32> {
    match env::var(key) {
        Ok(s) if !s.is_empty() => s.parse::<u32>().with_context(|| format!("invalid {}", key)),
        _ => Ok(default),
    }
}

pub fn parse_bool_env(key: &str, default: bool) -> bool {
    match env::var(key) {
        Ok(s) => matches!(
            s.trim().to_ascii_lowercase().as_str(),
            "true" | "1" | "yes" | "on"
        ),
        Err(_) => default,
    }
}
