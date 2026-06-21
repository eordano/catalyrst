use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

use crate::http::ApiError;
use crate::AppState;

use super::common::{
    admin_actor, authorize_admin, normalize_address, paginate, validate_escrow_ref,
    validate_idempotency_key, validate_positive_amount, validated_reason,
};

#[derive(Debug, Deserialize)]
pub(super) struct GrantBody {
    address: String,

    amount: String,
    #[serde(default)]
    reason: Option<String>,

    #[serde(rename = "idempotencyKey", default)]
    idempotency_key: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct GrantOut {
    address: String,
    applied: String,
    available: String,
    replayed: bool,
}

pub(super) async fn grant_credits(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Option<Json<GrantBody>>,
) -> Result<Json<GrantOut>, ApiError> {
    authorize_admin(&state, &headers)?;
    let Json(b) = body.ok_or_else(|| ApiError::bad_request("missing JSON body"))?;
    let address = normalize_address(&b.address)?;
    let amount = validate_positive_amount(&b.amount)?;
    let reason = validated_reason(&b.reason)?;
    let idempotency_key = validate_idempotency_key(&b.idempotency_key)?;
    let actor = admin_actor(&headers);
    let detail = json!({
        "address": address, "amount": amount, "reason": reason,
        "idempotencyKey": idempotency_key,
    });
    let outcome = state
        .credits
        .admin_grant_credits(
            &address,
            &amount,
            "grant",
            reason.as_deref(),
            actor.as_deref(),
            idempotency_key.as_deref(),
            &detail,
        )
        .await?;
    Ok(Json(GrantOut {
        address,
        applied: outcome.applied,
        available: outcome.available,
        replayed: outcome.replayed,
    }))
}

pub(super) async fn revoke_credits(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Option<Json<GrantBody>>,
) -> Result<Json<GrantOut>, ApiError> {
    authorize_admin(&state, &headers)?;
    let Json(b) = body.ok_or_else(|| ApiError::bad_request("missing JSON body"))?;
    let address = normalize_address(&b.address)?;
    let amount = validate_positive_amount(&b.amount)?;
    let reason = validated_reason(&b.reason)?;
    let actor = admin_actor(&headers);
    let detail = json!({ "address": address, "amount": amount, "reason": reason });
    let outcome = state
        .credits
        .admin_revoke_credits(
            &address,
            &amount,
            reason.as_deref(),
            actor.as_deref(),
            &detail,
        )
        .await?;
    Ok(Json(GrantOut {
        address,
        applied: outcome.applied,
        available: outcome.available,
        replayed: outcome.replayed,
    }))
}

#[derive(Debug, Deserialize)]
pub(super) struct BlockBody {
    blocked: bool,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct BlockOut {
    address: String,
    blocked: bool,
}

pub(super) async fn block_user(
    State(state): State<AppState>,
    Path(address): Path<String>,
    headers: HeaderMap,
    body: Option<Json<BlockBody>>,
) -> Result<Json<BlockOut>, ApiError> {
    authorize_admin(&state, &headers)?;
    let address = normalize_address(&address)?;
    let Json(b) = body.ok_or_else(|| ApiError::bad_request("missing JSON body { blocked }"))?;
    let reason = validated_reason(&b.reason)?;
    let actor = admin_actor(&headers);
    let detail = json!({ "address": address, "blocked": b.blocked, "reason": reason });
    let blocked = state
        .credits
        .admin_set_blocked(
            &address,
            b.blocked,
            reason.as_deref(),
            actor.as_deref(),
            &detail,
        )
        .await?;
    Ok(Json(BlockOut { address, blocked }))
}

#[derive(Debug, Deserialize)]
pub(super) struct PurchaseListQuery {
    status: Option<String>,
    address: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Serialize)]
pub(super) struct PurchaseOut {
    id: i64,
    address: String,
    sku: String,
    credits: String,
    #[serde(rename = "amountCents")]
    amount_cents: i64,
    currency: String,
    #[serde(rename = "stripePaymentIntent")]
    stripe_payment_intent: Option<String>,
    method: String,
    status: String,
    #[serde(rename = "createdAt")]
    created_at: String,
}

pub(super) async fn list_purchases(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<PurchaseListQuery>,
) -> Result<Json<Vec<PurchaseOut>>, ApiError> {
    authorize_admin(&state, &headers)?;
    let address = match q.address.as_deref() {
        Some(a) => Some(normalize_address(a)?),
        None => None,
    };
    let (limit, offset) = paginate(q.limit, q.offset);
    let rows = state
        .credits
        .admin_list_purchases(q.status.as_deref(), address.as_deref(), limit, offset)
        .await?;
    let out = rows
        .into_iter()
        .map(|p| PurchaseOut {
            id: p.id,
            address: p.address,
            sku: p.sku,
            credits: p.credits,
            amount_cents: p.amount_cents,
            currency: p.currency,
            stripe_payment_intent: p.stripe_payment_intent,
            method: p.method,
            status: p.status,
            created_at: p.created_at.to_rfc3339(),
        })
        .collect();
    Ok(Json(out))
}

#[derive(Debug, Deserialize)]
pub(super) struct CheckoutListQuery {
    address: Option<String>,
    status: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Serialize)]
struct OutboxLineOut {
    id: i64,
    #[serde(rename = "itemId")]
    item_id: String,
    urn: String,
    #[serde(rename = "tokenId")]
    token_id: Option<String>,
    #[serde(rename = "unitPriceCredits")]
    unit_price_credits: String,
    mode: String,
    status: String,
    attempts: i32,
    #[serde(rename = "lastError")]
    last_error: Option<String>,
    #[serde(rename = "externalRef")]
    external_ref: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct CheckoutOut {
    id: i64,
    address: String,
    #[serde(rename = "totalCredits")]
    total_credits: String,
    status: String,
    #[serde(rename = "createdAt")]
    created_at: String,
    lines: Vec<OutboxLineOut>,
}

pub(super) async fn list_checkouts(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<CheckoutListQuery>,
) -> Result<Json<Vec<CheckoutOut>>, ApiError> {
    authorize_admin(&state, &headers)?;
    let address = match q.address.as_deref() {
        Some(a) => Some(normalize_address(a)?),
        None => None,
    };
    let (limit, offset) = paginate(q.limit, q.offset);
    let rows = state
        .credits
        .admin_list_checkouts(address.as_deref(), q.status.as_deref(), limit, offset)
        .await?;
    let out = rows
        .into_iter()
        .map(|c| CheckoutOut {
            id: c.id,
            address: c.address,
            total_credits: c.total_credits,
            status: c.status,
            created_at: c.created_at.to_rfc3339(),
            lines: c
                .lines
                .into_iter()
                .map(|l| OutboxLineOut {
                    id: l.id,
                    item_id: l.item_id,
                    urn: l.urn,
                    token_id: l.token_id,
                    unit_price_credits: l.unit_price_credits,
                    mode: l.mode,
                    status: l.status,
                    attempts: l.attempts,
                    last_error: l.last_error,
                    external_ref: l.external_ref,
                })
                .collect(),
        })
        .collect();
    Ok(Json(out))
}

#[derive(Debug, Deserialize)]
pub(super) struct LedgerListQuery {
    address: String,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Serialize)]
pub(super) struct LedgerOut {
    id: i64,
    address: String,
    kind: String,
    amount: String,
    #[serde(rename = "txRef")]
    tx_ref: Option<String>,
    #[serde(rename = "createdAt")]
    created_at: String,
}

pub(super) async fn list_ledger(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<LedgerListQuery>,
) -> Result<Json<Vec<LedgerOut>>, ApiError> {
    authorize_admin(&state, &headers)?;
    let address = normalize_address(&q.address)?;
    let (limit, offset) = paginate(q.limit, q.offset);
    let rows = state
        .credits
        .admin_list_ledger(&address, limit, offset)
        .await?;
    let out = rows
        .into_iter()
        .map(|e| LedgerOut {
            id: e.id,
            address: e.address,
            kind: e.kind,
            amount: e.amount,
            tx_ref: e.tx_ref,
            created_at: e.created_at.to_rfc3339(),
        })
        .collect();
    Ok(Json(out))
}

#[derive(Debug, Deserialize, Default)]
pub(super) struct ManualOpBody {
    #[serde(default)]
    reason: Option<String>,
}

pub(super) async fn refund_checkout(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    body: Option<Json<ManualOpBody>>,
) -> Result<Json<JsonValue>, ApiError> {
    authorize_admin(&state, &headers)?;
    let reason = validated_reason(&body.map(|b| b.0.reason).unwrap_or(None))?;
    let actor = admin_actor(&headers);

    let checkout = state
        .credits
        .get_checkout(id)
        .await?
        .ok_or_else(|| ApiError::not_found("checkout not found"))?;

    if checkout.status != "fulfilling" && checkout.status != "fulfilled" {
        return Err(ApiError::conflict(
            "checkout is not refundable (only a debited, not-yet-reversed checkout \
             in 'fulfilling'/'fulfilled' can be manually refunded)",
        ));
    }

    let idem = format!("admin:refund:{}", id);
    let tx_ref = format!("checkout:{}", id);
    tracing::info!(
        action = "checkout.refund",
        checkout_id = id,
        "admin manual refund"
    );
    let outcome = state
        .credits
        .refund(
            &checkout.address,
            &checkout.total_credits,
            &tx_ref,
            Some(&idem),
        )
        .await?;

    let detail = json!({
        "checkoutId": id, "address": checkout.address,
        "amount": checkout.total_credits, "replayed": outcome.replayed, "reason": reason,
    });
    state
        .credits
        .admin_audit_op(
            "checkout.refund",
            Some(&checkout.address),
            Some(id),
            Some(&checkout.total_credits),
            actor.as_deref(),
            &detail,
        )
        .await?;

    Ok(Json(json!({
        "checkoutId": id,
        "address": checkout.address,
        "refunded": checkout.total_credits,
        "available": outcome.available,
        "replayed": outcome.replayed,
    })))
}

pub(super) async fn force_fulfill_checkout(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    body: Option<Json<ManualOpBody>>,
) -> Result<Json<JsonValue>, ApiError> {
    authorize_admin(&state, &headers)?;
    let reason = validated_reason(&body.map(|b| b.0.reason).unwrap_or(None))?;
    let actor = admin_actor(&headers);

    tracing::info!(
        action = "checkout.force_fulfill",
        checkout_id = id,
        "admin force-fulfill"
    );
    let rearmed = state.credits.admin_force_fulfill(id).await?;

    let detail = json!({ "checkoutId": id, "rearmedLines": rearmed, "reason": reason });
    state
        .credits
        .admin_audit_op(
            "checkout.force_fulfill",
            None,
            Some(id),
            None,
            actor.as_deref(),
            &detail,
        )
        .await?;

    Ok(Json(json!({ "checkoutId": id, "rearmedLines": rearmed })))
}

fn escrow_deps(state: &AppState) -> Result<(&sqlx::PgPool, &str), ApiError> {
    let Some(pool) = state.usage_grants_pool.as_ref() else {
        return Err(ApiError::not_implemented(
            "escrow ops disabled (USAGE_GRANTS_PG_CONNECTION_STRING unset)",
        ));
    };
    let Some(token) = state.economy_admin_token.as_deref() else {
        return Err(ApiError::not_implemented(
            "escrow ops disabled (CATALYRST_ECONOMY_ADMIN_TOKEN unset)",
        ));
    };
    Ok((pool, token))
}

pub(super) async fn reclaim_grant(
    State(state): State<AppState>,
    Path(escrow_ref): Path<String>,
    headers: HeaderMap,
    body: Option<Json<ManualOpBody>>,
) -> Result<Json<JsonValue>, ApiError> {
    authorize_admin(&state, &headers)?;
    let escrow_ref = validate_escrow_ref(&escrow_ref)?;
    let reason = validated_reason(&body.map(|b| b.0.reason).unwrap_or(None))?;
    let actor = admin_actor(&headers);
    let (pool, token) = escrow_deps(&state)?;

    let grant = crate::ports::admin::fetch_usage_grant(pool, &escrow_ref)
        .await?
        .ok_or_else(|| ApiError::not_found("usage_grant not found"))?;

    if grant.status != "active" && grant.status != "revoked" {
        return Err(ApiError::conflict(
            "usage_grant is not reclaimable (must be active or a resumable revoked)",
        ));
    }
    let (Some(collection), Some(token_id)) =
        (grant.collection.as_deref(), grant.token_id.as_deref())
    else {
        return Err(ApiError::bad_request(
            "grant has no on-chain token_id/collection; cannot reclaim (primary mint pending)",
        ));
    };

    let idem = format!("admin:reclaim:{}", escrow_ref);
    tracing::info!(action = "grant.reclaim", escrow_ref = %escrow_ref, "admin reclaim");
    let tx_hash = crate::ports::escrow::reclaim_escrowed(
        &state.economy_http,
        &state.economy_base_url,
        token,
        collection,
        token_id,
        &idem,
    )
    .await?;

    state.credits.revoke_usage_grant(pool, &escrow_ref).await?;

    let refunded = match state
        .credits
        .find_confirmed_line_by_ref(&escrow_ref)
        .await?
    {
        Some((address, amount)) => {
            let tx_ref = format!("reclaim:{}", escrow_ref);
            state
                .credits
                .refund(&address, &amount, &tx_ref, Some(&idem))
                .await?;
            Some((address, amount))
        }
        None => None,
    };

    let detail = json!({
        "escrowRef": escrow_ref, "grantee": grant.grantee_address, "urn": grant.urn,
        "txHash": tx_hash, "refunded": refunded.as_ref().map(|(_, a)| a.clone()),
        "reason": reason,
    });
    state
        .credits
        .admin_audit_op(
            "grant.reclaim",
            refunded.as_ref().map(|(a, _)| a.as_str()),
            None,
            refunded.as_ref().map(|(_, a)| a.as_str()),
            actor.as_deref(),
            &detail,
        )
        .await?;

    Ok(Json(json!({
        "escrowRef": escrow_ref,
        "txHash": tx_hash,
        "refunded": refunded.map(|(addr, amt)| json!({ "address": addr, "amount": amt })),
    })))
}

pub(super) async fn release_grant(
    State(state): State<AppState>,
    Path(escrow_ref): Path<String>,
    headers: HeaderMap,
    body: Option<Json<ManualOpBody>>,
) -> Result<Json<JsonValue>, ApiError> {
    authorize_admin(&state, &headers)?;
    let escrow_ref = validate_escrow_ref(&escrow_ref)?;
    let reason = validated_reason(&body.map(|b| b.0.reason).unwrap_or(None))?;
    let actor = admin_actor(&headers);
    let (pool, token) = escrow_deps(&state)?;

    let grant = crate::ports::admin::fetch_usage_grant(pool, &escrow_ref)
        .await?
        .ok_or_else(|| ApiError::not_found("usage_grant not found"))?;

    if grant.status != "active" {
        return Err(ApiError::conflict(
            "usage_grant is not releasable (must be active)",
        ));
    }
    if grant.unlock_at > chrono::Utc::now() {
        return Err(ApiError::conflict(format!(
            "usage_grant is still in the return window; release is not allowed until unlock_at ({}). Use reclaim to return during the window.",
            grant.unlock_at.to_rfc3339()
        )));
    }
    let (Some(collection), Some(token_id)) =
        (grant.collection.as_deref(), grant.token_id.as_deref())
    else {
        return Err(ApiError::bad_request(
            "grant has no on-chain token_id/collection; cannot release (primary mint pending)",
        ));
    };

    let idem = format!("admin:release:{}", escrow_ref);
    tracing::info!(action = "grant.release", escrow_ref = %escrow_ref, "admin release");
    let tx_hash = crate::ports::escrow::release_escrowed(
        &state.economy_http,
        &state.economy_base_url,
        token,
        collection,
        token_id,
        &grant.grantee_address,
        &idem,
    )
    .await?;
    crate::ports::admin::mark_usage_grant_released(pool, &escrow_ref).await?;

    let detail = json!({
        "escrowRef": escrow_ref, "grantee": grant.grantee_address, "urn": grant.urn,
        "txHash": tx_hash, "reason": reason,
    });
    state
        .credits
        .admin_audit_op(
            "grant.release",
            Some(&grant.grantee_address),
            None,
            None,
            actor.as_deref(),
            &detail,
        )
        .await?;

    Ok(Json(json!({ "escrowRef": escrow_ref, "txHash": tx_hash })))
}

pub(super) async fn reconcile(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<crate::ports::reconcile::ReconcileReport>, ApiError> {
    authorize_admin(&state, &headers)?;
    tracing::info!(action = "reconcile", "admin reconciliation run");
    let report = state
        .credits
        .reconcile(state.usage_grants_pool.as_ref())
        .await?;
    Ok(Json(report))
}
