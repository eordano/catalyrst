use anyhow::Result;
use catalyrst_envcfg::{get_port, required};
use std::env;
use std::path::PathBuf;

/// Default upstream host baked into the seed fixture (migrations/0002,
/// 0005). Kept as the default `public_asset_base_url` so `tests/upstream_parity.rs`
/// stays green with no env vars set -- self-hosted deployments override via
/// `BADGES_PUBLIC_ASSET_BASE_URL`.
pub const DEFAULT_ASSET_BASE_URL: &str = "https://badges.decentraland.org";

pub struct Config {
    pub http_host: String,
    pub http_port: u16,
    pub badges_database_url: String,

    pub admin_token: Option<String>,

    /// Directory served at `/assets` (tower_http ServeDir). Missing files
    /// 404 per-request rather than failing startup, so an unset/empty dir is
    /// a soft degradation, not a crash.
    pub assets_dir: PathBuf,
    /// Host+scheme prefix substituted for `DEFAULT_ASSET_BASE_URL` in every
    /// `assets` JSONB blob at serve time (DB rows stay upstream-parity-faithful;
    /// only the HTTP response is environment-rewritten).
    pub public_asset_base_url: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port: get_port("HTTP_SERVER_PORT", 5147)?,
            badges_database_url: required("BADGES_PG_CONNECTION_STRING")?,
            admin_token: env::var("CATALYRST_BADGES_ADMIN_TOKEN")
                .ok()
                .filter(|s| !s.is_empty()),
            assets_dir: env::var("BADGES_ASSETS_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("./assets")),
            public_asset_base_url: env::var("BADGES_PUBLIC_ASSET_BASE_URL")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| DEFAULT_ASSET_BASE_URL.to_string()),
        })
    }
}
