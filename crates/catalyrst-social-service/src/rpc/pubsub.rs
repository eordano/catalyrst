use crate::rpc::proto::v2::{
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
    // `Arc<SocialEvent>` rather than `SocialEvent`: one client holds up to six receivers on
    // an address, and `broadcast::Receiver::recv` clones the stored value once per receiver.
    // With the Arc each recv is a refcount bump and only the matched inner variant is cloned,
    // inside the picker.
    channels: Arc<DashMap<String, broadcast::Sender<Arc<SocialEvent>>>>,
}

impl PubSub {
    pub fn new() -> Self {
        Self {
            channels: Arc::new(DashMap::new()),
        }
    }

    pub fn subscribe(&self, address: &str) -> broadcast::Receiver<Arc<SocialEvent>> {
        let sender = self
            .channels
            .entry(address.to_lowercase())
            .or_insert_with(|| broadcast::channel(CHANNEL_CAP).0)
            .clone();
        sender.subscribe()
    }

    pub fn publish(&self, address: &str, event: SocialEvent) {
        if let Some(sender) = self.channels.get(&address.to_lowercase()) {
            let _ = sender.send(Arc::new(event));
        }
    }
}

impl Default for PubSub {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The pre-fix `broadcast::Sender<SocialEvent>` deep-cloned the whole payload once per
    // live receiver; all six must now observe one shared allocation.
    #[test]
    fn publish_is_shared_by_every_receiver() {
        let ps = PubSub::new();
        let mut rxs: Vec<_> = (0..6).map(|_| ps.subscribe("0xabc")).collect();
        ps.publish(
            "0xABC",
            SocialEvent::CommunityMember(CommunityMemberConnectivityUpdate {
                community_id: "c".into(),
                member: None,
                status: 0,
            }),
        );

        let got: Vec<_> = rxs.iter_mut().map(|rx| rx.try_recv().unwrap()).collect();

        assert_eq!(got.len(), 6);
        assert!(
            got.iter().all(|e| Arc::ptr_eq(e, &got[0])),
            "receivers did not share one Arc allocation"
        );
    }
}
