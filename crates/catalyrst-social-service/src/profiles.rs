use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use catalyrst_commons::cache::TtlMap;
use serde::Serialize;
use sqlx::PgPool;

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
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

const CACHE_TTL: Duration = Duration::from_secs(300);

/// Preserves first-seen order. `HashSet` rather than `Vec::contains`, which would make this
/// shared enrichment primitive O(n^2) in the address count.
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

#[derive(Clone)]
pub struct ProfilesCache {
    pub(crate) pool: Option<PgPool>,
    content_base: String,
    cache: Arc<TtlMap<String, Option<ProfileInfo>>>,
}

impl ProfilesCache {
    pub fn new(pool: Option<PgPool>, content_base: String) -> Self {
        let content_base = content_base.trim_end_matches('/').to_string();
        Self {
            pool,
            content_base,
            cache: Arc::new(TtlMap::new("social-profiles", CACHE_TTL)),
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
        for addr in &wanted {
            match self.cache.get_fresh(addr) {
                Some(Some(info)) => {
                    out.insert(addr.clone(), info);
                }
                Some(None) => {}
                None => misses.push(addr.clone()),
            }
        }

        if misses.is_empty() {
            return out;
        }

        let Some(pool) = &self.pool else {
            for addr in misses {
                self.cache.insert(addr, None);
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

        for addr in misses {
            let info = resolved.get(&addr).cloned();
            if let Some(info) = &info {
                out.insert(addr.clone(), info.clone());
            }
            self.cache.insert(addr, info);
        }

        out
    }

    pub async fn get_profile(&self, address: &str) -> Option<ProfileInfo> {
        self.get_profiles(std::slice::from_ref(&address.to_string()))
            .await
            .remove(&address.to_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::dedup_lowercased;

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

    /// Seeded so the random cases are deterministic and need no `rand` dep.
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
        assert!(
            elapsed < std::time::Duration::from_secs(2),
            "dedup took {elapsed:?}, expected < 2s"
        );
    }
}
