use crate::config::Config;
use std::sync::Arc;

pub struct AppStateInner {
    pub cfg: Config,
    pub http: reqwest::Client,
}

pub type AppState = Arc<AppStateInner>;
