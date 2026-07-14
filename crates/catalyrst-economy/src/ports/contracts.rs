use std::time::Duration;

use catalyrst_commons::cache::TtlCell;
use serde_json::Value;
use sqlx::PgPool;

use crate::http::errors::ApiError;

pub struct ContractsComponent {
    pool: PgPool,
    squid_schema: String,
    http: reqwest::Client,
    addresses_url: String,
    chain_key: String,
    ttl: Duration,
    cache: TtlCell<Vec<String>>,
}

fn whitelist_accept_header() -> (&'static str, &'static str) {
    ("accept", "application/json")
}

impl ContractsComponent {
    pub fn new(
        pool: PgPool,
        squid_schema: String,
        addresses_url: String,
        chain_key: String,
        ttl: Duration,
    ) -> Self {
        Self {
            pool,
            squid_schema,
            http: reqwest::Client::new(),
            addresses_url,
            chain_key,
            ttl,
            cache: TtlCell::new("economy-address-whitelist"),
        }
    }

    pub async fn is_valid_address(&self, address: &str) -> Result<bool, ApiError> {
        let addr = address.to_lowercase();
        if self.is_collection_address(&addr).await? {
            return Ok(true);
        }
        self.is_whitelisted(&addr).await
    }

    pub async fn is_collection_address(&self, address: &str) -> Result<bool, ApiError> {
        let addr = address.to_lowercase();
        let sql = format!(
            "SELECT 1 FROM {}.collection WHERE id = $1 LIMIT 1",
            self.squid_schema
        );
        let found = sqlx::query_scalar::<_, i32>(sqlx::AssertSqlSafe(sql))
            .bind(&addr)
            .fetch_optional(&self.pool)
            .await?;
        Ok(found.is_some())
    }

    pub async fn is_whitelisted(&self, address: &str) -> Result<bool, ApiError> {
        let addr = address.to_lowercase();
        let fresh = self.cache.get(self.ttl).await.filter(|a| !a.is_empty());
        let addresses = match fresh {
            Some(addresses) => addresses,
            None => match self.fetch_whitelist().await {
                Ok(addresses) => {
                    self.cache.set(addresses.clone()).await;
                    addresses
                }
                Err(e) => match self.cache.last().await.filter(|a| !a.is_empty()) {
                    Some(stale) => {
                        tracing::warn!(error = %e, "addresses.json refresh failed, serving stale cache");
                        stale
                    }
                    None => return Err(e),
                },
            },
        };

        Ok(addresses.iter().any(|a| a == &addr))
    }

    async fn fetch_whitelist(&self) -> Result<Vec<String>, ApiError> {
        let (accept_name, accept_value) = whitelist_accept_header();
        let resp = self
            .http
            .get(&self.addresses_url)
            .header(accept_name, accept_value)
            .send()
            .await
            .map_err(|e| ApiError::Internal(format!("whitelist fetch failed: {e}")))?;
        if !resp.status().is_success() {
            return Err(ApiError::Internal(format!(
                "Could not get the whitelisted addresses from {}",
                self.addresses_url
            )));
        }
        let json: Value = resp
            .json()
            .await
            .map_err(|e| ApiError::Internal(format!("whitelist decode failed: {e}")))?;

        let map = json
            .get(self.chain_key.to_lowercase())
            .and_then(|v| v.as_object());

        Ok(map
            .map(|obj| {
                obj.values()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_lowercase())
                    .collect()
            })
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitelist_get_uses_accept_not_content_type() {
        let (name, value) = whitelist_accept_header();
        assert_eq!(name, "accept");
        assert_ne!(name, "content-type");
        assert_eq!(value, "application/json");
    }
}
