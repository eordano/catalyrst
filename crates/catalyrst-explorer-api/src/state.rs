use crate::config::Config;
use crate::modules::auth_api::AuthApiState;
use crate::modules::blocklist::Denylist;
use crate::modules::feature_flags::FeatureFlagsState;
use crate::modules::onboarding::OnboardingState;
use crate::modules::runtime_config::RuntimeConfigState;
use parking_lot::RwLock;
use std::sync::Arc;

pub struct AppStateInner {
    pub cfg: Config,
    pub http: reqwest::Client,
    pub auth_api: AuthApiState,
    pub feature_flags: FeatureFlagsState,
    pub runtime_config: RuntimeConfigState,
    pub onboarding: OnboardingState,
    pub denylist: RwLock<Arc<Denylist>>,
    pub(crate) catalyst_status_cache: RwLock<
        Option<(
            std::time::Instant,
            Option<Arc<crate::modules::realm_provider::CatalystStatus>>,
        )>,
    >,
    pub(crate) hot_scenes_cache: RwLock<
        Option<(
            std::time::Instant,
            Arc<Vec<crate::modules::realm_provider::HotSceneInfo>>,
        )>,
    >,
}

pub type AppState = Arc<AppStateInner>;
