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

/// The squid schema indexes LAND ownership and estate membership only: its
/// `parcel`/`estate` tables carry `owner_id` and `estate_id` and nothing else,
/// and it has no authorization entity at all. The operator, update-operator,
/// update-manager and approved-for-all legs therefore cannot be answered
/// locally and must come from whatever indexer a deployment configures for
/// them; with none configured those legs are denied, never assumed.
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
        Self {
            pool,
            additional_decentraland_address,
            tp_subgraph: None,
            tp_root_via_squid: false,
            operator_resolver: None,
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
        .map_err(|e| {
            ValidatorError::BlockchainQuery(format!("third-party root query failed: {e}"))
        })?;

        Ok(root
            .flatten()
            .and_then(|s| crate::merkle::decode_hash32(&s)))
    }
}

// `account_id` is `0x<address>-<NETWORK>`; compare only the address segment and
// compare it whole. A prefix test would let a truncated address ("0x") match
// every account.
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
    let lower = address.to_lowercase();
    list.iter().any(|a| a.to_lowercase() == lower)
}

pub fn operator_flags(address: &str, operators: &LandOperators) -> ParcelPermissionFlags {
    let matches = |o: &Option<String>| {
        o.as_deref()
            .map(|v| addresses_match(address, v))
            .unwrap_or(false)
    };
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
    .map_err(|e| ValidatorError::BlockchainQuery(format!("parcel query failed: {e}")))?;

    Ok(row.map(|(parcel_owner, estate_owner)| ParcelOwnership {
        parcel_owner,
        estate_owner,
    }))
}

/// One round trip resolving parcel/estate ownership for a whole parcel set,
/// keyed on the (x,y) pair. Coordinates are paired via a composite join on the
/// SAME `unnest` tuple (`ON p.x = t.x AND p.y = t.y`) -- never `x = ANY AND
/// y = ANY`, which would cross-match coordinates from different requested pairs
/// and could grant an unrequested parcel. A pair absent from the map is exactly
/// what the old per-parcel `fetch_optional -> None` meant ("no parcel indexed").
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
        .map_err(|e| ValidatorError::BlockchainQuery(format!("parcel query failed: {e}")))?;

        let mut map = HashMap::new();
        for (x, y, parcel_owner, estate_owner) in rows {
            map.insert(
                (x, y),
                ParcelOwnership {
                    parcel_owner,
                    estate_owner,
                },
            );
        }
        Ok(map)
    }
}

/// Per-parcel verdict identical to `check_parcel_access`: owned (parcel or
/// estate owner matches) => allowed; otherwise fall to the operator legs and
/// their fail-closed default. Results are positional by input index; duplicate
/// coordinates dedupe harmlessly (same answer per pair). Operator legs stay
/// sequential (the ownership query is the collapsed round trip; the legs run
/// only for not-owned parcels and only when a resolver is configured).
async fn land_access_batch<S: ParcelOwnerSource + ?Sized>(
    src: &S,
    operator_resolver: Option<&dyn LandOperatorResolver>,
    address: &str,
    parcels: &[(i32, i32)],
) -> Result<Vec<bool>, ValidatorError> {
    let map = src.ownership_for(parcels).await?;
    let mut results = Vec::with_capacity(parcels.len());
    for &(x, y) in parcels {
        if map.get(&(x, y)).is_some_and(|o| o.owned_by(address)) {
            results.push(true);
            continue;
        }
        results.push(
            operator_legs(operator_resolver, address, x, y)
                .await
                .grants_deploy(),
        );
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

/// The single reading of who holds which right on a parcel: owner and estate
/// owner from the local squid, the operator legs from the resolver. Both the
/// deploy predicate and the lambdas permissions route answer from this, so
/// what the route reports is what a deploy will actually be allowed to do.
pub async fn parcel_permission_flags(
    pool: &PgPool,
    operator_resolver: Option<&dyn LandOperatorResolver>,
    address: &str,
    x: i32,
    y: i32,
) -> Result<Option<ParcelPermissionFlags>, ValidatorError> {
    let Some(ownership) = parcel_ownership(pool, x, y).await? else {
        return Ok(None);
    };
    let mut flags = operator_legs(operator_resolver, address, x, y).await;
    flags.owner = ownership.owned_by(address);
    Ok(Some(flags))
}

/// Batch analogue of `parcel_permission_flags`: ownership for the whole set
/// resolves in the ONE unnest round trip `land_access_batch` uses, then the
/// operator legs run per parcel exactly as the single call does. Results are
/// positional by input index; `None` per parcel keeps the single call's
/// "no parcel indexed at all" reading. A resolver outage denies only that
/// parcel's operator legs (fail-closed), never the locally-settled owner leg.
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
        let Some(ownership) = map.get(&(x, y)) else {
            results.push(None);
            continue;
        };
        let mut flags = operator_legs(operator_resolver, address, x, y).await;
        flags.owner = ownership.owned_by(address);
        results.push(Some(flags));
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
    let ownership = parcel_ownership(pool, x, y).await?;
    if ownership.is_some_and(|o| o.owned_by(address)) {
        return Ok(true);
    }
    Ok(operator_legs(operator_resolver, address, x, y)
        .await
        .grants_deploy())
}

/// One round trip resolving ENS ownership for a set of claimed names: returns
/// `subdomain -> owner_id(s)`. A subdomain absent from the map is exactly what
/// the old per-name `fetch_optional -> None` meant (unowned/unindexed).
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
        // Ownership comes from the NFT entity, never from `ens.owner_id`: the
        // squid's ENS handler seeds the owner from the registrar *caller* and
        // never updates it, so a DCLControllerV2 registration records the
        // controller contract rather than the buyer. `nft.owner_id` is the
        // ERC-721 owner. `= ANY($1)` is the same per-name equality the old
        // per-name `subdomain = $1` used, batched into one query.
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT e.subdomain, n.owner_id FROM squid_marketplace.nft n
             JOIN squid_marketplace.ens e ON n.ens_id = e.id
             WHERE n.category = 'ens' AND e.subdomain = ANY($1)",
        )
        .bind(names)
        .fetch_all(self.pool)
        .await
        .map_err(|e| ValidatorError::BlockchainQuery(format!("ENS query failed: {e}")))?;

        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        for (subdomain, owner) in rows {
            map.entry(subdomain).or_default().push(owner);
        }
        Ok(map)
    }
}

/// Returns the failing (unowned) names in input order. Per-name verdict is
/// identical to the old `check_name_ownership`: `address_matches_account_id`
/// against the owner; absence => unowned. When a subdomain maps to multiple nft
/// rows (anomalous; ens_id is unique in practice) "any owner matching wins",
/// which is no stricter than the old unordered `fetch_optional`'s arbitrary pick.
async fn names_ownership_batch<S: NameOwnerSource + ?Sized>(
    src: &S,
    address: &str,
    names: &[String],
) -> Result<Vec<String>, ValidatorError> {
    let map = src.owners_for(names).await?;
    let mut failing = Vec::new();
    for name in names {
        let owned = map.get(name).is_some_and(|owners| {
            owners
                .iter()
                .any(|o| address_matches_account_id(address, o))
        });
        if !owned {
            failing.push(name.clone());
        }
    }
    Ok(failing)
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

/// Memoize a boolean probe for the process lifetime, caching BOTH outcomes but
/// never caching a transient failure (probe returns `Err(())`): on error the
/// cell is left unset so a later call retries, and the fail-closed `false`
/// default is returned without being pinned. A successful `Ok(true)`/`Ok(false)`
/// is cached permanently -- correct because the only thing that can change the
/// answer (a schema migration) ships with a process restart that clears the cell.
async fn cached_bool<F, Fut>(cell: &OnceCell<bool>, probe: F) -> bool
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<bool, ()>>,
{
    cell.get_or_try_init(|| async { probe().await })
        .await
        .copied()
        .unwrap_or(false)
}

async fn usage_grants_present(pool: &PgPool) -> bool {
    // Process-lifetime memo: the `marketplace.usage_grants` overlay table's
    // presence is a schema fact that only changes across a redeploy/restart, so
    // the previous code's re-probe on every negative call was pure waste. Both
    // outcomes are now cached after at most one `to_regclass` round trip.
    static PRESENT: OnceCell<bool> = OnceCell::const_new();
    cached_bool(&PRESENT, || async {
        sqlx::query_scalar("SELECT to_regclass('marketplace.usage_grants') IS NOT NULL")
            .fetch_one(pool)
            .await
            .map_err(|_| ())
    })
    .await
}

/// The three ownership tiers, each a single batched round trip over the set of
/// URNs still unresolved after the prior tier. Every method returns the input
/// indices it resolved to `true`; each tier keeps the exact per-URN predicate
/// (including the `OR ... usage_grants` overlay arm and the LIKE ESCAPE) the old
/// per-URN `check_nft_ownership` used, just widened to a list via `unnest`.
#[async_trait]
trait NftBatchSource {
    /// Tier 1 (token-exact): `items` are `(index, item_urn, token_id)`.
    async fn tier_token(
        &self,
        address: &str,
        items: &[(usize, String, String)],
    ) -> Result<Vec<usize>, ValidatorError>;
    /// Tier 2 (urn-exact): `items` are `(index, urn)`.
    async fn tier_exact(
        &self,
        address: &str,
        items: &[(usize, String)],
    ) -> Result<Vec<usize>, ValidatorError>;
    /// Tier 3 (prefix): `items` are `(index, urn)`; the `{urn}:%` LIKE pattern is
    /// built (with the same escaping) inside the implementation.
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
        // Probe the overlay table once for the whole batch (itself memoized for
        // the process lifetime -- see `usage_grants_present`).
        let overlay = usage_grants_present(pool).await;
        Self { pool, overlay }
    }
}

#[async_trait]
impl NftBatchSource for SquidNftSource<'_> {
    async fn tier_token(
        &self,
        address: &str,
        items: &[(usize, String, String)],
    ) -> Result<Vec<usize>, ValidatorError> {
        let mut idx = Vec::with_capacity(items.len());
        let mut item_urns = Vec::with_capacity(items.len());
        let mut token_ids = Vec::with_capacity(items.len());
        for (i, item_urn, token_id) in items {
            idx.push(*i as i32);
            item_urns.push(item_urn.clone());
            token_ids.push(token_id.clone());
        }
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
            .map_err(|e| {
                ValidatorError::BlockchainQuery(format!("nft token ownership query failed: {e}"))
            })?;
        Ok(rows.into_iter().map(|(i,)| i as usize).collect())
    }

    async fn tier_exact(
        &self,
        address: &str,
        items: &[(usize, String)],
    ) -> Result<Vec<usize>, ValidatorError> {
        let mut idx = Vec::with_capacity(items.len());
        let mut urns = Vec::with_capacity(items.len());
        for (i, urn) in items {
            idx.push(*i as i32);
            urns.push(urn.clone());
        }
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
            .map_err(|e| {
                ValidatorError::BlockchainQuery(format!("nft ownership query failed: {e}"))
            })?;
        Ok(rows.into_iter().map(|(i,)| i as usize).collect())
    }

    async fn tier_prefix(
        &self,
        address: &str,
        items: &[(usize, String)],
    ) -> Result<Vec<usize>, ValidatorError> {
        let mut idx = Vec::with_capacity(items.len());
        let mut pats = Vec::with_capacity(items.len());
        for (i, urn) in items {
            idx.push(*i as i32);
            let esc = urn
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            pats.push(format!("{esc}:%"));
        }
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
            .map_err(|e| {
                ValidatorError::BlockchainQuery(format!("nft prefix query failed: {e}"))
            })?;
        Ok(rows.into_iter().map(|(i,)| i as usize).collect())
    }
}

/// Batched ownership check for a list of URNs, returning a positional
/// `owns`-per-URN vector. Escalation reproduces the old per-URN early-return
/// short-circuit exactly: a URN is passed to tier 2 only if tier 1 did not
/// resolve it, and to tier 3 only if tier 2 did not -- so the 3N sequential round
/// trips collapse to at most 3 (one per non-empty tier) with byte-identical
/// per-URN verdicts. Tier 1 eligibility mirrors the old guard exactly (7-part
/// `:collections-` URN with an all-digit token id); non-eligible URNs fall
/// straight to tier 2 carrying their FULL original URN, never a truncated one.
async fn nft_ownership_batch<S: NftBatchSource + ?Sized>(
    src: &S,
    address: &str,
    urns: &[String],
) -> Result<Vec<bool>, ValidatorError> {
    let mut resolved = vec![false; urns.len()];

    // Tier 1: token-exact over the eligible subset.
    let mut tier1: Vec<(usize, String, String)> = Vec::new();
    for (i, urn) in urns.iter().enumerate() {
        let parts: Vec<&str> = urn.split(':').collect();
        if parts.len() == 7 && urn.contains(":collections-") {
            let token_id = parts[6];
            if token_id.chars().all(|c| c.is_ascii_digit()) {
                tier1.push((i, parts[..6].join(":"), token_id.to_string()));
            }
        }
    }
    if !tier1.is_empty() {
        for i in src.tier_token(address, &tier1).await? {
            resolved[i] = true;
        }
    }

    // Tier 2: urn-exact over everything not yet resolved (full original URN).
    let tier2: Vec<(usize, String)> = urns
        .iter()
        .enumerate()
        .filter(|(i, _)| !resolved[*i])
        .map(|(i, urn)| (i, urn.clone()))
        .collect();
    if !tier2.is_empty() {
        for i in src.tier_exact(address, &tier2).await? {
            resolved[i] = true;
        }
    }

    // Tier 3: prefix over whatever remains.
    let tier3: Vec<(usize, String)> = urns
        .iter()
        .enumerate()
        .filter(|(i, _)| !resolved[*i])
        .map(|(i, urn)| (i, urn.clone()))
        .collect();
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
        let failing = names_ownership_batch(&src, eth_address, names).await?;
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
        let src = SquidNftSource::new(&self.pool).await;
        let owned = nft_ownership_batch(&src, eth_address, urns).await?;
        let failing: Vec<String> = urns
            .iter()
            .zip(owned)
            .filter(|(_, o)| !o)
            .map(|(u, _)| u.clone())
            .collect();
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
        let src = SquidNftSource::new(&self.pool).await;
        nft_ownership_batch(&src, eth_address, item_urns).await
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
    use async_trait::async_trait;
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // [Performance] the usage_grants overlay probe runs at most ONCE for the
    // process lifetime, caching BOTH outcomes. Red-check: reverting to a per-call
    // probe makes the counter == N (here 10).
    #[tokio::test]
    async fn usage_grants_probe_runs_once() {
        let cell = OnceCell::new();
        let probes = AtomicUsize::new(0);
        for _ in 0..10 {
            let present = cached_bool(&cell, || async {
                probes.fetch_add(1, Ordering::SeqCst);
                Ok::<bool, ()>(false) // overlay table absent, the common case
            })
            .await;
            assert!(!present);
        }
        assert_eq!(
            probes.load(Ordering::SeqCst),
            1,
            "the to_regclass probe must run at most once, negative outcome included"
        );
    }

    // Correctness guard for the memo: a transient probe error is NOT cached, so a
    // later call retries; once a value is cached, no further probe runs.
    #[tokio::test]
    async fn usage_grants_probe_retries_after_transient_error() {
        let cell = OnceCell::new();
        let probes = AtomicUsize::new(0);

        let v1 = cached_bool(&cell, || async {
            probes.fetch_add(1, Ordering::SeqCst);
            Err::<bool, ()>(())
        })
        .await;
        assert!(!v1); // fail-closed default, not cached

        let v2 = cached_bool(&cell, || async {
            probes.fetch_add(1, Ordering::SeqCst);
            Ok::<bool, ()>(true)
        })
        .await;
        assert!(v2); // success cached

        let v3 = cached_bool(&cell, || async {
            probes.fetch_add(1, Ordering::SeqCst);
            Ok::<bool, ()>(false)
        })
        .await;
        assert!(v3); // served from cache

        assert_eq!(probes.load(Ordering::SeqCst), 2);
    }

    // Fake NFT source: counts one call per tier and resolves an index when the
    // relevant `owned` token exists. "token:<item>:<tok>" / "exact:<urn>" /
    // "prefix:<urn>" let a test steer which tier resolves which URN.
    struct CountingNft {
        token: AtomicUsize,
        exact: AtomicUsize,
        prefix: AtomicUsize,
        owned: HashSet<String>,
    }

    impl CountingNft {
        fn new(owned: &[&str]) -> Self {
            Self {
                token: AtomicUsize::new(0),
                exact: AtomicUsize::new(0),
                prefix: AtomicUsize::new(0),
                owned: owned.iter().map(|s| s.to_string()).collect(),
            }
        }
        fn total_calls(&self) -> usize {
            self.token.load(Ordering::SeqCst)
                + self.exact.load(Ordering::SeqCst)
                + self.prefix.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl NftBatchSource for CountingNft {
        async fn tier_token(
            &self,
            _address: &str,
            items: &[(usize, String, String)],
        ) -> Result<Vec<usize>, ValidatorError> {
            self.token.fetch_add(1, Ordering::SeqCst);
            Ok(items
                .iter()
                .filter(|(_, item_urn, tok)| {
                    self.owned.contains(&format!("token:{item_urn}:{tok}"))
                })
                .map(|(i, _, _)| *i)
                .collect())
        }
        async fn tier_exact(
            &self,
            _address: &str,
            items: &[(usize, String)],
        ) -> Result<Vec<usize>, ValidatorError> {
            self.exact.fetch_add(1, Ordering::SeqCst);
            Ok(items
                .iter()
                .filter(|(_, urn)| self.owned.contains(&format!("exact:{urn}")))
                .map(|(i, _)| *i)
                .collect())
        }
        async fn tier_prefix(
            &self,
            _address: &str,
            items: &[(usize, String)],
        ) -> Result<Vec<usize>, ValidatorError> {
            self.prefix.fetch_add(1, Ordering::SeqCst);
            Ok(items
                .iter()
                .filter(|(_, urn)| self.owned.contains(&format!("prefix:{urn}")))
                .map(|(i, _)| *i)
                .collect())
        }
    }

    // [Performance] N URNs (mix of 7-part token URNs and bare item URNs) resolve
    // in AT MOST 3 tier round trips (one per non-empty tier), never 3N. Also
    // asserts verdict parity: each tier's owned entry flips exactly its URN true,
    // in positional order. Red-check: a per-URN loop makes total calls scale ~N.
    #[tokio::test]
    async fn nft_ownership_batch_query_count() {
        let mut urns: Vec<String> = Vec::new();
        for k in 0..10 {
            // 7-part collections URN, all-digit token => tier-1 eligible.
            urns.push(format!(
                "urn:decentraland:matic:collections-v2:0xabc{k}:0:{k}"
            ));
        }
        for k in 0..10 {
            // 6-part bare item URN => not tier-1 eligible.
            urns.push(format!(
                "urn:decentraland:matic:collections-v2:0xdef{k}:{k}"
            ));
        }
        assert_eq!(urns.len(), 20);

        // Own three URNs, one via each tier, to prove escalation + parity:
        //  - index 0 via tier-1 token (item_urn = first 6 parts, tok = "0")
        //  - index 5 via tier-2 exact (its full urn)
        //  - index 12 via tier-3 prefix (a bare urn, escalated past exact)
        let item0 = "urn:decentraland:matic:collections-v2:0xabc0:0";
        let owned = [
            format!("token:{item0}:0"),
            format!("exact:{}", urns[5]),
            format!("prefix:{}", urns[12]),
        ];
        let owned_refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
        let src = CountingNft::new(&owned_refs);

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

    // [Performance] N claimed names resolve in exactly ONE round trip. Red-check:
    // reverting to a per-name loop makes the count == N (here 8).
    #[tokio::test]
    async fn names_ownership_single_query() {
        let names: Vec<String> = (0..8).map(|k| format!("name{k}")).collect();
        let mut owners = HashMap::new();
        // name3 owned by the deployer; others unowned or owned by someone else.
        owners.insert("name3".to_string(), vec!["0xowner-ETHEREUM".to_string()]);
        owners.insert("name6".to_string(), vec!["0xstranger-ETHEREUM".to_string()]);
        let src = CountingName {
            calls: AtomicUsize::new(0),
            owners,
        };

        let failing = names_ownership_batch(&src, "0xowner", &names)
            .await
            .unwrap();

        assert_eq!(src.calls.load(Ordering::SeqCst), 1, "exactly one ENS query");
        // Everything except name3 is failing (unowned or owned by another).
        assert!(!failing.contains(&"name3".to_string()));
        assert!(failing.contains(&"name6".to_string()));
        assert_eq!(failing.len(), 7);
    }

    struct CountingParcel {
        calls: AtomicUsize,
        owned: HashMap<(i32, i32), ParcelOwnership>,
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

    // [Performance] N parcels resolve in exactly ONE ownership round trip (no
    // operator resolver configured). Red-check: reverting to per-parcel
    // check_parcel_access makes the count == N (here 25).
    #[tokio::test]
    async fn land_access_single_parcel_query() {
        let parcels: Vec<(i32, i32)> = (0..25).map(|k| (k, -k)).collect();
        let mut owned = HashMap::new();
        // (3,-3) owned by the deployer via parcel owner; (7,-7) via estate owner.
        owned.insert(
            (3, -3),
            ParcelOwnership {
                parcel_owner: Some("0xowner-ETHEREUM".to_string()),
                estate_owner: None,
            },
        );
        owned.insert(
            (7, -7),
            ParcelOwnership {
                parcel_owner: Some("0xother-ETHEREUM".to_string()),
                estate_owner: Some("0xowner-ETHEREUM".to_string()),
            },
        );
        let src = CountingParcel {
            calls: AtomicUsize::new(0),
            owned,
        };

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

    // [Performance] the batched flags call resolves N parcels' ownership in
    // exactly ONE round trip, with per-parcel verdicts positionally matching
    // the single `parcel_permission_flags`: owner leg for owned/estate parcels,
    // all-false for a stranger's, `None` for a pair the squid does not index.
    // Red-check: a per-parcel loop makes the count == N (here 4).
    #[tokio::test]
    async fn flags_batch_single_query_mixed_verdicts() {
        let parcels = vec![(1, 1), (2, 2), (3, 3), (4, 4)];
        let mut owned = HashMap::new();
        // (1,1) owned directly; (2,2) via estate; (3,3) someone else's;
        // (4,4) left unindexed.
        owned.insert(
            (1, 1),
            ParcelOwnership {
                parcel_owner: Some("0xowner-ETHEREUM".to_string()),
                estate_owner: None,
            },
        );
        owned.insert(
            (2, 2),
            ParcelOwnership {
                parcel_owner: Some("0xother-ETHEREUM".to_string()),
                estate_owner: Some("0xowner-ETHEREUM".to_string()),
            },
        );
        owned.insert(
            (3, 3),
            ParcelOwnership {
                parcel_owner: Some("0xother-ETHEREUM".to_string()),
                estate_owner: None,
            },
        );
        let src = CountingParcel {
            calls: AtomicUsize::new(0),
            owned,
        };

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

    // A resolver outage fails ONLY the operator legs: the owner leg is settled
    // locally and must survive, matching the single-call posture exactly.
    #[tokio::test]
    async fn flags_batch_resolver_outage_keeps_owner_leg() {
        let parcels = vec![(1, 1), (3, 3)];
        let mut owned = HashMap::new();
        owned.insert(
            (1, 1),
            ParcelOwnership {
                parcel_owner: Some("0xowner-ETHEREUM".to_string()),
                estate_owner: None,
            },
        );
        owned.insert(
            (3, 3),
            ParcelOwnership {
                parcel_owner: Some("0xother-ETHEREUM".to_string()),
                estate_owner: None,
            },
        );
        let src = CountingParcel {
            calls: AtomicUsize::new(0),
            owned,
        };
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

    // The granted operator legs ride the batch the same way they ride the
    // single call: an update-operator grant flips exactly that leg.
    #[tokio::test]
    async fn flags_batch_operator_grant_matches_single_call_shape() {
        let parcels = vec![(3, 3)];
        let mut owned = HashMap::new();
        owned.insert(
            (3, 3),
            ParcelOwnership {
                parcel_owner: Some("0xother-ETHEREUM".to_string()),
                estate_owner: None,
            },
        );
        let src = CountingParcel {
            calls: AtomicUsize::new(0),
            owned,
        };
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

    #[test]
    fn a_truncated_address_never_matches() {
        let account_id = "0x959e104e1a4db6317fa58f8295f586e1a978c297-ETHEREUM";
        for prefix in [
            "0x",
            "0x959e",
            "0x959e104e1a4db6317fa58f8295f586e1a978c29",
            "",
        ] {
            assert!(
                !address_matches_account_id(prefix, account_id),
                "prefix {prefix:?} must not authorize"
            );
        }
    }

    #[tokio::test]
    async fn decentraland_address_check() {
        let checker = SquidBlockchainChecker {
            pool: PgPool::connect_lazy("postgres://localhost/test").unwrap(),
            additional_decentraland_address: Some("0xextra".to_string()),
            tp_subgraph: None,
            tp_root_via_squid: false,
            operator_resolver: None,
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
