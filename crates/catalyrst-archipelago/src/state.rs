use crate::auth::ChallengeStore;
use crate::cluster::Cluster;
use crate::config::Config;
use crate::content::ContentResolver;
use crate::gossip::GossipBus;
use crate::livekit::LivekitMinter;
use std::sync::Arc;

pub struct AppStateInner {
    pub cfg: Config,
    pub cluster: Arc<Cluster>,
    pub challenges: Arc<ChallengeStore>,
    pub livekit: Arc<LivekitMinter>,
    pub gossip: Arc<GossipBus>,

    pub content: Arc<ContentResolver>,
}

pub type AppState = Arc<AppStateInner>;
