use crate::proto::v2::{
    BlockUpdate, CommunityMemberConnectivityUpdate, CommunityVoiceChatUpdate,
    FriendConnectivityUpdate, FriendshipUpdate, PrivateVoiceChatUpdate,
};
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::broadcast;

const CHANNEL_CAP: usize = 256;

#[derive(Clone)]
pub enum SocialEvent {
    Friendship(FriendshipUpdate),
    FriendConnectivity(FriendConnectivityUpdate),
    Block(BlockUpdate),
    PrivateVoice(PrivateVoiceChatUpdate),
    CommunityVoice(CommunityVoiceChatUpdate),
    CommunityMember(CommunityMemberConnectivityUpdate),
}

#[derive(Clone)]
pub struct PubSub {
    channels: Arc<DashMap<String, broadcast::Sender<SocialEvent>>>,
}

impl PubSub {
    pub fn new() -> Self {
        Self {
            channels: Arc::new(DashMap::new()),
        }
    }

    pub fn subscribe(&self, address: &str) -> broadcast::Receiver<SocialEvent> {
        let sender = self
            .channels
            .entry(address.to_lowercase())
            .or_insert_with(|| broadcast::channel(CHANNEL_CAP).0)
            .clone();
        sender.subscribe()
    }

    pub fn publish(&self, address: &str, event: SocialEvent) {
        if let Some(sender) = self.channels.get(&address.to_lowercase()) {
            let _ = sender.send(event);
        }
    }
}

impl Default for PubSub {
    fn default() -> Self {
        Self::new()
    }
}
