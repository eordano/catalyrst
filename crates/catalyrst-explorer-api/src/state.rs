use crate::config::Config;
use crate::modules::auth_api::AuthApiState;
use crate::modules::feature_flags::FeatureFlagsState;
use crate::modules::runtime_config::RuntimeConfigState;
use std::sync::Arc;

pub struct AppStateInner {
    pub cfg: Config,
    pub http: reqwest::Client,
    pub auth_api: AuthApiState,
    pub feature_flags: FeatureFlagsState,
    pub runtime_config: RuntimeConfigState,
}

pub type AppState = Arc<AppStateInner>;
