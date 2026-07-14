use anyhow::Result;
use catalyrst_envcfg::{env_bool, get_port, get_u64};
use std::env;
use std::path::PathBuf;

pub struct Config {
    pub http_host: String,
    pub http_port: u16,
    pub store_root: PathBuf,
    pub quarantine_list: PathBuf,
    pub store_max_bytes: u64,
    pub cdn_base: String,
    pub cdn_realm_segment: String,
    pub readthrough_timeout_secs: u64,
    pub bake_enabled: bool,
    pub bake_wrapper: String,
    pub impost_bin: String,
    pub impost_server: String,
    pub impost_content_server: String,
    pub bake_queue_depth: usize,
    pub bake_timeout_secs: u64,
    pub bake_max_failures: u32,
    pub bake_quarantine_secs: u64,
}

fn get_str(key: &str, default: &str) -> String {
    env::var(key)
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let store_root = PathBuf::from(get_str("BVIMPOSTERS_STORE_ROOT", "/var/lib/bvimposters"));
        let quarantine_list = match get_str("BVIMPOSTERS_QUARANTINE_LIST", "") {
            v if v.is_empty() => store_root.join("readthrough-quarantine.txt"),
            v => PathBuf::from(v),
        };
        Ok(Self {
            http_host: get_str("HTTP_SERVER_HOST", "127.0.0.1"),
            http_port: get_port("HTTP_SERVER_PORT", 5154)?,
            store_root,
            quarantine_list,
            store_max_bytes: get_u64("BVIMPOSTERS_STORE_MAX_BYTES", 21474836480)?,
            cdn_base: get_str(
                "BVIMPOSTERS_CDN_BASE",
                "https://bevy-imposters.dclregenesislabs.xyz",
            ),
            cdn_realm_segment: get_str(
                "BVIMPOSTERS_CDN_REALM_SEGMENT",
                "https%253A%252F%252Frealm-provider-ea.decentraland.org%252Fmain%252Fabout",
            ),
            readthrough_timeout_secs: get_u64("BVIMPOSTERS_READTHROUGH_TIMEOUT_SECS", 30)?,
            bake_enabled: env_bool("BVIMPOSTERS_BAKE_ENABLED", false),
            bake_wrapper: env::var("BVIMPOSTERS_BAKE_WRAPPER").unwrap_or_default(),
            impost_bin: get_str("BVIMPOSTERS_IMPOST_BIN", "impost"),
            impost_server: get_str("BVIMPOSTERS_IMPOST_SERVER", "https://catalyst.example.com"),
            impost_content_server: env::var("BVIMPOSTERS_IMPOST_CONTENT_SERVER")
                .unwrap_or_default()
                .trim()
                .to_string(),
            bake_queue_depth: get_u64("BVIMPOSTERS_BAKE_QUEUE_DEPTH", 1)?.clamp(1, 2) as usize,
            bake_timeout_secs: get_u64("BVIMPOSTERS_BAKE_TIMEOUT_SECS", 1800)?,
            bake_max_failures: get_u64("BVIMPOSTERS_BAKE_MAX_FAILURES", 3)? as u32,
            bake_quarantine_secs: get_u64("BVIMPOSTERS_BAKE_QUARANTINE_SECS", 86400)?,
        })
    }
}
