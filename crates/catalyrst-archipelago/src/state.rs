use crate::auth::ChallengeStore;
use crate::ban::{BanChecker, DenyList};
use crate::config::Config;
use crate::content::ContentResolver;
use crate::control_v4::AssignmentAuthority;
use crate::feed::FeedCache;
use crate::livekit::LivekitMinter;
use crate::nats::FeedPublisher;
use crate::peers::PeerDirectory;
use crate::registry::PeersRegistry;
use std::sync::Arc;

pub struct AppStateInner {
    pub cfg: Config,
    pub peers: Arc<PeerDirectory>,
    pub registry: Arc<PeersRegistry>,
    pub feed: Arc<FeedCache>,
    pub publisher: Arc<dyn FeedPublisher>,
    pub challenges: Arc<ChallengeStore>,
    pub livekit: Arc<LivekitMinter>,

    pub content: Arc<ContentResolver>,
    pub control_v4: Arc<AssignmentAuthority>,
    pub replica_id: String,

    pub ban_checker: Arc<BanChecker>,
    pub deny_list: Arc<DenyList>,
}

pub type AppState = Arc<AppStateInner>;
