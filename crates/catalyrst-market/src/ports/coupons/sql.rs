use std::sync::LazyLock;

use crate::MARKETPLACE_SQUID_SCHEMA;

pub const INSERT_COUPON: &str = "INSERT INTO marketplace.coupons \
     (network, chain_id, signer, signature, hashed_signature, state_key, coupon_manager, \
      coupon_address, checks, discount_type, discount_ppm, root, collections, \
      effective_since, expires_at) \
 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15) \
 RETURNING id::text, created_at";

pub const UPSERT_COUPON_STATE: &str =
    "INSERT INTO marketplace.coupon_state (coupon_id, uses, cancelled, revoked, checked_at) \
     VALUES ($1::uuid, $2, $3, $4, now()) \
     ON CONFLICT (coupon_id) DO UPDATE SET \
       uses = EXCLUDED.uses, cancelled = EXCLUDED.cancelled, \
       revoked = EXCLUDED.revoked, checked_at = now()";

const SELECT_WITH_STATE: &str =
    "SELECT c.id::text AS id, c.network, c.chain_id, c.signer, c.signature, c.state_key, \
            c.coupon_manager, c.coupon_address, c.checks, c.discount_type, c.discount_ppm, \
            c.root, c.collections, c.effective_since, c.expires_at, c.created_at, \
            cs.uses AS state_uses, cs.cancelled AS state_cancelled, \
            cs.revoked AS state_revoked, cs.checked_at AS state_checked_at \
     FROM marketplace.coupons c \
     LEFT JOIN marketplace.coupon_state cs ON cs.coupon_id = c.id";

pub static COUPONS_BY_SIGNER: LazyLock<String> = LazyLock::new(|| {
    format!("{SELECT_WITH_STATE} WHERE c.signer = $1 ORDER BY c.created_at DESC LIMIT $2 OFFSET $3")
});

pub static COUPON_BY_ID: LazyLock<String> =
    LazyLock::new(|| format!("{SELECT_WITH_STATE} WHERE c.id = $1::uuid"));

/// The coupons whose on-chain state is worth re-reading: live ones, and ones starting within a
/// day so the first read lands before the first buyer. Least recently checked first, bounded so
/// one tick stays cheap. Cancelled and revoked ones are left out -- neither can revert on chain,
/// so re-reading them would spend a slot of the batch on an answer that cannot change.
pub static COUPONS_TO_REFRESH: LazyLock<String> = LazyLock::new(|| {
    format!(
        "{SELECT_WITH_STATE} WHERE c.expires_at > now() \
           AND c.effective_since <= now() + interval '1 day' \
           AND cs.cancelled IS NOT TRUE \
           AND cs.revoked IS NOT TRUE \
         ORDER BY cs.checked_at ASC NULLS FIRST \
         LIMIT $1"
    )
});

/// Who created each collection on the coupon's own chain, from the squid. Scoped by `chain_id`
/// because the table holds every network the squid follows: the same address on another chain is
/// a different contract.
pub static COLLECTION_CREATORS: LazyLock<String> = LazyLock::new(|| {
    format!("SELECT id, creator FROM {MARKETPLACE_SQUID_SCHEMA}.collection WHERE id = ANY($1) AND chain_id = $2")
});
