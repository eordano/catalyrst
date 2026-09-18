use crate::config::Config;
use crate::modules::auth_api::AuthApiState;
use crate::modules::blocklist::DenylistCache;
use crate::modules::feature_flags::FeatureFlagsState;
use crate::modules::onboarding::OnboardingState;
use crate::modules::realm_provider::{CatalystStatus, ExternalCatalyst, HotSceneInfo};
use crate::modules::runtime_config::RuntimeConfigState;
use crate::modules::swr::SwrCell;
use crate::modules::worlds_content_server::CachedUpstream;
use catalyrst_commons::cache::TtlMap;
use parking_lot::RwLock;
use std::sync::Arc;

pub struct AppStateInner {
    pub cfg: Config,
    pub http: reqwest::Client,
    pub auth_api: AuthApiState,
    pub feature_flags: FeatureFlagsState,
    pub runtime_config: RuntimeConfigState,
    pub onboarding: OnboardingState,
    pub denylist: RwLock<DenylistCache>,
    pub(crate) denylist_write: tokio::sync::Mutex<()>,
    pub(crate) catalyst_status_cache: SwrCell<Arc<CatalystStatus>>,
    pub(crate) external_catalyst_cache: TtlMap<String, Option<Arc<ExternalCatalyst>>>,
    pub(crate) hot_scenes_cache: SwrCell<Arc<Vec<HotSceneInfo>>>,
    pub(crate) world_doc_cache: TtlMap<String, Arc<CachedUpstream>>,
    pub(crate) contents_cache: TtlMap<String, Arc<CachedUpstream>>,
}

pub type AppState = Arc<AppStateInner>;
