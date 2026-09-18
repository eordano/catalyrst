use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, TimeZone, Utc};
use futures::future::join_all;
use sqlx::PgPool;

use super::chain::CouponChainReader;
use super::contracts::{coupon_contracts, find_coupon_contracts, CouponContracts};
use super::errors::{
    discount_out_of_bounds, too_many_collections, CouponError, ALREADY_CANCELLED, AT_LEAST_ONE_USE,
    EFFECTIVE_BEFORE_EXPIRY, EXPIRATION_IN_THE_FUTURE, NO_ALLOWED_ROOT, NO_COLLECTIONS,
    NO_EXTERNAL_CHECKS, NO_USES_LEFT, ONLY_PERCENTAGE_DISCOUNTS, RUNS_TOO_LONG,
    SCHEDULED_TOO_FAR_AHEAD,
};
use super::merkle::{collections_root, from_hex32, to_hex32, unique_collections, MerkleError};
use super::signature::{
    coupon_signing_hash, digest_coupon_state_key, encode_coupon_data, hashed_signature_bytes,
    legacy_coupon_state_key, resolve_coupon_signature, DISCOUNT_TYPE_RATE,
};
use super::sql;
use super::types::{
    Coupon, CouponCreation, CouponPagination, CouponState, CouponStatus, CouponStoredState,
    DbCouponWithState, DEFAULT_PAGE_LIMIT, MAX_COUPON_COLLECTIONS, MAX_COUPON_DURATION_MS,
    MAX_COUPON_SCHEDULE_AHEAD_MS, MAX_DISCOUNT_PPM, MIN_DISCOUNT_PPM,
};
use crate::ports::trades::{checks_json, network_for_chain, TradeChecksInput};

const REFRESH_BATCH: i64 = 200;
/// How many coupons of a batch are read from chain at once -- enough to keep a tick short of its
/// interval, small enough not to hand the RPC the whole batch in one burst.
const REFRESH_CONCURRENCY: usize = 10;
const PG_UNIQUE_VIOLATION: &str = "23505";
const SIGNATURE_HEX_LEN: usize = 132;

fn is_unique_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(db) if db.code().as_deref() == Some(PG_UNIQUE_VIOLATION))
}

fn ms_to_utc(ms: i64) -> Option<DateTime<Utc>> {
    Utc.timestamp_millis_opt(ms).single()
}

fn is_zero_bytes32(value: &str) -> bool {
    value.is_empty() || value.eq_ignore_ascii_case("0x") || {
        let zero = format!("0x{}", "00".repeat(32));
        value.eq_ignore_ascii_case(&zero)
    }
}

/// Everything about a posted coupon that could be settled without touching the database or the
/// chain, in upstream's own order: which error a malformed request surfaces as is observable to
/// the shop.
#[derive(Debug)]
pub struct ValidatedCoupon {
    pub contracts: CouponContracts,
    pub network: String,
    pub collections: Vec<String>,
    pub root: [u8; 32],
    pub data: Vec<u8>,
    pub signature: String,
    pub hashed_signature: [u8; 32],
    pub state_key: [u8; 32],
}

fn normalize_collections(collections: &[String]) -> Result<Vec<String>, CouponError> {
    let unique = unique_collections(collections);
    if unique.is_empty() {
        return Err(CouponError::InvalidCollections(NO_COLLECTIONS.to_string()));
    }
    if unique.len() > MAX_COUPON_COLLECTIONS {
        return Err(CouponError::InvalidCollections(too_many_collections()));
    }
    Ok(unique)
}

fn validate_checks(checks: &TradeChecksInput, now_ms: i64) -> Result<(), CouponError> {
    let invalid = |why: &str| Err(CouponError::InvalidChecks(why.to_string()));
    if checks.uses < 1 {
        return invalid(AT_LEAST_ONE_USE);
    }
    if checks.expiration <= now_ms {
        return invalid(EXPIRATION_IN_THE_FUTURE);
    }
    if checks.effective >= checks.expiration {
        return invalid(EFFECTIVE_BEFORE_EXPIRY);
    }
    if checks.effective > now_ms.saturating_add(MAX_COUPON_SCHEDULE_AHEAD_MS) {
        return invalid(SCHEDULED_TOO_FAR_AHEAD);
    }
    if checks
        .expiration
        .saturating_sub(checks.effective.max(now_ms))
        > MAX_COUPON_DURATION_MS
    {
        return invalid(RUNS_TOO_LONG);
    }
    // The shop applies a coupon for whoever is buying, so one restricted to an allow-list or to
    // external checks would fail at checkout for everyone it is shown to.
    if !is_zero_bytes32(&checks.allowed_root) {
        return invalid(NO_ALLOWED_ROOT);
    }
    if checks.external_checks.iter().flatten().next().is_some() {
        return invalid(NO_EXTERNAL_CHECKS);
    }
    Ok(())
}

fn has_valid_v(signature: &str) -> bool {
    let trimmed = signature.strip_prefix("0x").unwrap_or(signature);
    trimmed
        .get(trimmed.len().saturating_sub(2)..)
        .and_then(|byte| u8::from_str_radix(byte, 16).ok())
        .is_some_and(|v| v == 27 || v == 28)
}

fn merkle_to_collections_error(e: MerkleError) -> CouponError {
    CouponError::InvalidCollections(e.to_string())
}

pub fn validate_creation(
    coupon: &CouponCreation,
    signer: &str,
    now_ms: i64,
) -> Result<ValidatedCoupon, CouponError> {
    if !coupon.signer.eq_ignore_ascii_case(signer) {
        return Err(CouponError::InvalidSigner);
    }

    let candidates = coupon_contracts(coupon.chain_id);
    let Some(first) = candidates.first() else {
        return Err(CouponError::UnsupportedChain(coupon.chain_id));
    };

    // One CollectionDiscountCoupon per chain, whichever manager the coupon is for.
    if !coupon
        .coupon_address
        .eq_ignore_ascii_case(first.collection_discount_coupon)
    {
        return Err(CouponError::InvalidAddress);
    }

    // The domain salt binds the signature to `chainId`; `network` is only a label, so a
    // mismatched one would make every later query that filters coupons by network miss.
    let network =
        network_for_chain(coupon.chain_id).ok_or(CouponError::UnsupportedChain(coupon.chain_id))?;
    if coupon.network != network {
        return Err(CouponError::InvalidNetwork);
    }

    if coupon.discount_type != DISCOUNT_TYPE_RATE {
        return Err(CouponError::InvalidDiscount(
            ONLY_PERCENTAGE_DISCOUNTS.to_string(),
        ));
    }
    if coupon.discount < MIN_DISCOUNT_PPM || coupon.discount > MAX_DISCOUNT_PPM {
        return Err(CouponError::InvalidDiscount(discount_out_of_bounds()));
    }

    let collections = normalize_collections(&coupon.collections)?;
    validate_checks(&coupon.checks, now_ms)?;

    if coupon.signature.len() != SIGNATURE_HEX_LEN || !has_valid_v(&coupon.signature) {
        return Err(CouponError::InvalidSignature);
    }
    // ECDSA lets the same signature be re-encoded with a flipped `s` and `v`. Both recover the
    // same signer but hash differently, so the two spellings would key two rows to one on-chain
    // coupon; the recovery helper refuses the non-canonical form outright.
    let signature = coupon.signature.to_lowercase();

    let root = collections_root(&collections).map_err(merkle_to_collections_error)?;
    let data = encode_coupon_data(coupon.discount_type, coupon.discount, root);
    // Whichever manager the creator signed against is the one the coupon belongs to.
    let contracts = resolve_coupon_signature(
        coupon.chain_id,
        &candidates,
        &coupon.checks,
        &coupon.coupon_address,
        &data,
        &signature,
        signer,
    )
    .ok_or(CouponError::InvalidSignature)?;

    let hashed_signature =
        hashed_signature_bytes(&signature).map_err(|_| CouponError::InvalidSignature)?;
    let state_key =
        legacy_coupon_state_key(signer, &signature).map_err(|_| CouponError::InvalidSignature)?;

    Ok(ValidatedCoupon {
        contracts,
        network: network.to_string(),
        collections,
        root,
        data,
        signature,
        hashed_signature,
        state_key,
    })
}

/// Creator-signed discount coupons for the shop.
///
/// A coupon is validated the way a trade is: the caller must be its signer, the EIP-712
/// signature must verify against one of the chain's CouponManagers, and everything the contract
/// will check at purchase time (creator, coupon allow-list, indexes, window) is checked here
/// first so a coupon that can never settle is never shown.
///
/// Each off-chain marketplace version has its own manager and only redeems coupons signed
/// against it, so the coupon is stored with the manager that verified it and reports that
/// marketplace.
#[derive(Clone)]
pub struct CouponsComponent {
    pool: PgPool,
    read: PgPool,
    chain: Arc<dyn CouponChainReader>,
}

impl CouponsComponent {
    pub fn new(pool: PgPool, read: PgPool, chain: Arc<dyn CouponChainReader>) -> Self {
        Self { pool, read, chain }
    }

    pub async fn add_coupon(
        &self,
        coupon: CouponCreation,
        signer: &str,
        now_ms: i64,
    ) -> Result<Coupon, CouponError> {
        let validated = validate_creation(&coupon, signer, now_ms)?;

        self.validate_creator(signer, &validated.collections, coupon.chain_id)
            .await?;

        let state_key = to_hex32(validated.state_key);
        let digest_key = coupon_signing_hash(
            coupon.chain_id,
            &validated.contracts,
            &coupon.checks,
            &coupon.coupon_address,
            &validated.data,
        )
        .map_err(|_| CouponError::InvalidSignature)
        .and_then(|digest| {
            digest_coupon_state_key(signer, digest)
                .map_err(|_| CouponError::InvalidSignature)
                .map(to_hex32)
        });

        // The three manager reads are independent; their results are checked in the original
        // order so a coupon failing several checks still gets the same error as before.
        let state_fut = async {
            match &digest_key {
                Ok(digest_key) => self
                    .chain
                    .read_state(
                        coupon.chain_id,
                        validated.contracts.coupon_manager.address,
                        &[digest_key.clone(), state_key.clone()],
                    )
                    .await
                    .map(Some),
                Err(_) => Ok(None),
            }
        };
        let (allowed, indexes, chain_state) = tokio::join!(
            self.chain.read_coupon_allowed(
                coupon.chain_id,
                validated.contracts.coupon_manager.address,
                &coupon.coupon_address,
            ),
            self.chain.read_indexes(
                coupon.chain_id,
                validated.contracts.coupon_manager.address,
                signer,
            ),
            state_fut,
        );

        // Each manager keeps its own allow-list of coupon contracts, and an older one may never
        // have learnt the current CollectionDiscountCoupon, so `applyCoupon` would revert on a
        // coupon that verifies here.
        let allowed = allowed.map_err(|e| CouponError::Internal(e.to_string()))?;
        if !allowed {
            return Err(CouponError::NotAllowed);
        }

        // The contract rejects a coupon whose indexes lag the manager's, so a stale one is
        // refused now rather than shown to buyers and failing at checkout.
        let indexes = indexes.map_err(|e| CouponError::Internal(e.to_string()))?;
        if indexes.contract_signature_index != coupon.checks.contract_signature_index
            || indexes.signer_signature_index != coupon.checks.signer_signature_index
        {
            return Err(CouponError::InvalidSignatureIndex);
        }

        digest_key?;
        let chain_state = chain_state
            .map_err(|e| CouponError::Internal(e.to_string()))?
            .ok_or_else(|| CouponError::Internal("coupon state was not read".to_string()))?;
        if chain_state.cancelled {
            return Err(CouponError::AlreadyUnusable(ALREADY_CANCELLED.to_string()));
        }
        if chain_state.uses >= coupon.checks.uses as i64 {
            return Err(CouponError::AlreadyUnusable(NO_USES_LEFT.to_string()));
        }
        // The indexes were just checked against the manager, so nothing is revoked yet.
        let state = CouponStoredState {
            uses: chain_state.uses,
            cancelled: chain_state.cancelled,
            revoked: false,
        };

        let checks = checks_json(&coupon.checks)
            .map_err(|e| CouponError::Internal(format!("checks are not serialisable: {e}")))?;
        let effective_since = ms_to_utc(coupon.checks.effective).ok_or_else(|| {
            CouponError::InvalidChecks(format!("unrepresentable time {}", coupon.checks.effective))
        })?;
        let expires_at = ms_to_utc(coupon.checks.expiration).ok_or_else(|| {
            CouponError::InvalidChecks(format!("unrepresentable time {}", coupon.checks.expiration))
        })?;

        let inserted: (String, DateTime<Utc>) = sqlx::query_as(sql::INSERT_COUPON_WITH_STATE)
            .bind(&validated.network)
            .bind(coupon.chain_id as i32)
            .bind(coupon.signer.to_lowercase())
            .bind(&validated.signature)
            .bind(to_hex32(validated.hashed_signature))
            .bind(&state_key)
            .bind(validated.contracts.coupon_manager.address.to_lowercase())
            .bind(coupon.coupon_address.to_lowercase())
            .bind(&checks)
            .bind(coupon.discount_type as i16)
            .bind(coupon.discount as i32)
            .bind(to_hex32(validated.root))
            .bind(&validated.collections)
            .bind(effective_since)
            .bind(expires_at)
            .bind(state.uses as i32)
            .bind(state.cancelled)
            .bind(state.revoked)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                if is_unique_violation(&e) {
                    CouponError::Duplicate
                } else {
                    CouponError::Db(e)
                }
            })?;
        let (id, created_at) = inserted;

        tracing::info!(
            coupon = %id,
            signer = %signer,
            collections = validated.collections.len(),
            "coupon created"
        );

        let row = DbCouponWithState {
            id,
            network: validated.network,
            chain_id: coupon.chain_id as i32,
            signer: coupon.signer.to_lowercase(),
            signature: validated.signature,
            state_key,
            coupon_manager: validated.contracts.coupon_manager.address.to_lowercase(),
            coupon_address: coupon.coupon_address.to_lowercase(),
            checks,
            discount_type: coupon.discount_type as i16,
            discount_ppm: coupon.discount as i32,
            root: to_hex32(validated.root),
            collections: validated.collections,
            effective_since,
            expires_at,
            created_at,
            state_uses: Some(state.uses as i32),
            state_cancelled: Some(state.cancelled),
            state_revoked: Some(state.revoked),
            state_checked_at: ms_to_utc(now_ms),
        };
        Ok(to_coupon(&row, now_ms))
    }

    async fn validate_creator(
        &self,
        signer: &str,
        collections: &[String],
        chain_id: i64,
    ) -> Result<(), CouponError> {
        let rows: Vec<(String, Option<String>)> =
            sqlx::query_as(sqlx::AssertSqlSafe(sql::COLLECTION_CREATORS.as_str()))
                .bind(collections)
                .bind(chain_id as i32)
                .fetch_all(&self.read)
                .await?;
        let creators: HashMap<String, Option<String>> = rows
            .into_iter()
            .map(|(id, creator)| (id.to_lowercase(), creator.map(|c| c.to_lowercase())))
            .collect();
        let signer = signer.to_lowercase();
        for collection in collections {
            if creators.get(collection).and_then(|c| c.as_deref()) != Some(signer.as_str()) {
                return Err(CouponError::NotCollectionCreator(collection.clone()));
            }
        }
        Ok(())
    }

    pub async fn get_coupons_by_signer(
        &self,
        signer: &str,
        pagination: CouponPagination,
    ) -> Result<Vec<Coupon>, CouponError> {
        let rows: Vec<DbCouponWithState> =
            sqlx::query_as(sqlx::AssertSqlSafe(sql::COUPONS_BY_SIGNER.as_str()))
                .bind(signer.to_lowercase())
                .bind(pagination.limit.unwrap_or(DEFAULT_PAGE_LIMIT))
                .bind(pagination.offset.unwrap_or(0))
                .fetch_all(&self.pool)
                .await?;
        let now = Utc::now().timestamp_millis();
        Ok(rows.iter().map(|row| to_coupon(row, now)).collect())
    }

    pub async fn get_coupon(&self, id: &str) -> Result<Coupon, CouponError> {
        let row: Option<DbCouponWithState> =
            sqlx::query_as(sqlx::AssertSqlSafe(sql::COUPON_BY_ID.as_str()))
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        let row = row.ok_or_else(|| CouponError::NotFound(id.to_string()))?;
        Ok(to_coupon(&row, Utc::now().timestamp_millis()))
    }

    /// Re-reads the on-chain state of every live or upcoming coupon. Answers how many were
    /// refreshed.
    pub async fn refresh_state(&self) -> Result<usize, CouponError> {
        let rows: Vec<DbCouponWithState> =
            sqlx::query_as(sqlx::AssertSqlSafe(sql::COUPONS_TO_REFRESH.as_str()))
                .bind(REFRESH_BATCH)
                .fetch_all(&self.pool)
                .await?;

        // Every coupon of one creator shares a signer, so the indexes cost roughly one read per
        // creator per tick rather than one per coupon. A failed read is not cached, so the next
        // chunk tries again instead of every later coupon of that signer inheriting one unlucky
        // timeout for the rest of the tick.
        let mut indexes = HashMap::new();
        let mut refreshed = 0usize;

        for chunk in rows.chunks(REFRESH_CONCURRENCY) {
            let readable: Vec<(&DbCouponWithState, Vec<String>)> = chunk
                .iter()
                .filter_map(|row| match state_keys_of(row) {
                    Some(keys) => Some((row, keys)),
                    None => {
                        tracing::warn!(
                            coupon_id = %row.id,
                            manager = %row.coupon_manager,
                            "coupon manager is not in the contract registry; leaving on-chain \
                             state unrefreshed"
                        );
                        None
                    }
                })
                .collect();

            let mut wanted: Vec<&DbCouponWithState> = Vec::new();
            for (row, _) in readable.iter() {
                let key = index_key(row);
                if !indexes.contains_key(&key) && !wanted.iter().any(|r| index_key(r) == key) {
                    wanted.push(row);
                }
            }
            let reads = join_all(wanted.iter().map(|row| {
                self.chain
                    .read_indexes(row.chain_id as i64, &row.coupon_manager, &row.signer)
            }))
            .await;
            for (row, read) in wanted.iter().zip(reads) {
                match read {
                    Ok(value) => {
                        indexes.insert(index_key(row), value);
                    }
                    Err(e) => tracing::warn!(
                        error = %e,
                        signer = %row.signer,
                        "could not read the coupon manager signature indexes"
                    ),
                }
            }

            let states = join_all(readable.iter().map(|(row, keys)| {
                self.chain
                    .read_state(row.chain_id as i64, &row.coupon_manager, keys)
            }))
            .await;
            for ((row, _), state) in readable.into_iter().zip(states) {
                let Some(index) = indexes.get(&index_key(row)) else {
                    continue;
                };
                let state = match state {
                    Ok(state) => state,
                    Err(e) => {
                        tracing::warn!(error = %e, coupon = %row.id, "could not refresh the state of a coupon");
                        continue;
                    }
                };
                // `cancelSignature` takes a coupon's whole calldata, so a creator ending every
                // sale at once reaches for `increaseSignerSignatureIndex()` instead. That leaves
                // `cancelled` false while the contract refuses the coupon.
                let Some(stored) = row.stored_checks() else {
                    tracing::warn!(coupon = %row.id, "skipping a coupon whose stored checks are unreadable");
                    continue;
                };
                let revoked = index.contract_signature_index != stored.contract_signature_index
                    || index.signer_signature_index != stored.signer_signature_index;
                match self
                    .upsert_state(
                        &row.id,
                        CouponStoredState {
                            uses: state.uses,
                            cancelled: state.cancelled,
                            revoked,
                        },
                    )
                    .await
                {
                    Ok(()) => refreshed += 1,
                    Err(e) => {
                        tracing::warn!(error = %e, coupon = %row.id, "could not store the refreshed coupon state")
                    }
                }
            }
        }

        // A per-tick line, not just the per-coupon warnings: failures are swallowed rather than
        // thrown, so a batch quietly failing half its reads would otherwise look exactly like a
        // batch succeeding.
        tracing::info!(
            "Refreshed the on-chain state of {refreshed}/{} coupon(s)",
            rows.len()
        );
        Ok(refreshed)
    }

    async fn upsert_state(&self, id: &str, state: CouponStoredState) -> Result<(), sqlx::Error> {
        sqlx::query(sql::UPSERT_COUPON_STATE)
            .bind(id)
            .bind(state.uses as i32)
            .bind(state.cancelled)
            .bind(state.revoked)
            .execute(&self.pool)
            .await
            .map(|_| ())
    }
}

/// Both slots this coupon could live in. `state_key` is the stored one, for the managers that
/// key on the signature bytes; the digest slot is rebuilt here, which the row can afford because
/// it names the manager it was signed against and so carries that EIP-712 domain by reference. A
/// manager the registry has stopped listing leaves nothing to rebuild from -- and could not have
/// taken the coupon in the first place -- so the read is refused and the row keeps its last known
/// state.
pub(super) fn state_keys_of(row: &DbCouponWithState) -> Option<Vec<String>> {
    let chain_id = row.chain_id as i64;
    let contracts = find_coupon_contracts(chain_id, &row.coupon_manager)?;
    let checks: TradeChecksInput = serde_json::from_value(row.checks.clone()).ok()?;
    let data = encode_coupon_data(
        row.discount_type as i64,
        row.discount_ppm as i64,
        from_hex32(&row.root)?,
    );
    let digest =
        coupon_signing_hash(chain_id, &contracts, &checks, &row.coupon_address, &data).ok()?;
    Some(vec![
        to_hex32(digest_coupon_state_key(&row.signer, digest).ok()?),
        row.state_key.clone(),
    ])
}

fn index_key(row: &DbCouponWithState) -> String {
    format!("{}:{}:{}", row.chain_id, row.coupon_manager, row.signer)
}

pub fn to_coupon(row: &DbCouponWithState, now_ms: i64) -> Coupon {
    let state = match (row.state_checked_at, row.state_uses, row.state_cancelled) {
        (Some(checked_at), Some(uses), Some(cancelled)) => Some(CouponState {
            uses: uses as i64,
            cancelled,
            revoked: row.state_revoked.unwrap_or(false),
            checked_at: checked_at.timestamp_millis(),
        }),
        _ => None,
    };
    let effective_since = row.effective_since.timestamp_millis();
    let expires_at = row.expires_at.timestamp_millis();
    let signed_uses = row.stored_checks().map(|c| c.uses);

    let status = match state {
        Some(state) if state.cancelled => CouponStatus::Cancelled,
        Some(state) if state.revoked => CouponStatus::Revoked,
        Some(state) if signed_uses.is_some_and(|u| state.uses >= u) => CouponStatus::Exhausted,
        _ if expires_at <= now_ms => CouponStatus::Ended,
        _ if effective_since > now_ms => CouponStatus::Scheduled,
        _ => CouponStatus::Active,
    };

    Coupon {
        id: row.id.clone(),
        signer: row.signer.clone(),
        chain_id: row.chain_id as i64,
        network: row.network.clone(),
        checks: row.checks.clone(),
        coupon_manager: row.coupon_manager.clone(),
        marketplace: find_coupon_contracts(row.chain_id as i64, &row.coupon_manager)
            .map(|contracts| contracts.marketplace),
        coupon_address: row.coupon_address.clone(),
        discount_type: row.discount_type as i64,
        discount: row.discount_ppm as i64,
        root: row.root.clone(),
        collections: row.collections.clone(),
        signature: row.signature.clone(),
        created_at: row.created_at.timestamp_millis(),
        state,
        status,
    }
}
