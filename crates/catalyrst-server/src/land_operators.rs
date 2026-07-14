use async_trait::async_trait;

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
