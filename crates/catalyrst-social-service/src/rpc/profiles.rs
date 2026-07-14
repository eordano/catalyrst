use crate::rpc::proto::common::Color3;
use crate::rpc::proto::v2::{BlockedUserProfile, FriendProfile};
use dashmap::DashMap;
use sqlx::PgPool;
use std::time::{Duration, Instant};

const CACHE_TTL: Duration = Duration::from_secs(300);

#[derive(Clone)]
pub struct ProfileInfo {
    pub name: String,
    pub profile_picture_url: String,
    pub has_claimed_name: bool,
    pub name_color: Option<Color3>,
}

struct CacheEntry {
    info: Option<ProfileInfo>,
    fetched_at: Instant,
}

#[derive(Clone)]
pub struct Profiles {
    pool: Option<PgPool>,
    content_base: String,
    cache: std::sync::Arc<DashMap<String, CacheEntry>>,
}

impl Profiles {
    pub fn new(pool: Option<PgPool>, content_base: String) -> Self {
        let content_base = content_base.trim_end_matches('/').to_string();
        Self {
            pool,
            content_base,
            cache: std::sync::Arc::new(DashMap::new()),
        }
    }

    fn picture_url(&self, face256: &str) -> String {
        format!("{}/contents/{}", self.content_base, face256)
    }

    pub async fn get_profiles(
        &self,
        addresses: &[String],
    ) -> std::collections::HashMap<String, ProfileInfo> {
        let mut out: std::collections::HashMap<String, ProfileInfo> =
            std::collections::HashMap::new();
        if addresses.is_empty() {
            return out;
        }

        let mut wanted: Vec<String> = Vec::new();
        for a in addresses {
            let lc = a.to_lowercase();
            if !wanted.contains(&lc) {
                wanted.push(lc);
            }
        }

        let mut misses: Vec<String> = Vec::new();
        let now = Instant::now();
        for addr in &wanted {
            match self.cache.get(addr) {
                Some(e) if now.duration_since(e.fetched_at) < CACHE_TTL => {
                    if let Some(info) = &e.info {
                        out.insert(addr.clone(), info.clone());
                    }
                }
                _ => misses.push(addr.clone()),
            }
        }

        if misses.is_empty() {
            return out;
        }

        let Some(pool) = &self.pool else {
            for addr in misses {
                self.cache.insert(
                    addr,
                    CacheEntry {
                        info: None,
                        fetched_at: Instant::now(),
                    },
                );
            }
            return out;
        };

        let rows = sqlx::query_as::<
            _,
            (
                String,
                Option<String>,
                Option<String>,
                Option<bool>,
                Option<f64>,
                Option<f64>,
                Option<f64>,
            ),
        >(
            "SELECT lower(d.entity_pointers[1]) AS addr, \
                    COALESCE(d.entity_metadata::jsonb #>> '{v,avatars,0,name}', \
                             d.entity_metadata::jsonb #>> '{v,avatars,0,unclaimedName}') AS name, \
                    d.entity_metadata::jsonb #>> '{v,avatars,0,avatar,snapshots,face256}' AS face256, \
                    (d.entity_metadata::jsonb #>> '{v,avatars,0,hasClaimedName}')::bool AS has_claimed, \
                    (d.entity_metadata::jsonb #>> '{v,avatars,0,nameColor,r}')::float8 AS color_r, \
                    (d.entity_metadata::jsonb #>> '{v,avatars,0,nameColor,g}')::float8 AS color_g, \
                    (d.entity_metadata::jsonb #>> '{v,avatars,0,nameColor,b}')::float8 AS color_b \
             FROM deployments d \
             WHERE d.entity_type = 'profile' \
               AND d.deleter_deployment IS NULL \
               AND d.entity_pointers && $1::text[]",
        )
        .bind(&misses)
        .fetch_all(pool)
        .await;

        let mut resolved: std::collections::HashMap<String, ProfileInfo> =
            std::collections::HashMap::new();
        match rows {
            Ok(rows) => {
                for (addr, name, face256, has_claimed, cr, cg, cb) in rows {
                    let name = match name {
                        Some(n) if !n.is_empty() => n,
                        _ => continue,
                    };
                    let face = match face256 {
                        Some(f) if !f.is_empty() => f,
                        _ => continue,
                    };
                    let name_color = match (cr, cg, cb) {
                        (Some(r), Some(g), Some(b)) => Some(Color3 {
                            r: r as f32,
                            g: g as f32,
                            b: b as f32,
                        }),
                        _ => None,
                    };
                    resolved.insert(
                        addr.clone(),
                        ProfileInfo {
                            name,
                            profile_picture_url: self.picture_url(&face),
                            has_claimed_name: has_claimed.unwrap_or(false),
                            name_color,
                        },
                    );
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "profile enrichment query failed; serving placeholders");
            }
        }

        let now = Instant::now();
        for addr in misses {
            let info = resolved.get(&addr).cloned();
            if let Some(info) = &info {
                out.insert(addr.clone(), info.clone());
            }
            self.cache.insert(
                addr,
                CacheEntry {
                    info,
                    fetched_at: now,
                },
            );
        }

        out
    }

    pub async fn friend_profile(&self, address: &str) -> FriendProfile {
        match self.get_profile(address).await {
            Some(info) => FriendProfile {
                address: address.to_string(),
                name: info.name,
                has_claimed_name: info.has_claimed_name,
                profile_picture_url: info.profile_picture_url,
                name_color: info.name_color,
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
                        name_color: info.name_color.clone(),
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
                name_color: info.name_color,
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

    pub async fn get_profile(&self, address: &str) -> Option<ProfileInfo> {
        self.get_profiles(std::slice::from_ref(&address.to_string()))
            .await
            .remove(&address.to_lowercase())
    }
}
