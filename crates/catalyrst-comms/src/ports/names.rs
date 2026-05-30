use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use sqlx::PgPool;

const CACHE_TTL: Duration = Duration::from_secs(300);

struct CacheEntry {
    name: String,
    at: Instant,
}

#[derive(Clone)]
pub struct NamesComponent {
    pool: Option<PgPool>,
    schema: String,
    cache: Arc<DashMap<String, CacheEntry>>,
}

impl NamesComponent {
    pub fn new(pool: Option<PgPool>, schema: String) -> Self {
        Self {
            pool,
            schema,
            cache: Arc::new(DashMap::new()),
        }
    }

    pub async fn get_names_from_addresses(&self, addresses: &[String]) -> HashMap<String, String> {
        let mut out: HashMap<String, String> = HashMap::new();
        if addresses.is_empty() {
            return out;
        }

        let mut misses: Vec<String> = Vec::new();
        let now = Instant::now();
        for addr in addresses {
            let addr = addr.to_lowercase();
            if let Some(e) = self.cache.get(&addr) {
                if now.duration_since(e.at) < CACHE_TTL {
                    out.insert(addr.clone(), e.name.clone());
                    continue;
                }
            }
            misses.push(addr);
        }

        if misses.is_empty() {
            return out;
        }

        for addr in &misses {
            out.entry(addr.clone()).or_default();
        }

        let Some(pool) = self.pool.as_ref() else {

            for addr in &misses {
                self.cache.insert(
                    addr.clone(),
                    CacheEntry {
                        name: String::new(),
                        at: now,
                    },
                );
            }
            return out;
        };

        let sql = format!(
            "SELECT DISTINCT ON (n.owner_address) n.owner_address, e.subdomain \
             FROM {schema}.nft n \
             JOIN {schema}.ens e ON e.id = n.ens_id \
             WHERE n.category = 'ens' \
               AND e.subdomain IS NOT NULL \
               AND n.owner_address = ANY($1) \
             ORDER BY n.owner_address, e.created_at DESC NULLS LAST",
            schema = self.schema
        );

        match sqlx::query_as::<_, (String, String)>(&sql)
            .bind(&misses)
            .fetch_all(pool)
            .await
        {
            Ok(rows) => {
                for (owner, subdomain) in rows {
                    out.insert(owner.to_lowercase(), subdomain);
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "names batch resolve failed; falling back to empty names");
            }
        }

        for addr in &misses {
            let name = out.get(addr).cloned().unwrap_or_default();
            self.cache.insert(addr.clone(), CacheEntry { name, at: now });
        }

        out
    }
}
