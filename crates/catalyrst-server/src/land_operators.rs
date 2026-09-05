use std::sync::Arc;

use async_trait::async_trait;
use sqlx::PgPool;

use catalyrst_land_authz::LandAuthzStore;
use catalyrst_validator::squid_checker::{LandOperatorResolver, LandOperators};

use crate::handlers::external_graph::parcel_operators;

pub struct SubgraphLandOperatorResolver {
    eth_network: String,
}

impl SubgraphLandOperatorResolver {
    pub fn new(eth_network: impl Into<String>) -> Self {
        Self {
            eth_network: eth_network.into(),
        }
    }
}

#[async_trait]
impl LandOperatorResolver for SubgraphLandOperatorResolver {
    async fn operators(&self, x: i32, y: i32) -> Result<Option<LandOperators>, String> {
        let resolved = parcel_operators(&self.eth_network, x as i64, y as i64).await?;
        Ok(resolved.map(|ops| LandOperators {
            operator: ops.operator,
            update_operator: ops.update_operator,
            update_managers: ops.update_managers,
            approved_for_all: ops.approved_for_all,
        }))
    }
}

/// True once the local authorization index exists in the squid database. The
/// answer only ever goes from absent to present, so it is worth latching.
pub async fn local_index_present(pool: &PgPool) -> bool {
    use std::sync::atomic::{AtomicBool, Ordering};
    static PRESENT: AtomicBool = AtomicBool::new(false);
    if PRESENT.load(Ordering::Relaxed) {
        return true;
    }
    let present: bool = sqlx::query_scalar!(
        r#"SELECT to_regclass('land_authz.token_right') IS NOT NULL AS "present!""#
    )
    .fetch_one(pool)
    .await
    .unwrap_or(false);
    if present {
        PRESENT.store(true, Ordering::Relaxed);
    }
    present
}

/// Prefers the local index and falls back to the land-manager subgraph only
/// where the index has not been built. Both answer the same four legs, so the
/// choice changes where an answer comes from, never what counts as a right.
pub async fn resolver_for(pool: &PgPool, eth_network: &str) -> Arc<dyn LandOperatorResolver> {
    if local_index_present(pool).await {
        return Arc::new(LandAuthzStore::new(pool.clone()));
    }
    tracing::warn!(
        "land_authz index absent; LAND operator legs fall back to the remote land-manager subgraph"
    );
    Arc::new(SubgraphLandOperatorResolver::new(eth_network.to_string()))
}
