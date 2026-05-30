use crate::config::Config;
use crate::db::Db;
use crate::gatekeeper::Gatekeeper;
use crate::profiles::Profiles;
use crate::proto::v2::{
    CommunityMemberConnectivityUpdate, ConnectivityStatus, FriendConnectivityUpdate, User,
};
use crate::pubsub::{PubSub, SocialEvent};
use catalyrst_types::EthAddress;
use dashmap::DashMap;
use std::sync::Arc;

pub struct ContextInner {
    pub cfg: Config,
    pub db: Db,
    pub pubsub: PubSub,
    pub gatekeeper: Gatekeeper,
    pub profiles: Profiles,
    identities: DashMap<u32, EthAddress>,
    presence: DashMap<String, u32>,
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

    pub fn forget_identity(&self, transport_id: u32) {
        self.0.identities.remove(&transport_id);
    }

    pub fn identity(&self, transport_id: u32) -> Option<EthAddress> {
        self.0.identities.get(&transport_id).map(|r| r.clone())
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
}

pub type SharedContext = Context;
