use crate::profiles::{NameColor, ProfilesCache};
use crate::rpc::proto::common::Color3;
use crate::rpc::proto::v2::{BlockedUserProfile, FriendProfile};

pub type Profiles = ProfilesCache;

impl From<NameColor> for Color3 {
    fn from(c: NameColor) -> Self {
        Color3 {
            r: c.r,
            g: c.g,
            b: c.b,
        }
    }
}

impl ProfilesCache {
    pub async fn friend_profile(&self, address: &str) -> FriendProfile {
        match self.get_profile(address).await {
            Some(info) => FriendProfile {
                address: address.to_string(),
                name: info.name,
                has_claimed_name: info.has_claimed_name,
                profile_picture_url: info.profile_picture_url,
                name_color: info.name_color.map(Into::into),
            },
            None => FriendProfile {
                address: address.to_string(),
                name: String::new(),
                has_claimed_name: false,
                profile_picture_url: String::new(),
                name_color: None,
            },
        }
    }

    pub async fn friend_profiles(&self, addresses: &[String]) -> Vec<FriendProfile> {
        let map = self.get_profiles(addresses).await;
        addresses
            .iter()
            .map(|a| {
                let key = a.to_lowercase();
                match map.get(&key) {
                    Some(info) => FriendProfile {
                        address: a.clone(),
                        name: info.name.clone(),
                        has_claimed_name: info.has_claimed_name,
                        profile_picture_url: info.profile_picture_url.clone(),
                        name_color: info.name_color.map(Into::into),
                    },
                    None => FriendProfile {
                        address: a.clone(),
                        name: String::new(),
                        has_claimed_name: false,
                        profile_picture_url: String::new(),
                        name_color: None,
                    },
                }
            })
            .collect()
    }

    pub async fn blocked_profile(
        &self,
        address: &str,
        blocked_at_ms: Option<i64>,
    ) -> BlockedUserProfile {
        match self.get_profile(address).await {
            Some(info) => BlockedUserProfile {
                address: address.to_string(),
                name: info.name,
                has_claimed_name: info.has_claimed_name,
                profile_picture_url: info.profile_picture_url,
                blocked_at: blocked_at_ms,
                name_color: info.name_color.map(Into::into),
            },
            None => BlockedUserProfile {
                address: address.to_string(),
                name: String::new(),
                has_claimed_name: false,
                profile_picture_url: String::new(),
                blocked_at: blocked_at_ms,
                name_color: None,
            },
        }
    }
}
