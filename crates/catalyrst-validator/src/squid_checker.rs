use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use async_trait::async_trait;
use sqlx::PgPool;
use tokio::sync::OnceCell;
use tracing::warn;

use crate::checker::{BlockchainChecker, BlockchainLayer};
use crate::error::{PermissionResult, ValidatorError};
use crate::land_rights::ParcelPermissionFlags;
use crate::types::*;

const DECENTRALAND_ADDRESS: &str = "0x1337e0507eb4ab47e08a179573ed4533d9e22a7b";

#[derive(Debug, Clone, Default)]
pub struct LandOperators {
    pub operator: Option<String>,
    pub update_operator: Option<String>,
    pub update_managers: Vec<String>,
    pub approved_for_all: Vec<String>,
}

/// The squid schema indexes LAND/estate ownership only and has no authorization
/// entity, so the operator, update-operator, update-manager and approved-for-all
/// legs must come from whatever indexer a deployment configures; with none
/// configured those legs are denied, never assumed.
#[async_trait]
pub trait LandOperatorResolver: Send + Sync {
    async fn operators(&self, x: i32, y: i32) -> Result<Option<LandOperators>, String>;
}

pub struct SquidBlockchainChecker {
    pool: PgPool,
    additional_decentraland_address: Option<String>,
    tp_subgraph: Option<crate::tp_subgraph::TpSubgraph>,
    tp_root_via_squid: bool,
    operator_resolver: Option<Arc<dyn LandOperatorResolver>>,
}

impl SquidBlockchainChecker {
    pub fn new(pool: PgPool, additional_decentraland_address: Option<String>) -> Self {
        Self::with_third_party(pool, additional_decentraland_address, None, false)
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
            operator_resolver: None,
        }
    }

    pub fn with_operator_resolver(mut self, resolver: Arc<dyn LandOperatorResolver>) -> Self {
        self.operator_resolver = Some(resolver);
        self
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
        .map_err(query_failed("third-party root query failed"))?;

        Ok(root
            .flatten()
            .and_then(|s| crate::merkle::decode_hash32(&s)))
    }

    async fn owned_urns(
        &self,
        address: &str,
        urns: &[String],
    ) -> Result<Vec<bool>, ValidatorError> {
        nft_ownership_batch(&SquidNftSource::new(&self.pool).await, address, urns).await
    }
}

fn query_failed(what: &'static str) -> impl FnOnce(sqlx::Error) -> ValidatorError {
    move |e| ValidatorError::BlockchainQuery(format!("{what}: {e}"))
}

fn permission(failing: Vec<String>) -> PermissionResult {
    if failing.is_empty() {
        PermissionResult::ok()
    } else {
        PermissionResult::denied(failing)
    }
}

fn address_matches_account_id(address: &str, account_id: &str) -> bool {
    account_id
        .split('-')
        .next()
        .is_some_and(|owner| owner.eq_ignore_ascii_case(address))
}

fn addresses_match(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

fn address_in_list(address: &str, list: &[String]) -> bool {
    list.iter().any(|a| addresses_match(address, a))
}

pub fn operator_flags(address: &str, operators: &LandOperators) -> ParcelPermissionFlags {
    let matches = |o: &Option<String>| o.as_deref().is_some_and(|v| addresses_match(address, v));
    ParcelPermissionFlags {
        owner: false,
        operator: matches(&operators.operator),
        update_operator: matches(&operators.update_operator),
        update_manager: address_in_list(address, &operators.update_managers),
        approved_for_all: address_in_list(address, &operators.approved_for_all),
    }
}

pub fn operator_grants(address: &str, operators: &LandOperators) -> bool {
    operator_flags(address, operators).grants_deploy()
}

#[derive(Debug, Clone, Default)]
pub struct ParcelOwnership {
    pub parcel_owner: Option<String>,
    pub estate_owner: Option<String>,
}

impl ParcelOwnership {
    pub fn owned_by(&self, address: &str) -> bool {
        [&self.parcel_owner, &self.estate_owner]
            .into_iter()
            .flatten()
            .any(|owner| address_matches_account_id(address, owner))
    }
}

/// `None` means the squid has no parcel at those coordinates at all, which is
/// a different answer from "indexed but you hold no rights on it".
pub async fn parcel_ownership(
    pool: &PgPool,
    x: i32,
    y: i32,
) -> Result<Option<ParcelOwnership>, ValidatorError> {
    let row: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT p.owner_id, e.owner_id \
         FROM squid_marketplace.parcel p \
         LEFT JOIN squid_marketplace.estate e ON e.id = p.estate_id \
         WHERE p.x = $1 AND p.y = $2",
    )
    .bind(x)
    .bind(y)
    .fetch_optional(pool)
    .await
    .map_err(query_failed("parcel query failed"))?;

    Ok(row.map(|(parcel_owner, estate_owner)| ParcelOwnership {
        parcel_owner,
        estate_owner,
    }))
}

/// Coordinates must be paired via a composite join on the SAME `unnest` tuple
/// (`ON p.x = t.x AND p.y = t.y`) -- never `x = ANY AND y = ANY`, which would
/// cross-match coordinates from different requested pairs and could grant an
/// unrequested parcel. A pair absent from the map means no parcel indexed.
#[async_trait]
trait ParcelOwnerSource {
    async fn ownership_for(
        &self,
        parcels: &[(i32, i32)],
    ) -> Result<HashMap<(i32, i32), ParcelOwnership>, ValidatorError>;
}

struct SquidParcelSource<'a> {
    pool: &'a PgPool,
}

#[async_trait]
impl ParcelOwnerSource for SquidParcelSource<'_> {
    async fn ownership_for(
        &self,
        parcels: &[(i32, i32)],
    ) -> Result<HashMap<(i32, i32), ParcelOwnership>, ValidatorError> {
        let xs: Vec<i32> = parcels.iter().map(|p| p.0).collect();
        let ys: Vec<i32> = parcels.iter().map(|p| p.1).collect();
        let rows: Vec<(i32, i32, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT t.x, t.y, p.owner_id, e.owner_id \
             FROM unnest($1::int[], $2::int[]) AS t(x, y) \
             JOIN squid_marketplace.parcel p ON p.x = t.x AND p.y = t.y \
             LEFT JOIN squid_marketplace.estate e ON e.id = p.estate_id",
        )
        .bind(&xs)
        .bind(&ys)
        .fetch_all(self.pool)
        .await
        .map_err(query_failed("parcel query failed"))?;

        Ok(rows
            .into_iter()
            .map(|(x, y, parcel_owner, estate_owner)| {
                (
                    (x, y),
                    ParcelOwnership {
                        parcel_owner,
                        estate_owner,
                    },
                )
            })
            .collect())
    }
}

/// Owned (parcel or estate owner matches) => allowed; otherwise the operator
/// legs and their fail-closed default decide. Results are positional by input
/// index; the operator legs run sequentially, only for not-owned parcels and
/// only when a resolver is configured.
async fn land_access_batch<S: ParcelOwnerSource + ?Sized>(
    src: &S,
    operator_resolver: Option<&dyn LandOperatorResolver>,
    address: &str,
    parcels: &[(i32, i32)],
) -> Result<Vec<bool>, ValidatorError> {
    let map = src.ownership_for(parcels).await?;
    let mut results = Vec::with_capacity(parcels.len());
    for &(x, y) in parcels {
        let allowed = map.get(&(x, y)).is_some_and(|o| o.owned_by(address))
            || operator_legs(operator_resolver, address, x, y)
                .await
                .grants_deploy();
        results.push(allowed);
    }
    Ok(results)
}

async fn operator_legs(
    operator_resolver: Option<&dyn LandOperatorResolver>,
    address: &str,
    x: i32,
    y: i32,
) -> ParcelPermissionFlags {
    let Some(resolver) = operator_resolver else {
        return ParcelPermissionFlags::default();
    };
    match resolver.operators(x, y).await {
        Ok(Some(operators)) => operator_flags(address, &operators),
        Ok(None) => ParcelPermissionFlags::default(),
        Err(e) => {
            warn!(x, y, error = %e, "land operator resolver failed; denying operator legs (fail-closed)");
            ParcelPermissionFlags::default()
        }
    }
}

async fn parcel_flags(
    ownership: Option<&ParcelOwnership>,
    operator_resolver: Option<&dyn LandOperatorResolver>,
    address: &str,
    x: i32,
    y: i32,
) -> Option<ParcelPermissionFlags> {
    let ownership = ownership?;
    let mut flags = operator_legs(operator_resolver, address, x, y).await;
    flags.owner = ownership.owned_by(address);
    Some(flags)
}

/// Owner and estate owner come from the local squid, the operator legs from the
/// resolver. Both the deploy predicate and the lambdas permissions route answer
/// from this, so what the route reports is what a deploy will be allowed to do.
pub async fn parcel_permission_flags(
    pool: &PgPool,
    operator_resolver: Option<&dyn LandOperatorResolver>,
    address: &str,
    x: i32,
    y: i32,
) -> Result<Option<ParcelPermissionFlags>, ValidatorError> {
    let ownership = parcel_ownership(pool, x, y).await?;
    Ok(parcel_flags(ownership.as_ref(), operator_resolver, address, x, y).await)
}

/// Results are positional by input index; `None` per parcel means "no parcel
/// indexed at all". A resolver outage denies only that parcel's operator legs
/// (fail-closed), never the locally-settled owner leg.
pub async fn parcel_permission_flags_batch(
    pool: &PgPool,
    operator_resolver: Option<&dyn LandOperatorResolver>,
    address: &str,
    parcels: &[(i32, i32)],
) -> Result<Vec<Option<ParcelPermissionFlags>>, ValidatorError> {
    let src = SquidParcelSource { pool };
    permission_flags_batch(&src, operator_resolver, address, parcels).await
}

async fn permission_flags_batch<S: ParcelOwnerSource + ?Sized>(
    src: &S,
    operator_resolver: Option<&dyn LandOperatorResolver>,
    address: &str,
    parcels: &[(i32, i32)],
) -> Result<Vec<Option<ParcelPermissionFlags>>, ValidatorError> {
    let map = src.ownership_for(parcels).await?;
    let mut results = Vec::with_capacity(parcels.len());
    for &(x, y) in parcels {
        results.push(parcel_flags(map.get(&(x, y)), operator_resolver, address, x, y).await);
    }
    Ok(results)
}

pub async fn check_parcel_access(
    pool: &PgPool,
    operator_resolver: Option<&dyn LandOperatorResolver>,
    address: &str,
    x: i32,
    y: i32,
) -> Result<bool, ValidatorError> {
    let owned = parcel_ownership(pool, x, y)
        .await?
        .is_some_and(|o| o.owned_by(address));
    Ok(owned
        || operator_legs(operator_resolver, address, x, y)
            .await
            .grants_deploy())
}

/// A subdomain absent from the returned map is unowned/unindexed.
#[async_trait]
trait NameOwnerSource {
    async fn owners_for(
        &self,
        names: &[String],
    ) -> Result<HashMap<String, Vec<String>>, ValidatorError>;
}

struct SquidNameSource<'a> {
    pool: &'a PgPool,
}

#[async_trait]
impl NameOwnerSource for SquidNameSource<'_> {
    async fn owners_for(
        &self,
        names: &[String],
    ) -> Result<HashMap<String, Vec<String>>, ValidatorError> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT e.subdomain, n.owner_id FROM squid_marketplace.nft n
             JOIN squid_marketplace.ens e ON n.ens_id = e.id
             WHERE n.category = 'ens' AND e.subdomain = ANY($1)",
        )
        .bind(names)
        .fetch_all(self.pool)
        .await
        .map_err(query_failed("ENS query failed"))?;

        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        for (subdomain, owner) in rows {
            map.entry(subdomain).or_default().push(owner);
        }
        Ok(map)
    }
}

/// Returns the failing (unowned) names in input order; absence => unowned. When
/// a subdomain maps to multiple nft rows (anomalous; ens_id is unique in
/// practice) any owner matching wins.
async fn names_ownership_batch<S: NameOwnerSource + ?Sized>(
    src: &S,
    address: &str,
    names: &[String],
) -> Result<Vec<String>, ValidatorError> {
    let map = src.owners_for(names).await?;
    Ok(names
        .iter()
        .filter(|name| {
            !map.get(*name).is_some_and(|owners| {
                owners
                    .iter()
                    .any(|o| address_matches_account_id(address, o))
            })
        })
        .cloned()
        .collect())
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

#[derive(Debug, sqlx::FromRow)]
struct ItemAccessRow {
    creator: String,
    managers: Vec<String>,
    minters: Vec<String>,
}

async fn check_collection_access_query(
    pool: &PgPool,
    address: &str,
    contract_address: &str,
    _layer: BlockchainLayer,
) -> Result<bool, ValidatorError> {
    let fetch = |sql: &'static str, what: &'static str| async move {
        sqlx::query_as::<_, CollectionRow>(sql)
            .bind(contract_address)
            .fetch_optional(pool)
            .await
            .map_err(query_failed(what))
    };
    let mut row = fetch(
        "SELECT creator, owner, managers, minters, is_approved, is_completed \
         FROM squid_marketplace.collection WHERE id = $1",
        "collection query failed",
    )
    .await?;
    if row.is_none() {
        row = fetch(
            "SELECT creator, owner, managers, minters, is_approved, is_completed \
             FROM squid_marketplace.collection WHERE lower(id) = lower($1)",
            "collection query (ci) failed",
        )
        .await?;
    }
    let Some(row) = row else {
        return Ok(false);
    };

    if addresses_match(address, &row.creator)
        || addresses_match(address, &row.owner)
        || address_in_list(address, &row.managers)
        || address_in_list(address, &row.minters)
    {
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
    .map_err(query_failed("item query failed"))?;

    Ok(item_row.is_some_and(|item| {
        addresses_match(address, &item.creator)
            || address_in_list(address, &item.managers)
            || address_in_list(address, &item.minters)
    }))
}

/// Caches both outcomes for the process lifetime but never a transient failure:
/// on `Err(())` the cell is left unset so a later call retries, and the
/// fail-closed `false` is returned without being pinned. Permanent caching is
/// correct because only a schema migration can change the answer, and that
/// ships with a process restart.
async fn cached_bool<F, Fut>(cell: &OnceCell<bool>, probe: F) -> bool
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<bool, ()>>,
{
    cell.get_or_try_init(probe).await.copied().unwrap_or(false)
}

async fn usage_grants_present(pool: &PgPool) -> bool {
    static PRESENT: OnceCell<bool> = OnceCell::const_new();
    cached_bool(&PRESENT, || async {
        sqlx::query_scalar("SELECT to_regclass('marketplace.usage_grants') IS NOT NULL")
            .fetch_one(pool)
            .await
            .map_err(|_| ())
    })
    .await
}

/// Three ownership tiers, each a single batched round trip over the URNs still
/// unresolved after the prior tier. Every method returns the input indices it
/// resolved to `true`.
#[async_trait]
trait NftBatchSource {
    /// `items` are `(index, item_urn, token_id)`.
    async fn tier_token(
        &self,
        address: &str,
        items: &[(usize, String, String)],
    ) -> Result<Vec<usize>, ValidatorError>;
    /// `items` are `(index, urn)`.
    async fn tier_exact(
        &self,
        address: &str,
        items: &[(usize, String)],
    ) -> Result<Vec<usize>, ValidatorError>;
    /// `items` are `(index, urn)`; the `{urn}:%` LIKE pattern is built (with
    /// escaping) inside the implementation.
    async fn tier_prefix(
        &self,
        address: &str,
        items: &[(usize, String)],
    ) -> Result<Vec<usize>, ValidatorError>;
}

struct SquidNftSource<'a> {
    pool: &'a PgPool,
    overlay: bool,
}

impl<'a> SquidNftSource<'a> {
    async fn new(pool: &'a PgPool) -> Self {
        let overlay = usage_grants_present(pool).await;
        Self { pool, overlay }
    }
}

fn indices(rows: Vec<(i32,)>) -> Vec<usize> {
    rows.into_iter().map(|(i,)| i as usize).collect()
}

fn columns(items: &[(usize, String)], f: impl Fn(&str) -> String) -> (Vec<i32>, Vec<String>) {
    items.iter().map(|(i, s)| (*i as i32, f(s))).unzip()
}

#[async_trait]
impl NftBatchSource for SquidNftSource<'_> {
    async fn tier_token(
        &self,
        address: &str,
        items: &[(usize, String, String)],
    ) -> Result<Vec<usize>, ValidatorError> {
        let idx: Vec<i32> = items.iter().map(|(i, ..)| *i as i32).collect();
        let item_urns: Vec<String> = items.iter().map(|(_, u, _)| u.clone()).collect();
        let token_ids: Vec<String> = items.iter().map(|(.., t)| t.clone()).collect();
        let sql = if self.overlay {
            "SELECT t.idx \
             FROM unnest($1::int[], $2::text[], $3::text[]) AS t(idx, item_urn, token_id) \
             WHERE EXISTS (SELECT 1 FROM squid_marketplace.nft \
                           WHERE urn = t.item_urn AND token_id = t.token_id::numeric \
                             AND owner_address = lower($4)) \
                OR EXISTS (SELECT 1 FROM marketplace.usage_grants ug \
                           WHERE ug.status = 'active' AND ug.grantee_address = lower($4) \
                             AND ug.urn = t.item_urn AND ug.token_id = t.token_id)"
        } else {
            "SELECT t.idx \
             FROM unnest($1::int[], $2::text[], $3::text[]) AS t(idx, item_urn, token_id) \
             WHERE EXISTS (SELECT 1 FROM squid_marketplace.nft \
                           WHERE urn = t.item_urn AND token_id = t.token_id::numeric \
                             AND owner_address = lower($4))"
        };
        let rows: Vec<(i32,)> = sqlx::query_as(sql)
            .bind(&idx)
            .bind(&item_urns)
            .bind(&token_ids)
            .bind(address)
            .fetch_all(self.pool)
            .await
            .map_err(query_failed("nft token ownership query failed"))?;
        Ok(indices(rows))
    }

    async fn tier_exact(
        &self,
        address: &str,
        items: &[(usize, String)],
    ) -> Result<Vec<usize>, ValidatorError> {
        let (idx, urns) = columns(items, str::to_string);
        let sql = if self.overlay {
            "SELECT t.idx FROM unnest($1::int[], $2::text[]) AS t(idx, urn) \
             WHERE EXISTS (SELECT 1 FROM squid_marketplace.nft \
                           WHERE urn = t.urn AND owner_address = lower($3)) \
                OR EXISTS (SELECT 1 FROM marketplace.usage_grants ug \
                           WHERE ug.status = 'active' AND ug.grantee_address = lower($3) \
                             AND ug.urn = t.urn)"
        } else {
            "SELECT t.idx FROM unnest($1::int[], $2::text[]) AS t(idx, urn) \
             WHERE EXISTS (SELECT 1 FROM squid_marketplace.nft \
                           WHERE urn = t.urn AND owner_address = lower($3))"
        };
        let rows: Vec<(i32,)> = sqlx::query_as(sql)
            .bind(&idx)
            .bind(&urns)
            .bind(address)
            .fetch_all(self.pool)
            .await
            .map_err(query_failed("nft ownership query failed"))?;
        Ok(indices(rows))
    }

    async fn tier_prefix(
        &self,
        address: &str,
        items: &[(usize, String)],
    ) -> Result<Vec<usize>, ValidatorError> {
        let (idx, pats) = columns(items, |urn| {
            let esc = urn
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            format!("{esc}:%")
        });
        let sql = if self.overlay {
            "SELECT t.idx FROM unnest($1::int[], $2::text[]) AS t(idx, pat) \
             WHERE EXISTS (SELECT 1 FROM squid_marketplace.nft \
                           WHERE urn LIKE t.pat ESCAPE '\\' AND owner_address = lower($3)) \
                OR EXISTS (SELECT 1 FROM marketplace.usage_grants ug \
                           WHERE ug.status = 'active' AND ug.grantee_address = lower($3) \
                             AND ug.urn LIKE t.pat ESCAPE '\\')"
        } else {
            "SELECT t.idx FROM unnest($1::int[], $2::text[]) AS t(idx, pat) \
             WHERE EXISTS (SELECT 1 FROM squid_marketplace.nft \
                           WHERE urn LIKE t.pat ESCAPE '\\' AND owner_address = lower($3))"
        };
        let rows: Vec<(i32,)> = sqlx::query_as(sql)
            .bind(&idx)
            .bind(&pats)
            .bind(address)
            .fetch_all(self.pool)
            .await
            .map_err(query_failed("nft prefix query failed"))?;
        Ok(indices(rows))
    }
}

/// Positional `owns`-per-URN. A URN reaches tier 2 only if tier 1 did not
/// resolve it, and tier 3 only if tier 2 did not, so at most 3 round trips run.
/// Tier 1 eligibility is a 7-part `:collections-` URN with an all-digit token
/// id; non-eligible URNs fall straight to tier 2 carrying their FULL original
/// URN, never a truncated one.
async fn nft_ownership_batch<S: NftBatchSource + ?Sized>(
    src: &S,
    address: &str,
    urns: &[String],
) -> Result<Vec<bool>, ValidatorError> {
    let mut resolved = vec![false; urns.len()];

    let tier1: Vec<(usize, String, String)> = urns
        .iter()
        .enumerate()
        .filter_map(|(i, urn)| {
            let parts: Vec<&str> = urn.split(':').collect();
            let eligible = parts.len() == 7
                && urn.contains(":collections-")
                && parts[6].chars().all(|c| c.is_ascii_digit());
            eligible.then(|| (i, parts[..6].join(":"), parts[6].to_string()))
        })
        .collect();
    if !tier1.is_empty() {
        for i in src.tier_token(address, &tier1).await? {
            resolved[i] = true;
        }
    }

    let pending = |resolved: &[bool]| -> Vec<(usize, String)> {
        urns.iter()
            .enumerate()
            .filter(|(i, _)| !resolved[*i])
            .map(|(i, urn)| (i, urn.clone()))
            .collect()
    };
    let tier2 = pending(&resolved);
    if !tier2.is_empty() {
        for i in src.tier_exact(address, &tier2).await? {
            resolved[i] = true;
        }
    }

    let tier3 = pending(&resolved);
    if !tier3.is_empty() {
        for i in src.tier_prefix(address, &tier3).await? {
            resolved[i] = true;
        }
    }

    Ok(resolved)
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
        let src = SquidParcelSource { pool: &self.pool };
        land_access_batch(
            &src,
            self.operator_resolver.as_deref(),
            eth_address,
            parcels,
        )
        .await
    }

    async fn check_names_ownership(
        &self,
        eth_address: &str,
        names: &[String],
        _timestamp: Timestamp,
    ) -> Result<PermissionResult, ValidatorError> {
        let src = SquidNameSource { pool: &self.pool };
        Ok(permission(
            names_ownership_batch(&src, eth_address, names).await?,
        ))
    }

    async fn check_items_ownership(
        &self,
        eth_address: &str,
        urns: &[String],
        _timestamp: Timestamp,
    ) -> Result<PermissionResult, ValidatorError> {
        let owned = self.owned_urns(eth_address, urns).await?;
        Ok(permission(
            urns.iter()
                .zip(owned)
                .filter(|(_, o)| !o)
                .map(|(u, _)| u.clone())
                .collect(),
        ))
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

        let Some(metadata) = &entity.metadata else {
            return Ok(false);
        };
        let tp_props: crate::third_party::ThirdPartyProps =
            match serde_json::from_value(metadata.clone()) {
                Ok(p) => p,
                Err(e) => {
                    warn!(asset_urn, error = %e, "could not parse third-party metadata");
                    return Ok(false);
                }
            };
        let Some(tp_id) = crate::third_party::get_third_party_id(asset_urn) else {
            warn!(asset_urn, "could not derive third-party id from urn");
            return Ok(false);
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
        self.owned_urns(eth_address, item_urns).await
    }

    fn is_address_owned_by_decentraland(&self, address: &str) -> bool {
        let lower = address.to_lowercase();
        lower == DECENTRALAND_ADDRESS
            || self
                .additional_decentraland_address
                .as_deref()
                .is_some_and(|a| a.to_lowercase() == lower)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const OWNER: &str = "0xowner-ETHEREUM";
    const OTHER: &str = "0xother-ETHEREUM";
    const ACCOUNT_ID: &str = "0x959e104e1a4db6317fa58f8295f586e1a978c297-ETHEREUM";

    async fn probe(cell: &OnceCell<bool>, probes: &AtomicUsize, outcome: Result<bool, ()>) -> bool {
        cached_bool(cell, || async {
            probes.fetch_add(1, Ordering::SeqCst);
            outcome
        })
        .await
    }

    #[tokio::test]
    async fn usage_grants_probe_runs_once() {
        let cell = OnceCell::new();
        let probes = AtomicUsize::new(0);
        for _ in 0..10 {
            assert!(!probe(&cell, &probes, Ok(false)).await);
        }
        assert_eq!(
            probes.load(Ordering::SeqCst),
            1,
            "the to_regclass probe must run at most once, negative outcome included"
        );
    }

    #[tokio::test]
    async fn usage_grants_probe_retries_after_transient_error() {
        let cell = OnceCell::new();
        let probes = AtomicUsize::new(0);
        assert!(!probe(&cell, &probes, Err(())).await);
        assert!(probe(&cell, &probes, Ok(true)).await);
        assert!(probe(&cell, &probes, Ok(false)).await);
        assert_eq!(probes.load(Ordering::SeqCst), 2);
    }

    struct CountingNft {
        token: AtomicUsize,
        exact: AtomicUsize,
        prefix: AtomicUsize,
        owned: HashSet<String>,
    }

    impl CountingNft {
        fn new(owned: impl IntoIterator<Item = String>) -> Self {
            Self {
                token: AtomicUsize::new(0),
                exact: AtomicUsize::new(0),
                prefix: AtomicUsize::new(0),
                owned: owned.into_iter().collect(),
            }
        }
        fn total_calls(&self) -> usize {
            self.token.load(Ordering::SeqCst)
                + self.exact.load(Ordering::SeqCst)
                + self.prefix.load(Ordering::SeqCst)
        }
        fn hits<T>(
            &self,
            counter: &AtomicUsize,
            items: &[T],
            key: impl Fn(&T) -> (usize, String),
        ) -> Vec<usize> {
            counter.fetch_add(1, Ordering::SeqCst);
            items
                .iter()
                .map(key)
                .filter(|(_, k)| self.owned.contains(k))
                .map(|(i, _)| i)
                .collect()
        }
    }

    #[async_trait]
    impl NftBatchSource for CountingNft {
        async fn tier_token(
            &self,
            _address: &str,
            items: &[(usize, String, String)],
        ) -> Result<Vec<usize>, ValidatorError> {
            Ok(self.hits(&self.token, items, |(i, u, t)| {
                (*i, format!("token:{u}:{t}"))
            }))
        }
        async fn tier_exact(
            &self,
            _address: &str,
            items: &[(usize, String)],
        ) -> Result<Vec<usize>, ValidatorError> {
            Ok(self.hits(&self.exact, items, |(i, u)| (*i, format!("exact:{u}"))))
        }
        async fn tier_prefix(
            &self,
            _address: &str,
            items: &[(usize, String)],
        ) -> Result<Vec<usize>, ValidatorError> {
            Ok(self.hits(&self.prefix, items, |(i, u)| (*i, format!("prefix:{u}"))))
        }
    }

    #[tokio::test]
    async fn nft_ownership_batch_query_count() {
        let urns: Vec<String> = (0..10)
            .map(|k| format!("urn:decentraland:matic:collections-v2:0xabc{k}:0:{k}"))
            .chain((0..10).map(|k| format!("urn:decentraland:matic:collections-v2:0xdef{k}:{k}")))
            .collect();
        assert_eq!(urns.len(), 20);

        let item0 = "urn:decentraland:matic:collections-v2:0xabc0:0";
        let src = CountingNft::new([
            format!("token:{item0}:0"),
            format!("exact:{}", urns[5]),
            format!("prefix:{}", urns[12]),
        ]);

        let result = nft_ownership_batch(&src, "0xowner", &urns).await.unwrap();

        assert!(
            src.total_calls() <= 3,
            "expected <= 3 tier round trips, got {}",
            src.total_calls()
        );
        assert_eq!(result.len(), 20);
        assert!(result[0], "index 0 owned via tier-1 token");
        assert!(result[5], "index 5 owned via tier-2 exact");
        assert!(result[12], "index 12 owned via tier-3 prefix");
        for (i, owned) in result.iter().enumerate() {
            if ![0usize, 5, 12].contains(&i) {
                assert!(!owned, "index {i} must be unowned");
            }
        }
    }

    struct CountingName {
        calls: AtomicUsize,
        owners: HashMap<String, Vec<String>>,
    }

    #[async_trait]
    impl NameOwnerSource for CountingName {
        async fn owners_for(
            &self,
            names: &[String],
        ) -> Result<HashMap<String, Vec<String>>, ValidatorError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(names
                .iter()
                .filter_map(|n| self.owners.get(n).map(|o| (n.clone(), o.clone())))
                .collect())
        }
    }

    #[tokio::test]
    async fn names_ownership_single_query() {
        let names: Vec<String> = (0..8).map(|k| format!("name{k}")).collect();
        let src = CountingName {
            calls: AtomicUsize::new(0),
            owners: HashMap::from([
                ("name3".to_string(), vec![OWNER.to_string()]),
                ("name6".to_string(), vec!["0xstranger-ETHEREUM".to_string()]),
            ]),
        };

        let failing = names_ownership_batch(&src, "0xowner", &names)
            .await
            .unwrap();

        assert_eq!(src.calls.load(Ordering::SeqCst), 1, "exactly one ENS query");
        assert!(!failing.contains(&"name3".to_string()));
        assert!(failing.contains(&"name6".to_string()));
        assert_eq!(failing.len(), 7);
    }

    struct CountingParcel {
        calls: AtomicUsize,
        owned: HashMap<(i32, i32), ParcelOwnership>,
    }

    fn ownership(parcel_owner: Option<&str>, estate_owner: Option<&str>) -> ParcelOwnership {
        ParcelOwnership {
            parcel_owner: parcel_owner.map(str::to_string),
            estate_owner: estate_owner.map(str::to_string),
        }
    }

    impl CountingParcel {
        fn new(owned: impl IntoIterator<Item = ((i32, i32), ParcelOwnership)>) -> Self {
            Self {
                calls: AtomicUsize::new(0),
                owned: owned.into_iter().collect(),
            }
        }
    }

    #[async_trait]
    impl ParcelOwnerSource for CountingParcel {
        async fn ownership_for(
            &self,
            parcels: &[(i32, i32)],
        ) -> Result<HashMap<(i32, i32), ParcelOwnership>, ValidatorError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(parcels
                .iter()
                .filter_map(|p| self.owned.get(p).map(|o| (*p, o.clone())))
                .collect())
        }
    }

    #[tokio::test]
    async fn land_access_single_parcel_query() {
        let parcels: Vec<(i32, i32)> = (0..25).map(|k| (k, -k)).collect();
        let src = CountingParcel::new([
            ((3, -3), ownership(Some(OWNER), None)),
            ((7, -7), ownership(Some(OTHER), Some(OWNER))),
        ]);

        let result = land_access_batch(&src, None, "0xowner", &parcels)
            .await
            .unwrap();

        assert_eq!(
            src.calls.load(Ordering::SeqCst),
            1,
            "exactly one parcel query"
        );
        assert_eq!(result.len(), 25);
        for (i, (x, y)) in parcels.iter().enumerate() {
            let expected = (*x, *y) == (3, -3) || (*x, *y) == (7, -7);
            assert_eq!(result[i], expected, "parcel ({x},{y})");
        }
    }

    struct FixedResolver(Result<Option<LandOperators>, String>);

    #[async_trait]
    impl LandOperatorResolver for FixedResolver {
        async fn operators(&self, _x: i32, _y: i32) -> Result<Option<LandOperators>, String> {
            self.0.clone()
        }
    }

    #[tokio::test]
    async fn flags_batch_single_query_mixed_verdicts() {
        let parcels = vec![(1, 1), (2, 2), (3, 3), (4, 4)];
        let src = CountingParcel::new([
            ((1, 1), ownership(Some(OWNER), None)),
            ((2, 2), ownership(Some(OTHER), Some(OWNER))),
            ((3, 3), ownership(Some(OTHER), None)),
        ]);

        let result = permission_flags_batch(&src, None, "0xowner", &parcels)
            .await
            .unwrap();

        assert_eq!(
            src.calls.load(Ordering::SeqCst),
            1,
            "exactly one ownership query"
        );
        assert_eq!(result.len(), 4);
        assert!(result[0].unwrap().owner, "(1,1) carries the owner leg");
        assert!(result[1].unwrap().owner, "(2,2) owner leg via the estate");
        assert_eq!(
            result[2].unwrap(),
            ParcelPermissionFlags::default(),
            "(3,3) is someone else's: every leg false"
        );
        assert!(
            result[3].is_none(),
            "(4,4) is unindexed: None, not all-false"
        );
    }

    #[tokio::test]
    async fn flags_batch_resolver_outage_keeps_owner_leg() {
        let parcels = vec![(1, 1), (3, 3)];
        let src = CountingParcel::new([
            ((1, 1), ownership(Some(OWNER), None)),
            ((3, 3), ownership(Some(OTHER), None)),
        ]);
        let broken = FixedResolver(Err("subgraph down".to_string()));

        let result = permission_flags_batch(&src, Some(&broken), "0xowner", &parcels)
            .await
            .unwrap();

        let owned_flags = result[0].expect("indexed parcel");
        assert!(
            owned_flags.owner,
            "an operator outage must never lock out the owner"
        );
        assert!(
            !owned_flags.operator && !owned_flags.update_operator,
            "operator legs deny on outage (fail-closed)"
        );
        assert_eq!(
            result[1].expect("indexed parcel"),
            ParcelPermissionFlags::default(),
            "a non-owner gets no legs when the resolver is down"
        );
    }

    #[tokio::test]
    async fn flags_batch_operator_grant_matches_single_call_shape() {
        let parcels = vec![(3, 3)];
        let src = CountingParcel::new([((3, 3), ownership(Some(OTHER), None))]);
        let granted = FixedResolver(Ok(Some(LandOperators {
            update_operator: Some("0xoperator".to_string()),
            ..Default::default()
        })));

        let result = permission_flags_batch(&src, Some(&granted), "0xoperator", &parcels)
            .await
            .unwrap();

        assert_eq!(
            result[0].expect("indexed parcel"),
            ParcelPermissionFlags {
                update_operator: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn account_id_matching() {
        assert!(address_matches_account_id(
            "0x959e104e1a4db6317fa58f8295f586e1a978c297",
            ACCOUNT_ID
        ));
        assert!(address_matches_account_id(
            "0x959E104E1A4DB6317FA58F8295F586E1A978C297",
            ACCOUNT_ID
        ));
        assert!(!address_matches_account_id("0xdeadbeef", ACCOUNT_ID));
    }

    #[test]
    fn a_truncated_address_never_matches() {
        for prefix in [
            "0x",
            "0x959e",
            "0x959e104e1a4db6317fa58f8295f586e1a978c29",
            "",
        ] {
            assert!(
                !address_matches_account_id(prefix, ACCOUNT_ID),
                "prefix {prefix:?} must not authorize"
            );
        }
    }

    #[tokio::test]
    async fn decentraland_address_check() {
        let checker = SquidBlockchainChecker::new(
            PgPool::connect_lazy("postgres://localhost/test").unwrap(),
            Some("0xextra".to_string()),
        );

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

    #[test]
    fn operator_grants_each_leg() {
        let ops = LandOperators {
            operator: Some("0xAAA1".into()),
            update_operator: Some("0xbbb2".into()),
            update_managers: vec!["0xccc3".into()],
            approved_for_all: vec!["0xDDD4".into()],
        };
        assert!(operator_grants("0xaaa1", &ops));
        assert!(operator_grants("0xBBB2", &ops));
        assert!(operator_grants("0xCCC3", &ops));
        assert!(operator_grants("0xddd4", &ops));
        assert!(!operator_grants("0xeee5", &ops));
    }

    #[test]
    fn operator_grants_denies_on_empty() {
        assert!(!operator_grants("0xaaa1", &LandOperators::default()));
    }
}
