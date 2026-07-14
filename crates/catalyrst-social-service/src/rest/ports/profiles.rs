use std::collections::{HashMap, HashSet};
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

/// Lowercase every address and drop duplicates, preserving first-seen order.
///
/// Membership is tracked with a `HashSet` (O(1) per probe) instead of the previous
/// `Vec::contains` scan, which made this shared enrichment primitive O(n^2) in the
/// address count. Output is identical: same lowercasing, same first-seen order.
fn dedup_lowercased(addresses: &[String]) -> Vec<String> {
    let mut seen = HashSet::with_capacity(addresses.len());
    let mut wanted = Vec::with_capacity(addresses.len());
    for a in addresses {
        let lc = a.to_lowercase();
        if seen.insert(lc.clone()) {
            wanted.push(lc);
        }
    }
    wanted
}

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

        let wanted = dedup_lowercased(addresses);

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

#[cfg(test)]
mod tests {
    use super::dedup_lowercased;

    /// The `Vec::contains` reference this fix replaced.
    fn reference(addrs: &[String]) -> Vec<String> {
        let mut w = Vec::new();
        for a in addrs {
            let lc = a.to_lowercase();
            if !w.contains(&lc) {
                w.push(lc);
            }
        }
        w
    }

    /// A tiny seeded LCG so the 50 random cases are deterministic and need no rand dep.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0 >> 33
        }
    }

    #[test]
    fn dedup_lowercased_preserves_first_seen_order_and_matches_reference() {
        let mixed: Vec<String> = ["0xAbC", "0xabc", "0xDEF", "0xdef", "0xABC", "0xghi"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(dedup_lowercased(&mixed), vec!["0xabc", "0xdef", "0xghi"]);
        assert_eq!(dedup_lowercased(&mixed), reference(&mixed));

        assert_eq!(dedup_lowercased(&[]), reference(&[]));

        let dups: Vec<String> = vec!["0xZ".to_string(); 8];
        assert_eq!(dedup_lowercased(&dups), reference(&dups));

        // 50 deterministic pseudo-random lists over a 10-address alphabet with random
        // casing.
        let alphabet = ["aa", "bb", "cc", "dd", "ee", "ff", "gg", "hh", "ii", "jj"];
        let mut rng = Lcg(0x1234_5678_9abc_def0);
        for _ in 0..50 {
            let len = (rng.next() % 30) as usize;
            let list: Vec<String> = (0..len)
                .map(|_| {
                    let base = alphabet[(rng.next() % 10) as usize];
                    if rng.next().is_multiple_of(2) {
                        format!("0X{}", base.to_uppercase())
                    } else {
                        format!("0x{base}")
                    }
                })
                .collect();
            assert_eq!(
                dedup_lowercased(&list),
                reference(&list),
                "mismatch for {list:?}"
            );
        }
    }

    #[test]
    fn dedup_of_twenty_thousand_addresses_is_not_quadratic() {
        // 20_000 addresses cycling through 5_000 unique values, alternating case, built
        // OUTSIDE the timer.
        let addrs: Vec<String> = (0..20_000)
            .map(|i| {
                let v = i % 5_000;
                if i % 2 == 0 {
                    format!("0xADDR{v}")
                } else {
                    format!("0xaddr{v}")
                }
            })
            .collect();
        let start = std::time::Instant::now();
        let out = dedup_lowercased(&addrs);
        let elapsed = start.elapsed();
        assert_eq!(out.len(), 5_000);
        // Comfortably true for O(n) (~tens of ms in debug), comfortably false for the
        // O(n^2) Vec::contains version (>10s in debug).
        assert!(
            elapsed < std::time::Duration::from_secs(2),
            "dedup took {elapsed:?}, expected < 2s"
        );
    }
}
