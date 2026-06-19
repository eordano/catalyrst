//! Admin console controls owned by catalyrst-market (docs/admin-console.md §4):
//! moderation flags on local listings/trades, trade dispute status, operator
//! force-cancel of a local listing, and an append-only admin audit log.
//!
//! Every route here is gated by a bearer token compared in constant time against
//! the crate's admin token env (`CATALYRST_MARKET_ADMIN_TOKEN`, surfaced as
//! `AppStateInner::admin_token`). When that env is unset the gate fails closed
//! (403) so a default deploy exposes no admin surface. These controls are
//! OPERATOR-authored and act on this node's local marketplace state only — they
//! are NOT minted as EIP-712 federation actions, with one deliberate exception:
//! force-cancel appends an operator-authored row to the existing
//! `market_cancellations` log so it propagates over the changes feed.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::AppState;

type AdminResponse = (StatusCode, Json<Value>);

/// Constant-time compare, mirroring catalyrst-comms `authorize_moderator` /
/// catalyrst-badges `timing_safe_eq`. Length is allowed to leak (same as
/// upstream); only the byte comparison is non-short-circuiting.
fn timing_safe_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
}

fn err(code: StatusCode, message: impl Into<String>) -> AdminResponse {
    (code, Json(json!({ "ok": false, "message": message.into() })))
}

/// Returns the authenticated admin identity, or a 403 response. Fails closed
/// (403) when no admin token is configured.
fn require_admin(state: &AppState, headers: &HeaderMap) -> Result<String, AdminResponse> {
    let Some(expected) = state.admin_token.as_deref() else {
        return Err(err(
            StatusCode::FORBIDDEN,
            "admin controls disabled (CATALYRST_MARKET_ADMIN_TOKEN unset)",
        ));
    };
    match bearer_token(headers) {
        Some(got) if timing_safe_eq(got.as_bytes(), expected.as_bytes()) => {
            Ok("admin-token".to_string())
        }
        _ => Err(err(StatusCode::FORBIDDEN, "admin bearer token required")),
    }
}

fn now_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Append an append-only audit row. Best-effort: a failure to write the audit
/// row is logged but does not mask the primary mutation's result — the primary
/// mutation and the audit write share the same connection pool and the primary
/// has already committed by the time this is called.
async fn write_audit(
    state: &AppState,
    actor: &str,
    action: &str,
    target_kind: &str,
    target_hash: &str,
    detail: Value,
) {
    let res = sqlx::query(
        "INSERT INTO market_admin_audit (actor, action, target_kind, target_hash, detail, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(actor)
    .bind(action)
    .bind(target_kind)
    .bind(target_hash)
    .bind(detail)
    .bind(now_secs())
    .execute(&state.pool)
    .await;
    if let Err(e) = res {
        tracing::error!(error = %e, action, target_hash, "admin audit write failed");
    }
}

fn valid_target_kind(kind: &str) -> bool {
    matches!(kind, "bid" | "order" | "trade")
}

/// Confirm the target exists in this node's local federation log before we let
/// an operator flag/dispute/cancel it. Returns Ok(true) if present.
async fn target_exists(state: &AppState, kind: &str, hash: &str) -> Result<bool, sqlx::Error> {
    let table = match kind {
        "bid" => "market_bids_local",
        "order" => "market_orders_local",
        "trade" => "market_trades_local",
        _ => return Ok(false),
    };
    let row: Option<(i64,)> = sqlx::query_as(&format!(
        "SELECT 1 FROM {table} WHERE signature_hash = $1 LIMIT 1"
    ))
    .bind(hash)
    .fetch_optional(&state.pool)
    .await?;
    Ok(row.is_some())
}

// ---------------------------------------------------------------------------
// Moderation flags
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
pub struct FlagBody {
    /// review | hide | block (default review)
    #[serde(default)]
    pub severity: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

/// `POST /v1/admin/moderation/{kind}/{hash}/flag` — bearer-gated. Sets (upserts)
/// a moderation flag on a local bid/order/trade. Body `{ severity?, reason? }`.
pub async fn set_flag(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((kind, hash)): Path<(String, String)>,
    body: Option<Json<FlagBody>>,
) -> AdminResponse {
    let actor = match require_admin(&state, &headers) {
        Ok(a) => a,
        Err(e) => return e,
    };
    if !valid_target_kind(&kind) {
        return err(StatusCode::BAD_REQUEST, "kind must be bid|order|trade");
    }
    let b = body.map(|Json(b)| b).unwrap_or_default();
    let severity = b.severity.unwrap_or_else(|| "review".to_string());
    if !matches!(severity.as_str(), "review" | "hide" | "block") {
        return err(StatusCode::BAD_REQUEST, "severity must be review|hide|block");
    }
    let reason = b.reason.unwrap_or_default();

    match target_exists(&state, &kind, &hash).await {
        Ok(true) => {}
        Ok(false) => return err(StatusCode::NOT_FOUND, "target not found in local log"),
        Err(e) => {
            tracing::error!(error = %e, "set_flag existence check failed");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "database error");
        }
    }

    let res = sqlx::query(
        "INSERT INTO market_moderation_flags (target_hash, target_kind, severity, reason, flagged_by, flagged_at) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         ON CONFLICT (target_hash) DO UPDATE SET \
           target_kind = EXCLUDED.target_kind, severity = EXCLUDED.severity, \
           reason = EXCLUDED.reason, flagged_by = EXCLUDED.flagged_by, flagged_at = EXCLUDED.flagged_at",
    )
    .bind(&hash)
    .bind(&kind)
    .bind(&severity)
    .bind(&reason)
    .bind(&actor)
    .bind(now_secs())
    .execute(&state.pool)
    .await;

    if let Err(e) = res {
        tracing::error!(error = %e, "set_flag upsert failed");
        return err(StatusCode::INTERNAL_SERVER_ERROR, "database error");
    }
    write_audit(
        &state,
        &actor,
        "flag.set",
        &kind,
        &hash,
        json!({ "severity": severity, "reason": reason }),
    )
    .await;
    (
        StatusCode::OK,
        Json(json!({ "ok": true, "target_kind": kind, "target_hash": hash, "severity": severity })),
    )
}

/// `DELETE /v1/admin/moderation/{kind}/{hash}/flag` — bearer-gated. Clears the
/// moderation flag (if any). Idempotent.
pub async fn clear_flag(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((kind, hash)): Path<(String, String)>,
) -> AdminResponse {
    let actor = match require_admin(&state, &headers) {
        Ok(a) => a,
        Err(e) => return e,
    };
    if !valid_target_kind(&kind) {
        return err(StatusCode::BAD_REQUEST, "kind must be bid|order|trade");
    }
    let res = sqlx::query("DELETE FROM market_moderation_flags WHERE target_hash = $1")
        .bind(&hash)
        .execute(&state.pool)
        .await;
    let removed = match res {
        Ok(r) => r.rows_affected() > 0,
        Err(e) => {
            tracing::error!(error = %e, "clear_flag delete failed");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "database error");
        }
    };
    if removed {
        write_audit(&state, &actor, "flag.clear", &kind, &hash, json!({})).await;
    }
    (
        StatusCode::OK,
        Json(json!({ "ok": true, "target_hash": hash, "removed": removed })),
    )
}

#[derive(Debug, Default, Deserialize)]
pub struct ListFlagsQuery {
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub severity: Option<String>,
}

/// `GET /v1/admin/moderation/flags` — bearer-gated. Lists active moderation
/// flags, optionally filtered by `?kind=` and `?severity=`. Capped at 500.
pub async fn list_flags(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListFlagsQuery>,
) -> AdminResponse {
    if let Err(e) = require_admin(&state, &headers) {
        return e;
    }
    let rows: Result<Vec<(String, String, String, String, String, i64)>, _> = sqlx::query_as(
        "SELECT target_hash, target_kind, severity, reason, flagged_by, flagged_at \
           FROM market_moderation_flags \
          WHERE ($1::text IS NULL OR target_kind = $1) \
            AND ($2::text IS NULL OR severity = $2) \
          ORDER BY flagged_at DESC LIMIT 500",
    )
    .bind(q.kind.as_deref())
    .bind(q.severity.as_deref())
    .fetch_all(&state.pool)
    .await;

    match rows {
        Ok(rows) => {
            let data: Vec<Value> = rows
                .into_iter()
                .map(|(target_hash, target_kind, severity, reason, flagged_by, flagged_at)| {
                    json!({
                        "target_hash": target_hash,
                        "target_kind": target_kind,
                        "severity": severity,
                        "reason": reason,
                        "flagged_by": flagged_by,
                        "flagged_at": flagged_at,
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({ "data": data, "total": data.len() })))
        }
        Err(e) => {
            tracing::error!(error = %e, "list_flags failed");
            err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

// ---------------------------------------------------------------------------
// Disputes (trades only)
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
pub struct OpenDisputeBody {
    #[serde(default)]
    pub reason: Option<String>,
}

/// `POST /v1/admin/disputes/{trade_hash}/open` — bearer-gated. Opens (or
/// re-opens) a dispute against a recorded local trade. Body `{ reason? }`.
pub async fn open_dispute(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(trade_hash): Path<String>,
    body: Option<Json<OpenDisputeBody>>,
) -> AdminResponse {
    let actor = match require_admin(&state, &headers) {
        Ok(a) => a,
        Err(e) => return e,
    };
    let reason = body.and_then(|Json(b)| b.reason).unwrap_or_default();

    match target_exists(&state, "trade", &trade_hash).await {
        Ok(true) => {}
        Ok(false) => return err(StatusCode::NOT_FOUND, "trade not found in local log"),
        Err(e) => {
            tracing::error!(error = %e, "open_dispute existence check failed");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "database error");
        }
    }

    let res = sqlx::query(
        "INSERT INTO market_disputes (trade_hash, status, reason, opened_by, opened_at) \
         VALUES ($1, 'open', $2, $3, $4) \
         ON CONFLICT (trade_hash) DO UPDATE SET \
           status = 'open', reason = EXCLUDED.reason, opened_by = EXCLUDED.opened_by, \
           opened_at = EXCLUDED.opened_at, resolution = '', resolved_by = NULL, resolved_at = NULL",
    )
    .bind(&trade_hash)
    .bind(&reason)
    .bind(&actor)
    .bind(now_secs())
    .execute(&state.pool)
    .await;

    if let Err(e) = res {
        tracing::error!(error = %e, "open_dispute upsert failed");
        return err(StatusCode::INTERNAL_SERVER_ERROR, "database error");
    }
    write_audit(
        &state,
        &actor,
        "dispute.open",
        "trade",
        &trade_hash,
        json!({ "reason": reason }),
    )
    .await;
    (
        StatusCode::OK,
        Json(json!({ "ok": true, "trade_hash": trade_hash, "status": "open" })),
    )
}

#[derive(Debug, Default, Deserialize)]
pub struct ResolveDisputeBody {
    /// resolved | rejected (default resolved)
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub resolution: Option<String>,
}

/// `POST /v1/admin/disputes/{trade_hash}/resolve` — bearer-gated. Closes an
/// open dispute. Body `{ status?: resolved|rejected, resolution? }`.
pub async fn resolve_dispute(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(trade_hash): Path<String>,
    body: Option<Json<ResolveDisputeBody>>,
) -> AdminResponse {
    let actor = match require_admin(&state, &headers) {
        Ok(a) => a,
        Err(e) => return e,
    };
    let b = body.map(|Json(b)| b).unwrap_or_default();
    let status = b.status.unwrap_or_else(|| "resolved".to_string());
    if !matches!(status.as_str(), "resolved" | "rejected") {
        return err(StatusCode::BAD_REQUEST, "status must be resolved|rejected");
    }
    let resolution = b.resolution.unwrap_or_default();

    let res = sqlx::query(
        "UPDATE market_disputes \
            SET status = $2, resolution = $3, resolved_by = $4, resolved_at = $5 \
          WHERE trade_hash = $1 AND status = 'open'",
    )
    .bind(&trade_hash)
    .bind(&status)
    .bind(&resolution)
    .bind(&actor)
    .bind(now_secs())
    .execute(&state.pool)
    .await;

    let updated = match res {
        Ok(r) => r.rows_affected() > 0,
        Err(e) => {
            tracing::error!(error = %e, "resolve_dispute update failed");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "database error");
        }
    };
    if !updated {
        return err(StatusCode::NOT_FOUND, "no open dispute for this trade");
    }
    write_audit(
        &state,
        &actor,
        "dispute.resolve",
        "trade",
        &trade_hash,
        json!({ "status": status, "resolution": resolution }),
    )
    .await;
    (
        StatusCode::OK,
        Json(json!({ "ok": true, "trade_hash": trade_hash, "status": status })),
    )
}

#[derive(Debug, Default, Deserialize)]
pub struct ListDisputesQuery {
    #[serde(default)]
    pub status: Option<String>,
}

/// `GET /v1/admin/disputes` — bearer-gated. Lists disputes, optionally filtered
/// by `?status=open|resolved|rejected`. Capped at 500.
pub async fn list_disputes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListDisputesQuery>,
) -> AdminResponse {
    if let Err(e) = require_admin(&state, &headers) {
        return e;
    }
    let rows: Result<Vec<(String, String, String, String, String, i64, Option<String>, Option<i64>)>, _> =
        sqlx::query_as(
            "SELECT trade_hash, status, reason, resolution, opened_by, opened_at, resolved_by, resolved_at \
               FROM market_disputes \
              WHERE ($1::text IS NULL OR status = $1) \
              ORDER BY opened_at DESC LIMIT 500",
        )
        .bind(q.status.as_deref())
        .fetch_all(&state.pool)
        .await;

    match rows {
        Ok(rows) => {
            let data: Vec<Value> = rows
                .into_iter()
                .map(
                    |(trade_hash, status, reason, resolution, opened_by, opened_at, resolved_by, resolved_at)| {
                        json!({
                            "trade_hash": trade_hash,
                            "status": status,
                            "reason": reason,
                            "resolution": resolution,
                            "opened_by": opened_by,
                            "opened_at": opened_at,
                            "resolved_by": resolved_by,
                            "resolved_at": resolved_at,
                        })
                    },
                )
                .collect();
            (StatusCode::OK, Json(json!({ "data": data, "total": data.len() })))
        }
        Err(e) => {
            tracing::error!(error = %e, "list_disputes failed");
            err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

// ---------------------------------------------------------------------------
// Operator force-cancel
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
pub struct ForceCancelBody {
    #[serde(default)]
    pub reason: Option<String>,
}

/// `POST /v1/admin/listings/{kind}/{hash}/force-cancel` — bearer-gated. Records
/// an operator-authored cancellation of a local bid/order by appending a row to
/// the existing `market_cancellations` log (so it propagates over the changes
/// feed). `kind` must be bid|order (trades are immutable on-chain settlements;
/// use a dispute instead). Idempotent: a second call for the same target is a
/// no-op once a cancellation already exists.
pub async fn force_cancel(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((kind, hash)): Path<(String, String)>,
    body: Option<Json<ForceCancelBody>>,
) -> AdminResponse {
    let actor = match require_admin(&state, &headers) {
        Ok(a) => a,
        Err(e) => return e,
    };
    if !matches!(kind.as_str(), "bid" | "order") {
        return err(
            StatusCode::BAD_REQUEST,
            "kind must be bid|order (trades cannot be force-cancelled)",
        );
    }
    let reason = body.and_then(|Json(b)| b.reason).unwrap_or_default();

    match target_exists(&state, &kind, &hash).await {
        Ok(true) => {}
        Ok(false) => return err(StatusCode::NOT_FOUND, "target not found in local log"),
        Err(e) => {
            tracing::error!(error = %e, "force_cancel existence check failed");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "database error");
        }
    }

    // Already cancelled? Treat as a no-op success (idempotent).
    let existing: Result<Option<(String,)>, _> = sqlx::query_as(
        "SELECT signature_hash FROM market_cancellations WHERE target_signature_hash = $1 LIMIT 1",
    )
    .bind(&hash)
    .fetch_optional(&state.pool)
    .await;
    match existing {
        Ok(Some((sig,))) => {
            return (
                StatusCode::OK,
                Json(json!({ "ok": true, "target_hash": hash, "cancellation_hash": sig, "already_cancelled": true })),
            );
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!(error = %e, "force_cancel existing check failed");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "database error");
        }
    }

    let now = now_secs();
    // Deterministic, non-forgeable operator cancellation hash. It is NOT an
    // EIP-712 signature hash — the `operator:` prefix marks it as an authority
    // override, and `signer` is recorded as `operator:<actor>`.
    let mut h = Sha256::new();
    h.update(b"operator-force-cancel:");
    h.update(kind.as_bytes());
    h.update(b":");
    h.update(hash.as_bytes());
    h.update(b":");
    h.update(now.to_le_bytes());
    let cancellation_hash = format!("operator:{}", hex::encode(h.finalize()));
    let operator_signer = format!("operator:{actor}");
    let payload = json!({
        "operator_force_cancel": true,
        "actor": actor,
        "reason": reason,
        "target_kind": kind,
    });

    let res = sqlx::query(
        "INSERT INTO market_cancellations \
           (signature_hash, target_signature_hash, kind, signer, signed_at, message_payload, received_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(&cancellation_hash)
    .bind(&hash)
    .bind(&kind)
    .bind(&operator_signer)
    .bind(now)
    .bind(&payload)
    .bind(now)
    .execute(&state.pool)
    .await;

    if let Err(e) = res {
        tracing::error!(error = %e, "force_cancel insert failed");
        return err(StatusCode::INTERNAL_SERVER_ERROR, "database error");
    }
    write_audit(
        &state,
        &actor,
        "listing.force_cancel",
        &kind,
        &hash,
        json!({ "reason": reason, "cancellation_hash": cancellation_hash }),
    )
    .await;
    (
        StatusCode::OK,
        Json(json!({ "ok": true, "target_hash": hash, "cancellation_hash": cancellation_hash })),
    )
}

// ---------------------------------------------------------------------------
// Audit log read
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
pub struct AuditQuery {
    #[serde(default)]
    pub target_hash: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
}

/// `GET /v1/admin/audit` — bearer-gated. Reads the append-only admin audit log,
/// most-recent first, optionally filtered by `?target_hash=` and `?action=`.
/// `?limit=` is clamped to [1, 1000], default 200.
pub async fn list_audit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<AuditQuery>,
) -> AdminResponse {
    if let Err(e) = require_admin(&state, &headers) {
        return e;
    }
    let limit = q.limit.unwrap_or(200).clamp(1, 1000);
    let rows: Result<Vec<(i64, String, String, String, String, Value, i64)>, _> = sqlx::query_as(
        "SELECT id, actor, action, target_kind, target_hash, detail, created_at \
           FROM market_admin_audit \
          WHERE ($1::text IS NULL OR target_hash = $1) \
            AND ($2::text IS NULL OR action = $2) \
          ORDER BY id DESC LIMIT $3",
    )
    .bind(q.target_hash.as_deref())
    .bind(q.action.as_deref())
    .bind(limit)
    .fetch_all(&state.pool)
    .await;

    match rows {
        Ok(rows) => {
            let data: Vec<Value> = rows
                .into_iter()
                .map(|(id, actor, action, target_kind, target_hash, detail, created_at)| {
                    json!({
                        "id": id,
                        "actor": actor,
                        "action": action,
                        "target_kind": target_kind,
                        "target_hash": target_hash,
                        "detail": detail,
                        "created_at": created_at,
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({ "data": data, "total": data.len() })))
        }
        Err(e) => {
            tracing::error!(error = %e, "list_audit failed");
            err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timing_safe_eq_matches_and_mismatches() {
        assert!(timing_safe_eq(b"secret", b"secret"));
        assert!(!timing_safe_eq(b"secret", b"secreT"));
        assert!(!timing_safe_eq(b"secret", b"secret-longer"));
        assert!(!timing_safe_eq(b"", b"x"));
    }

    #[test]
    fn bearer_token_parses_prefix() {
        let mut h = HeaderMap::new();
        h.insert("authorization", "Bearer abc123".parse().unwrap());
        assert_eq!(bearer_token(&h), Some("abc123"));

        let mut h2 = HeaderMap::new();
        h2.insert("authorization", "Basic abc123".parse().unwrap());
        assert_eq!(bearer_token(&h2), None);

        assert_eq!(bearer_token(&HeaderMap::new()), None);
    }

    #[test]
    fn target_kind_validation() {
        assert!(valid_target_kind("bid"));
        assert!(valid_target_kind("order"));
        assert!(valid_target_kind("trade"));
        assert!(!valid_target_kind("listing"));
        assert!(!valid_target_kind(""));
    }
}
