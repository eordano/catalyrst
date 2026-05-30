use std::sync::Arc;

use crate::config::Config;
use crate::scene::SceneManager;

pub struct AppStateInner {
    pub cfg: Config,
    pub scenes: SceneManager,
    pub http: reqwest::Client,
}

pub type AppState = Arc<AppStateInner>;
