use crate::rpc::config::Config;
use crate::rpc::db::Db;
use crate::rpc::gatekeeper::Gatekeeper;
use crate::rpc::profiles::Profiles;
use crate::rpc::proto::v2::{
    CommunityMemberConnectivityUpdate, ConnectivityStatus, FriendConnectivityUpdate,
    PrivateVoiceChatStatus, PrivateVoiceChatUpdate, User,
};
use crate::rpc::pubsub::{PubSub, SocialEvent};
use catalyrst_types::EthAddress;
use dashmap::DashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::Notify;

pub struct ContextInner {
    pub cfg: Config,
    pub db: Db,
    pub pubsub: PubSub,
    pub gatekeeper: Gatekeeper,
    pub profiles: Profiles,
    identities: DashMap<u32, EthAddress>,
    presence: DashMap<String, u32>,

    kill_handles: DashMap<u32, Arc<Notify>>,
    connections: AtomicUsize,
}

#[derive(Clone)]
pub struct Context(Arc<ContextInner>);

impl Context {
    pub fn new(cfg: Config, db: Db, profiles: Profiles) -> Self {
        let gatekeeper = Gatekeeper::new(cfg.comms_gatekeeper_url.clone());
        Self(Arc::new(ContextInner {
            cfg,
            db,
            pubsub: PubSub::new(),
            gatekeeper,
            profiles,
            identities: DashMap::new(),
            presence: DashMap::new(),
            kill_handles: DashMap::new(),
            connections: AtomicUsize::new(0),
        }))
    }

    pub fn cfg(&self) -> &Config {
        &self.0.cfg
    }
    pub fn db(&self) -> &Db {
        &self.0.db
    }
    pub fn pubsub(&self) -> &PubSub {
        &self.0.pubsub
    }
    pub fn gatekeeper(&self) -> &Gatekeeper {
        &self.0.gatekeeper
    }
    pub fn profiles(&self) -> &Profiles {
        &self.0.profiles
    }

    pub fn register_identity(&self, transport_id: u32, address: EthAddress) {
        self.0.identities.insert(transport_id, address);
    }

    pub fn connection_opened(&self) {
        self.0.connections.fetch_add(1, Ordering::Relaxed);
    }

    pub fn connection_closed(&self) {
        self.0.connections.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn live_connections(&self) -> usize {
        self.0.connections.load(Ordering::Relaxed)
    }

    pub fn register_kill_handle(&self, transport_id: u32, kill: Arc<Notify>) {
        self.0.kill_handles.insert(transport_id, kill);
    }

    pub fn forget_identity(&self, transport_id: u32) {
        self.0.identities.remove(&transport_id);
        self.0.kill_handles.remove(&transport_id);
    }

    pub fn identity(&self, transport_id: u32) -> Option<EthAddress> {
        self.0.identities.get(&transport_id).map(|r| r.clone())
    }

    pub fn presence_snapshot(&self) -> Vec<(String, u32)> {
        self.0
            .presence
            .iter()
            .map(|e| (e.key().clone(), *e.value()))
            .collect()
    }

    pub fn online_count(&self) -> usize {
        self.0.presence.len()
    }

    pub fn disconnect_address(&self, address: &str) -> usize {
        let addr = address.to_lowercase();
        let ids: Vec<u32> = self
            .0
            .identities
            .iter()
            .filter(|e| e.value().to_lowercase() == addr)
            .map(|e| *e.key())
            .collect();
        let mut kicked = 0;
        for id in ids {
            if let Some(handle) = self.0.kill_handles.get(&id) {
                handle.notify_waiters();
                kicked += 1;
            }
        }
        kicked
    }

    pub fn mark_online(&self, address: &str) -> bool {
        let addr = address.to_lowercase();
        let mut entry = self.0.presence.entry(addr).or_insert(0);
        *entry += 1;
        *entry == 1
    }

    pub fn mark_offline(&self, address: &str) -> bool {
        let addr = address.to_lowercase();
        let mut became_offline = false;
        if let Some(mut entry) = self.0.presence.get_mut(&addr) {
            if *entry > 0 {
                *entry -= 1;
            }
            became_offline = *entry == 0;
        }
        if became_offline {
            self.0.presence.remove(&addr);
        }
        became_offline
    }

    pub fn is_online(&self, address: &str) -> bool {
        self.0
            .presence
            .get(&address.to_lowercase())
            .map(|c| *c > 0)
            .unwrap_or(false)
    }

    pub async fn fan_connectivity(&self, address: &str, status: ConnectivityStatus) {
        let address = address.to_lowercase();

        if let Ok(friends) = self.0.db.friend_addresses(&address).await {
            let profile = self.0.profiles.friend_profile(&address).await;
            for friend in &friends {
                self.0.pubsub.publish(
                    friend,
                    SocialEvent::FriendConnectivity(FriendConnectivityUpdate {
                        friend: Some(profile.clone()),
                        status: status as i32,
                    }),
                );
            }
        }

        if let Ok(communities) = self.0.db.communities_for_member(&address).await {
            for community_id in &communities {
                if let Ok(members) = self.0.db.community_member_addresses(community_id).await {
                    for member in &members {
                        if member == &address {
                            continue;
                        }
                        self.0.pubsub.publish(
                            member,
                            SocialEvent::CommunityMember(CommunityMemberConnectivityUpdate {
                                community_id: community_id.clone(),
                                member: Some(User {
                                    address: address.clone(),
                                }),
                                status: status as i32,
                            }),
                        );
                    }
                }
            }
        }
    }

    pub async fn expire_private_voice_chats(&self, expiration_ms: i64, batch_size: i64) -> usize {
        let mut total = 0usize;
        loop {
            let expired = match self
                .0
                .db
                .expire_private_voice_chats(expiration_ms, batch_size)
                .await
            {
                Ok(rows) => rows,
                Err(e) => {
                    tracing::warn!(error = %e, "private voice chat expiry sweep failed");
                    break;
                }
            };
            if expired.is_empty() {
                break;
            }
            for (id, caller, callee) in &expired {
                let update = PrivateVoiceChatUpdate {
                    call_id: id.to_string(),
                    status: PrivateVoiceChatStatus::VoiceChatExpired as i32,
                    caller: Some(User {
                        address: caller.clone(),
                    }),
                    callee: Some(User {
                        address: callee.clone(),
                    }),
                    credentials: None,
                };
                self.0
                    .pubsub
                    .publish(caller, SocialEvent::PrivateVoice(update.clone()));
                self.0
                    .pubsub
                    .publish(callee, SocialEvent::PrivateVoice(update));
            }
            total += expired.len();
        }
        total
    }
}

pub type SharedContext = Context;
