//! Configuration for the standalone dcl.social Rust service.
use std::path::PathBuf;

pub struct Config {
    pub upstream: String,
    pub database: String,
    pub assets: PathBuf,
    pub bind: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let upstream = std::env::var("SOCIAL_UPSTREAM")
            .unwrap_or("https://social-api.decentraland.org".into());
        let url = reqwest::Url::parse(&upstream)?;
        let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1"))
            && std::env::var("SOCIAL_ALLOW_LOCAL_UPSTREAM").as_deref() == Ok("1");
        anyhow::ensure!(
            local
                || (url.scheme() == "https"
                    && matches!(
                        url.host_str(),
                        Some("social-api.decentraland.org" | "social-api.decentraland.zone")
                    )),
            "unsupported Foundation upstream"
        );
        anyhow::ensure!(
            url.path() == "/"
                && url.query().is_none()
                && url.fragment().is_none()
                && url.username().is_empty()
                && url.password().is_none(),
            "upstream must be an origin"
        );
        let database = std::env::var("SOCIAL_DATABASE").unwrap_or("dcl-social.sqlite".into());
        let assets = PathBuf::from(std::env::var("SOCIAL_ASSETS").unwrap_or("social/dist".into()));
        let bind = std::env::var("SOCIAL_BIND").unwrap_or("127.0.0.1:5191".into());
        Ok(Self {
            upstream,
            database,
            assets,
            bind,
        })
    }
}
