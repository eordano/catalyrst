use anyhow::{Context, Result};
use serde::Deserialize;
use std::env;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub http_host: String,
    pub http_port: u16,
    pub cluster: ClusterConfig,
    pub server: ServerConfig,
    pub auth: AuthConfig,
    pub livekit: LivekitConfig,
    pub gossip: GossipConfig,

    pub content_database_url: Option<String>,

    pub content_base_url: String,

    pub commit_hash: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ClusterConfig {
    pub heartbeat_timeout_secs: u64,
    pub recluster_interval_secs: u64,
    pub island_radius_parcels: f32,
    pub island_max_peers: usize,
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            heartbeat_timeout_secs: 30,
            recluster_interval_secs: 2,
            island_radius_parcels: 4.0,
            island_max_peers: 50,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct ServerConfig {
    pub livekit_realm_prefix: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            livekit_realm_prefix: "wss://livekit.dcl.example".into(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct AuthConfig {
    #[serde(default)]
    pub require_signed_challenge: bool,
    #[serde(default = "default_challenge_ttl_secs")]
    pub challenge_ttl_secs: u64,
    #[serde(default = "default_signature_max_age_secs")]
    pub signature_max_age_secs: u64,
    #[serde(default)]
    pub deny_list_url: Option<String>,
}

pub const DEFAULT_DENY_LIST_URL: &str = "https://config.decentraland.org/denylist.json";

fn default_challenge_ttl_secs() -> u64 {
    120
}
fn default_signature_max_age_secs() -> u64 {
    300
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct LivekitConfig {
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub api_secret: Option<String>,
    #[serde(default = "default_lk_ws_url")]
    pub ws_url: String,
    #[serde(default = "default_lk_ttl_secs")]
    pub token_ttl_secs: i64,
    #[serde(default)]
    pub comms_gatekeeper_url: Option<String>,
}

fn default_lk_ws_url() -> String {
    "wss://livekit.dcl.example".into()
}
fn default_lk_ttl_secs() -> i64 {
    21600
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct GossipConfig {
    #[serde(default)]
    pub node_id: Option<String>,
    #[serde(default)]
    pub peers: Vec<String>,
    #[serde(default)]
    pub hmac_key: Option<String>,
    #[serde(default = "default_gossip_interval_secs")]
    pub interval_secs: u64,
    #[serde(default = "default_gossip_skew_secs")]
    pub max_clock_skew_secs: i64,
}

fn default_gossip_interval_secs() -> u64 {
    3
}
fn default_gossip_skew_secs() -> i64 {
    60
}

#[derive(Deserialize, Default)]
struct FileConfig {
    #[serde(default)]
    cluster: Option<ClusterConfig>,
    #[serde(default)]
    server: Option<ServerConfig>,
    #[serde(default)]
    auth: Option<AuthConfig>,
    #[serde(default)]
    livekit: Option<LivekitConfig>,
    #[serde(default)]
    gossip: Option<GossipConfig>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let http_host = env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".into());
        let http_port: u16 = env::var("HTTP_SERVER_PORT")
            .unwrap_or_else(|_| "5139".into())
            .parse()
            .context("HTTP_SERVER_PORT must be u16")?;

        let path = env::var("ARCHIPELAGO_CONFIG_PATH").ok().map(PathBuf::from);
        let (cluster, server, mut auth, mut livekit, mut gossip) = match path {
            Some(p) if p.exists() => {
                let raw = std::fs::read_to_string(&p)
                    .with_context(|| format!("read config {}", p.display()))?;
                let parsed: FileConfig =
                    toml::from_str(&raw).with_context(|| format!("parse toml {}", p.display()))?;
                (
                    parsed.cluster.unwrap_or_default(),
                    parsed.server.unwrap_or_default(),
                    parsed.auth.unwrap_or_default(),
                    parsed.livekit.unwrap_or_default(),
                    parsed.gossip.unwrap_or_default(),
                )
            }
            _ => (
                ClusterConfig::default(),
                ServerConfig::default(),
                AuthConfig::default(),
                LivekitConfig::default(),
                GossipConfig::default(),
            ),
        };

        if let Ok(v) = env::var("ARCHIPELAGO_REQUIRE_AUTH") {
            auth.require_signed_challenge = v == "1" || v.eq_ignore_ascii_case("true");
        }
        if livekit.api_key.is_none() {
            if let Ok(v) = env::var("LIVEKIT_API_KEY") {
                if !v.is_empty() {
                    livekit.api_key = Some(v);
                }
            }
        }
        if livekit.api_secret.is_none() {
            if let Ok(v) = env::var("LIVEKIT_API_SECRET") {
                if !v.is_empty() {
                    livekit.api_secret = Some(v);
                }
            }
        }
        if let Ok(v) = env::var("LIVEKIT_WS_URL") {
            if !v.is_empty() {
                livekit.ws_url = v;
            }
        }
        if livekit.comms_gatekeeper_url.is_none() {
            if let Ok(v) = env::var("COMMS_GATEKEEPER_URL") {
                if !v.is_empty() {
                    livekit.comms_gatekeeper_url = Some(v);
                }
            }
        }
        if auth.deny_list_url.is_none() {
            match env::var("DENY_LIST_URL") {
                Ok(v) if v.is_empty() => {}
                Ok(v) => auth.deny_list_url = Some(v),
                Err(_) => auth.deny_list_url = Some(DEFAULT_DENY_LIST_URL.to_string()),
            }
        }
        if gossip.node_id.is_none() {
            if let Ok(v) = env::var("ARCHIPELAGO_NODE_ID") {
                if !v.is_empty() {
                    gossip.node_id = Some(v);
                }
            }
        }
        if gossip.hmac_key.is_none() {
            if let Ok(v) = env::var("ARCHIPELAGO_GOSSIP_HMAC_KEY") {
                if !v.is_empty() {
                    gossip.hmac_key = Some(v);
                }
            }
        }
        if gossip.peers.is_empty() {
            if let Ok(v) = env::var("ARCHIPELAGO_GOSSIP_PEERS") {
                gossip.peers = v
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
        }

        let content_database_url = content_connection_string();
        let content_base_url = env::var("CONTENT_BASE_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "https://peer.decentraland.org/content".into());
        let commit_hash = env::var("COMMIT_HASH").unwrap_or_default();

        Ok(Self {
            http_host,
            http_port,
            cluster,
            server,
            auth,
            livekit,
            gossip,
            content_database_url,
            content_base_url,
            commit_hash,
        })
    }
}

fn content_connection_string() -> Option<String> {
    if let Ok(url) = env::var("CONTENT_PG_CONNECTION_STRING") {
        if !url.is_empty() {
            return Some(url);
        }
    }
    let user = env::var("POSTGRES_CONTENT_USER")
        .ok()
        .filter(|s| !s.is_empty())?;
    let host = env::var("POSTGRES_HOST").unwrap_or_else(|_| "./data/run".into());
    let port = env::var("POSTGRES_PORT").unwrap_or_else(|_| "6432".into());
    let password = env::var("POSTGRES_CONTENT_PASSWORD").unwrap_or_default();
    let db = env::var("POSTGRES_CONTENT_DB").unwrap_or_else(|_| "content".into());

    let esc = |s: &str| s.replace('\\', "\\\\").replace('\'', "\\'");
    Some(format!(
        "host='{}' port={} user='{}' password='{}' dbname='{}' connect_timeout=30",
        esc(&host),
        port,
        esc(&user),
        esc(&password),
        esc(&db),
    ))
}
