use async_trait::async_trait;
use sqlx::PgPool;
use tracing::warn;

use crate::checker::{BlockchainChecker, BlockchainLayer};
use crate::error::{PermissionResult, ValidatorError};
use crate::types::*;

const DECENTRALAND_ADDRESS: &str = "0x1337e0507eb4ab47e08a179573ed4533d9e22a7b";

pub struct SquidBlockchainChecker {
    pool: PgPool,
    additional_decentraland_address: Option<String>,
    tp_subgraph: Option<crate::tp_subgraph::TpSubgraph>,
    tp_root_via_squid: bool,
}

impl SquidBlockchainChecker {
    pub fn new(pool: PgPool, additional_decentraland_address: Option<String>) -> Self {
        Self {
            pool,
            additional_decentraland_address,
            tp_subgraph: None,
            tp_root_via_squid: false,
        }
    }

    pub fn with_third_party(
        pool: PgPool,
        additional_decentraland_address: Option<String>,
        tp_subgraph: Option<crate::tp_subgraph::TpSubgraph>,
        tp_root_via_squid: bool,
    ) -> Self {
        Self {
            pool,
            additional_decentraland_address,
            tp_subgraph,
            tp_root_via_squid,
        }
    }

    async fn third_party_root_from_squid(
        &self,
        third_party_id: &str,
        block: Option<u64>,
    ) -> Result<Option<[u8; 32]>, ValidatorError> {
        let root: Option<Option<String>> = if let Some(block) = block {
            sqlx::query_scalar(
                r#"
                SELECT root FROM squid_marketplace.third_party_root_change
                WHERE third_party_id = $1 AND is_approved = true AND block <= $2
                ORDER BY block DESC LIMIT 1
                "#,
            )
            .bind(third_party_id)
            .bind(block as i64)
            .fetch_optional(&self.pool)
            .await
        } else {
            sqlx::query_scalar(
                r#"
                SELECT root FROM squid_marketplace.third_party
                WHERE id = $1 AND is_approved = true LIMIT 1
                "#,
            )
            .bind(third_party_id)
            .fetch_optional(&self.pool)
            .await
        }
        .map_err(|e| {
            ValidatorError::BlockchainQuery(format!("third-party root query failed: {e}"))
        })?;

        Ok(root
            .flatten()
            .and_then(|s| crate::merkle::decode_hash32(&s)))
    }
}

fn address_matches_account_id(address: &str, account_id: &str) -> bool {
    account_id
        .to_lowercase()
        .starts_with(&address.to_lowercase())
}

fn addresses_match(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

fn address_in_list(address: &str, list: &[String]) -> bool {
    let lower = address.to_lowercase();
    list.iter().any(|a| a.to_lowercase() == lower)
}

async fn check_parcel_access(
    pool: &PgPool,
    address: &str,
    x: i32,
    y: i32,
) -> Result<bool, ValidatorError> {
    let parcel_owner: Option<String> =
        sqlx::query_scalar("SELECT owner_id FROM squid_marketplace.parcel WHERE x = $1 AND y = $2")
            .bind(x)
            .bind(y)
            .fetch_optional(pool)
            .await
            .map_err(|e| ValidatorError::BlockchainQuery(format!("parcel query failed: {e}")))?;

    if let Some(ref owner_id) = parcel_owner {
        if address_matches_account_id(address, owner_id) {
            return Ok(true);
        }
    }

    let estate_owner: Option<String> = sqlx::query_scalar(
        "SELECT e.owner_id FROM squid_marketplace.parcel p \
         JOIN squid_marketplace.estate e ON e.id = p.estate_id \
         WHERE p.x = $1 AND p.y = $2",
    )
    .bind(x)
    .bind(y)
    .fetch_optional(pool)
    .await
    .map_err(|e| ValidatorError::BlockchainQuery(format!("estate query failed: {e}")))?;

    if let Some(ref owner_id) = estate_owner {
        if address_matches_account_id(address, owner_id) {
            return Ok(true);
        }
    }

    Ok(false)
}

async fn check_name_ownership(
    pool: &PgPool,
    address: &str,
    name: &str,
) -> Result<bool, ValidatorError> {
    let owner: Option<String> =
        sqlx::query_scalar("SELECT owner_id FROM squid_marketplace.ens WHERE subdomain = $1")
            .bind(name)
            .fetch_optional(pool)
            .await
            .map_err(|e| ValidatorError::BlockchainQuery(format!("ENS query failed: {e}")))?;

    Ok(match owner {
        Some(o) => address_matches_account_id(address, &o),
        None => false,
    })
}

#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code)]
struct CollectionRow {
    creator: String,
    owner: String,
    managers: Vec<String>,
    minters: Vec<String>,
    is_approved: Option<bool>,
    is_completed: Option<bool>,
}

async fn check_collection_access_query(
    pool: &PgPool,
    address: &str,
    contract_address: &str,
    _layer: BlockchainLayer,
) -> Result<bool, ValidatorError> {
    let row: Option<CollectionRow> = sqlx::query_as(
        "SELECT creator, owner, managers, minters, is_approved, is_completed \
         FROM squid_marketplace.collection WHERE id = $1",
    )
    .bind(contract_address)
    .fetch_optional(pool)
    .await
    .map_err(|e| ValidatorError::BlockchainQuery(format!("collection query failed: {e}")))?;

    let row = match row {
        Some(r) => r,
        None => {
            let row2: Option<CollectionRow> = sqlx::query_as(
                "SELECT creator, owner, managers, minters, is_approved, is_completed \
                 FROM squid_marketplace.collection WHERE lower(id) = lower($1)",
            )
            .bind(contract_address)
            .fetch_optional(pool)
            .await
            .map_err(|e| {
                ValidatorError::BlockchainQuery(format!("collection query (ci) failed: {e}"))
            })?;

            match row2 {
                Some(r) => r,
                None => return Ok(false),
            }
        }
    };

    if addresses_match(address, &row.creator) {
        return Ok(true);
    }
    if addresses_match(address, &row.owner) {
        return Ok(true);
    }
    if address_in_list(address, &row.managers) {
        return Ok(true);
    }
    if address_in_list(address, &row.minters) {
        return Ok(true);
    }

    let item_row: Option<ItemAccessRow> = sqlx::query_as(
        "SELECT creator, managers, minters \
         FROM squid_marketplace.item \
         WHERE collection_id = $1 OR lower(collection_id) = lower($1) \
         LIMIT 1",
    )
    .bind(contract_address)
    .fetch_optional(pool)
    .await
    .map_err(|e| ValidatorError::BlockchainQuery(format!("item query failed: {e}")))?;

    if let Some(item) = item_row {
        if addresses_match(address, &item.creator) {
            return Ok(true);
        }
        if address_in_list(address, &item.managers) {
            return Ok(true);
        }
        if address_in_list(address, &item.minters) {
            return Ok(true);
        }
    }

    Ok(false)
}

#[derive(Debug, sqlx::FromRow)]
struct ItemAccessRow {
    creator: String,
    managers: Vec<String>,
    minters: Vec<String>,
}

async fn usage_grants_present(pool: &PgPool) -> bool {
    use std::sync::atomic::{AtomicBool, Ordering};
    static PRESENT: AtomicBool = AtomicBool::new(false);
    if PRESENT.load(Ordering::Relaxed) {
        return true;
    }
    let present: bool =
        sqlx::query_scalar("SELECT to_regclass('marketplace.usage_grants') IS NOT NULL")
            .fetch_one(pool)
            .await
            .unwrap_or(false);
    if present {
        PRESENT.store(true, Ordering::Relaxed);
    }
    present
}

async fn check_nft_ownership(
    pool: &PgPool,
    address: &str,
    urn: &str,
) -> Result<bool, ValidatorError> {
    let overlay = usage_grants_present(pool).await;

    let exact_sql = if overlay {
        "SELECT \
             EXISTS (SELECT 1 FROM squid_marketplace.nft \
                     WHERE urn = $1 AND owner_address = lower($2)) \
          OR EXISTS (SELECT 1 FROM marketplace.usage_grants ug \
                     WHERE ug.status = 'active' \
                       AND ug.grantee_address = lower($2) \
                       AND ug.urn = $1)"
    } else {
        "SELECT EXISTS (SELECT 1 FROM squid_marketplace.nft \
                     WHERE urn = $1 AND owner_address = lower($2))"
    };
    let owns_exact: bool = sqlx::query_scalar(exact_sql)
        .bind(urn)
        .bind(address)
        .fetch_one(pool)
        .await
        .map_err(|e| ValidatorError::BlockchainQuery(format!("nft ownership query failed: {e}")))?;

    if owns_exact {
        return Ok(true);
    }

    let prefix_sql = if overlay {
        "SELECT \
             EXISTS (SELECT 1 FROM squid_marketplace.nft \
                     WHERE urn LIKE $1 AND owner_address = lower($2)) \
          OR EXISTS (SELECT 1 FROM marketplace.usage_grants ug \
                     WHERE ug.status = 'active' \
                       AND ug.grantee_address = lower($2) \
                       AND ug.urn LIKE $1)"
    } else {
        "SELECT EXISTS (SELECT 1 FROM squid_marketplace.nft \
                     WHERE urn LIKE $1 AND owner_address = lower($2))"
    };
    let owns_prefix: bool = sqlx::query_scalar(prefix_sql)
        .bind(format!("{urn}:%"))
        .bind(address)
        .fetch_one(pool)
        .await
        .map_err(|e| ValidatorError::BlockchainQuery(format!("nft prefix query failed: {e}")))?;

    Ok(owns_prefix)
}

#[async_trait]
impl BlockchainChecker for SquidBlockchainChecker {
    async fn find_blocks_for_timestamp(
        &self,
        timestamp: Timestamp,
        layer: BlockchainLayer,
    ) -> Result<BlockInformation, ValidatorError> {
        let block_at_deployment = match (layer, &self.tp_subgraph) {
            (BlockchainLayer::L2, Some(tp)) => tp.block_for_timestamp(timestamp).await,
            _ => None,
        };
        Ok(BlockInformation {
            block_at_deployment,
            block_five_min_before: None,
        })
    }

    async fn check_land_access(
        &self,
        eth_address: &str,
        parcels: &[(i32, i32)],
        _timestamp: Timestamp,
    ) -> Result<Vec<bool>, ValidatorError> {
        let mut results = Vec::with_capacity(parcels.len());
        for &(x, y) in parcels {
            let has_access = check_parcel_access(&self.pool, eth_address, x, y).await?;
            results.push(has_access);
        }
        Ok(results)
    }

    async fn check_names_ownership(
        &self,
        eth_address: &str,
        names: &[String],
        _timestamp: Timestamp,
    ) -> Result<PermissionResult, ValidatorError> {
        let mut failing = Vec::new();
        for name in names {
            let owns = check_name_ownership(&self.pool, eth_address, name).await?;
            if !owns {
                failing.push(name.clone());
            }
        }
        if failing.is_empty() {
            Ok(PermissionResult::ok())
        } else {
            Ok(PermissionResult::denied(failing))
        }
    }

    async fn check_items_ownership(
        &self,
        eth_address: &str,
        urns: &[String],
        _timestamp: Timestamp,
    ) -> Result<PermissionResult, ValidatorError> {
        let mut failing = Vec::new();
        for urn in urns {
            let owns = check_nft_ownership(&self.pool, eth_address, urn).await?;
            if !owns {
                failing.push(urn.clone());
            }
        }
        if failing.is_empty() {
            Ok(PermissionResult::ok())
        } else {
            Ok(PermissionResult::denied(failing))
        }
    }

    async fn check_collection_access(
        &self,
        eth_address: &str,
        contract_address: &str,
        _item_id: &str,
        _entity: &Entity,
        _timestamp: Timestamp,
        layer: BlockchainLayer,
    ) -> Result<bool, ValidatorError> {
        check_collection_access_query(&self.pool, eth_address, contract_address, layer).await
    }

    async fn check_third_party_access(
        &self,
        asset_urn: &str,
        entity: &Entity,
        _deployment: &DeploymentToValidate,
        timestamp: Timestamp,
    ) -> Result<bool, ValidatorError> {
        if !self.tp_root_via_squid && self.tp_subgraph.is_none() {
            warn!(
                asset_urn,
                "no third-party root source configured; rejecting (fail-closed)"
            );
            return Ok(false);
        }

        let metadata = match &entity.metadata {
            Some(m) => m,
            None => return Ok(false),
        };
        let tp_props: crate::third_party::ThirdPartyProps =
            match serde_json::from_value(metadata.clone()) {
                Ok(p) => p,
                Err(e) => {
                    warn!(asset_urn, error = %e, "could not parse third-party metadata");
                    return Ok(false);
                }
            };
        let tp_id = match crate::third_party::get_third_party_id(asset_urn) {
            Some(id) => id,
            None => {
                warn!(asset_urn, "could not derive third-party id from urn");
                return Ok(false);
            }
        };

        let block = match &self.tp_subgraph {
            Some(tp) => tp.block_for_timestamp(timestamp).await,
            None => None,
        };

        let root = if self.tp_root_via_squid {
            self.third_party_root_from_squid(&tp_id, block).await?
        } else if let (Some(tp), Some(block)) = (&self.tp_subgraph, block) {
            tp.third_party_root(&tp_id, block).await
        } else {
            warn!(
                asset_urn,
                "could not resolve L2 block for registry-subgraph root lookup"
            );
            None
        };

        let Some(root) = root else {
            warn!(
                tp_id,
                ?block,
                "third-party not approved or root unavailable"
            );
            return Ok(false);
        };

        Ok(crate::third_party::verify_third_party_merkle_proof(
            &tp_props.merkle_proof,
            &root,
        ))
    }

    async fn check_third_party_items(
        &self,
        eth_address: &str,
        item_urns: &[String],
        _block: u64,
    ) -> Result<Vec<bool>, ValidatorError> {
        let mut results = Vec::with_capacity(item_urns.len());
        for urn in item_urns {
            let owns = check_nft_ownership(&self.pool, eth_address, urn).await?;
            results.push(owns);
        }
        Ok(results)
    }

    fn is_address_owned_by_decentraland(&self, address: &str) -> bool {
        let lower = address.to_lowercase();
        if lower == DECENTRALAND_ADDRESS {
            return true;
        }
        if let Some(ref additional) = self.additional_decentraland_address {
            if lower == additional.to_lowercase() {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_id_matching() {
        assert!(address_matches_account_id(
            "0x959e104e1a4db6317fa58f8295f586e1a978c297",
            "0x959e104e1a4db6317fa58f8295f586e1a978c297-ETHEREUM"
        ));
        assert!(address_matches_account_id(
            "0x959E104E1A4DB6317FA58F8295F586E1A978C297",
            "0x959e104e1a4db6317fa58f8295f586e1a978c297-ETHEREUM"
        ));
        assert!(!address_matches_account_id(
            "0xdeadbeef",
            "0x959e104e1a4db6317fa58f8295f586e1a978c297-ETHEREUM"
        ));
    }

    #[tokio::test]
    async fn decentraland_address_check() {
        let checker = SquidBlockchainChecker {
            pool: PgPool::connect_lazy("postgres://localhost/test").unwrap(),
            additional_decentraland_address: Some("0xextra".to_string()),
            tp_subgraph: None,
            tp_root_via_squid: false,
        };

        assert!(checker.is_address_owned_by_decentraland(DECENTRALAND_ADDRESS));
        assert!(
            checker.is_address_owned_by_decentraland("0x1337E0507EB4AB47E08A179573ED4533D9E22A7B")
        );
        assert!(checker.is_address_owned_by_decentraland("0xextra"));
        assert!(!checker.is_address_owned_by_decentraland("0xrandom"));
    }

    #[test]
    fn address_list_membership() {
        let list = vec!["0xabc123".to_string(), "0xDEF456".to_string()];
        assert!(address_in_list("0xABC123", &list));
        assert!(address_in_list("0xdef456", &list));
        assert!(!address_in_list("0x999999", &list));
    }
}
