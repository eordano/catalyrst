use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use catalyrst_commons::cache::TtlMap;
use sqlx::PgPool;

const CACHE_TTL: Duration = Duration::from_secs(300);

#[derive(Clone)]
pub struct NamesComponent {
    pool: Option<PgPool>,
    schema: String,
    cache: Arc<TtlMap<String, String>>,
}

impl NamesComponent {
    pub fn new(pool: Option<PgPool>, schema: String) -> Self {
        Self {
            pool,
            schema,
            cache: Arc::new(TtlMap::new("comms-names", CACHE_TTL)),
        }
    }

    pub async fn get_names_from_addresses(&self, addresses: &[String]) -> HashMap<String, String> {
        let mut out: HashMap<String, String> = HashMap::new();
        if addresses.is_empty() {
            return out;
        }

        let mut misses: Vec<String> = Vec::new();
        for addr in addresses {
            let addr = addr.to_lowercase();
            if let Some(name) = self.cache.get_fresh(&addr) {
                out.insert(addr, name);
                continue;
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
                self.cache.insert(addr.clone(), String::new());
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

        match sqlx::query_as::<_, (String, String)>(sqlx::AssertSqlSafe(sql))
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
            self.cache.insert(addr.clone(), name);
        }

        out
    }
}
