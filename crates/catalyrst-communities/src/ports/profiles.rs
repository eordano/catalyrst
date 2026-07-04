use std::collections::HashMap;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::Serialize;
use sqlx::PgPool;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "communities/")
)]
pub struct NameColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProfileInfo {
    pub name: String,
    #[serde(rename = "profilePictureUrl")]
    pub profile_picture_url: String,
    #[serde(rename = "hasClaimedName")]
    pub has_claimed_name: bool,
    #[serde(rename = "nameColor", skip_serializing_if = "Option::is_none")]
    pub name_color: Option<NameColor>,
}

struct CacheEntry {
    info: Option<ProfileInfo>,
    fetched_at: Instant,
}

const CACHE_TTL: Duration = Duration::from_secs(300);

pub struct ProfilesComponent {
    pool: Option<PgPool>,
    content_base: String,
    cache: Mutex<HashMap<String, CacheEntry>>,
}

impl ProfilesComponent {
    pub fn new(pool: Option<PgPool>, content_base: String) -> Self {
        let content_base = content_base.trim_end_matches('/').to_string();
        Self {
            pool,
            content_base,
            cache: Mutex::new(HashMap::new()),
        }
    }

    fn picture_url(&self, face256: &str) -> String {
        format!("{}/contents/{}", self.content_base, face256)
    }

    pub async fn get_profiles(&self, addresses: &[String]) -> HashMap<String, ProfileInfo> {
        let mut out: HashMap<String, ProfileInfo> = HashMap::new();
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
        {
            let now = Instant::now();
            let cache = self.cache.lock();
            for addr in &wanted {
                match cache.get(addr) {
                    Some(e) if now.duration_since(e.fetched_at) < CACHE_TTL => {
                        if let Some(info) = &e.info {
                            out.insert(addr.clone(), info.clone());
                        }
                    }
                    _ => misses.push(addr.clone()),
                }
            }
        }

        if misses.is_empty() {
            return out;
        }

        let Some(pool) = &self.pool else {
            let mut cache = self.cache.lock();
            for addr in misses {
                cache.insert(
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

        let mut resolved: HashMap<String, ProfileInfo> = HashMap::new();
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
                        (Some(r), Some(g), Some(b)) => Some(NameColor {
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

        let mut cache = self.cache.lock();
        let now = Instant::now();
        for addr in misses {
            let info = resolved.get(&addr).cloned();
            if let Some(info) = &info {
                out.insert(addr.clone(), info.clone());
            }
            cache.insert(
                addr,
                CacheEntry {
                    info,
                    fetched_at: now,
                },
            );
        }

        out
    }

    pub async fn get_profile(&self, address: &str) -> Option<ProfileInfo> {
        self.get_profiles(std::slice::from_ref(&address.to_string()))
            .await
            .remove(&address.to_lowercase())
    }

    pub async fn get_owner_names(&self, addresses: &[String]) -> HashMap<String, String> {
        self.get_profiles(addresses)
            .await
            .into_iter()
            .map(|(addr, info)| (addr, info.name))
            .collect()
    }

    pub async fn has_owned_name(&self, address: &str) -> Option<bool> {
        let pool = self.pool.as_ref()?;
        let addr = address.to_lowercase();
        let row: Option<(bool,)> = sqlx::query_as(
            "SELECT COALESCE((d.entity_metadata::jsonb #>> '{v,avatars,0,hasClaimedName}')::bool, false) \
             FROM deployments d \
             WHERE d.entity_type = 'profile' \
               AND d.deleter_deployment IS NULL \
               AND d.entity_pointers && ARRAY[$1]::text[] \
             LIMIT 1",
        )
        .bind(&addr)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
        Some(row.map(|(c,)| c).unwrap_or(false))
    }
}
