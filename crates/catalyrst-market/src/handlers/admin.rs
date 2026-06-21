use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::AppState;

type AdminResponse = Response;
type FlagRow = (String, String, String, String, String, i64);
type DisputeRow = (
    String,
    String,
    String,
    String,
    String,
    i64,
    Option<String>,
    Option<i64>,
);
type AuditRow = (i64, String, String, String, String, Value, i64);

#[derive(Debug, Serialize)]
struct AdminError {
    ok: bool,
    message: String,
}

#[derive(Debug, Serialize)]
struct ListEnvelope<T> {
    data: Vec<T>,
    total: usize,
}

impl<T> ListEnvelope<T> {
    fn of(data: Vec<T>) -> Self {
        let total = data.len();
        Self { data, total }
    }
}

#[derive(Debug, Serialize)]
struct SetFlagResponse {
    ok: bool,
    target_kind: String,
    target_hash: String,
    severity: String,
}

#[derive(Debug, Serialize)]
struct ClearFlagResponse {
    ok: bool,
    target_hash: String,
    removed: bool,
}

#[derive(Debug, Serialize)]
struct FlagEntry {
    target_hash: String,
    target_kind: String,
    severity: String,
    reason: String,
    flagged_by: String,
    flagged_at: i64,
}

#[derive(Debug, Serialize)]
struct DisputeActionResponse {
    ok: bool,
    trade_hash: String,
    status: String,
}

#[derive(Debug, Serialize)]
struct DisputeEntry {
    trade_hash: String,
    status: String,
    reason: String,
    resolution: String,
    opened_by: String,
    opened_at: i64,
    resolved_by: Option<String>,
    resolved_at: Option<i64>,
}

#[derive(Debug, Serialize)]
struct ForceCancelResponse {
    ok: bool,
    target_hash: String,
    cancellation_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    already_cancelled: Option<bool>,
}

#[derive(Debug, Serialize)]
struct AuditEntry {
    id: i64,
    actor: String,
    action: String,
    target_kind: String,
    target_hash: String,
    detail: Value,
    created_at: i64,
}

#[derive(Debug, Serialize)]
struct FlagSetDetail<'a> {
    severity: &'a str,
    reason: &'a str,
}

#[derive(Debug, Serialize)]
struct EmptyDetail {}

#[derive(Debug, Serialize)]
struct ReasonDetail<'a> {
    reason: &'a str,
}

#[derive(Debug, Serialize)]
struct DisputeResolveDetail<'a> {
    status: &'a str,
    resolution: &'a str,
}

#[derive(Debug, Serialize)]
struct ForceCancelDetail<'a> {
    reason: &'a str,
    cancellation_hash: &'a str,
}

#[derive(Debug, Serialize)]
struct OperatorCancelPayload<'a> {
    operator_force_cancel: bool,
    actor: &'a str,
    reason: &'a str,
    target_kind: &'a str,
}

fn to_detail_value(detail: impl Serialize, context: &str) -> Value {
    serde_json::to_value(detail).unwrap_or_else(|e| {
        tracing::error!(error = %e, context, "admin detail serialization failed; storing null");
        Value::Null
    })
}

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
    (
        code,
        Json(AdminError {
            ok: false,
            message: message.into(),
        }),
    )
        .into_response()
}

#[allow(clippy::result_large_err)]
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

async fn write_audit(
    state: &AppState,
    actor: &str,
    action: &str,
    target_kind: &str,
    target_hash: &str,
    detail: impl Serialize,
) {
    let detail = to_detail_value(detail, action);
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

async fn target_exists(state: &AppState, kind: &str, hash: &str) -> Result<bool, sqlx::Error> {
    let table = match kind {
        "bid" => "market_bids_local",
        "order" => "market_orders_local",
        "trade" => "market_trades_local",
        _ => return Ok(false),
    };
    let row: Option<(i64,)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT 1 FROM {table} WHERE signature_hash = $1 LIMIT 1"
    )))
    .bind(hash)
    .fetch_optional(&state.pool)
    .await?;
    Ok(row.is_some())
}

#[derive(Debug, Default, Deserialize)]
pub struct FlagBody {
    #[serde(default)]
    pub severity: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

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
        return err(
            StatusCode::BAD_REQUEST,
            "severity must be review|hide|block",
        );
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
        FlagSetDetail {
            severity: &severity,
            reason: &reason,
        },
    )
    .await;
    (
        StatusCode::OK,
        Json(SetFlagResponse {
            ok: true,
            target_kind: kind,
            target_hash: hash,
            severity,
        }),
    )
        .into_response()
}

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
        write_audit(&state, &actor, "flag.clear", &kind, &hash, EmptyDetail {}).await;
    }
    (
        StatusCode::OK,
        Json(ClearFlagResponse {
            ok: true,
            target_hash: hash,
            removed,
        }),
    )
        .into_response()
}

#[derive(Debug, Default, Deserialize)]
pub struct ListFlagsQuery {
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub severity: Option<String>,
}

pub async fn list_flags(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListFlagsQuery>,
) -> AdminResponse {
    if let Err(e) = require_admin(&state, &headers) {
        return e;
    }
    let rows: Result<Vec<FlagRow>, _> = sqlx::query_as(
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
            let data: Vec<FlagEntry> = rows
                .into_iter()
                .map(
                    |(target_hash, target_kind, severity, reason, flagged_by, flagged_at)| {
                        FlagEntry {
                            target_hash,
                            target_kind,
                            severity,
                            reason,
                            flagged_by,
                            flagged_at,
                        }
                    },
                )
                .collect();
            (StatusCode::OK, Json(ListEnvelope::of(data))).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "list_flags failed");
            err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct OpenDisputeBody {
    #[serde(default)]
    pub reason: Option<String>,
}

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
        ReasonDetail { reason: &reason },
    )
    .await;
    (
        StatusCode::OK,
        Json(DisputeActionResponse {
            ok: true,
            trade_hash,
            status: "open".to_string(),
        }),
    )
        .into_response()
}

#[derive(Debug, Default, Deserialize)]
pub struct ResolveDisputeBody {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub resolution: Option<String>,
}

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
        DisputeResolveDetail {
            status: &status,
            resolution: &resolution,
        },
    )
    .await;
    (
        StatusCode::OK,
        Json(DisputeActionResponse {
            ok: true,
            trade_hash,
            status,
        }),
    )
        .into_response()
}

#[derive(Debug, Default, Deserialize)]
pub struct ListDisputesQuery {
    #[serde(default)]
    pub status: Option<String>,
}

pub async fn list_disputes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListDisputesQuery>,
) -> AdminResponse {
    if let Err(e) = require_admin(&state, &headers) {
        return e;
    }
    let rows: Result<Vec<DisputeRow>, _> =
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
            let data: Vec<DisputeEntry> = rows
                .into_iter()
                .map(
                    |(
                        trade_hash,
                        status,
                        reason,
                        resolution,
                        opened_by,
                        opened_at,
                        resolved_by,
                        resolved_at,
                    )| {
                        DisputeEntry {
                            trade_hash,
                            status,
                            reason,
                            resolution,
                            opened_by,
                            opened_at,
                            resolved_by,
                            resolved_at,
                        }
                    },
                )
                .collect();
            (StatusCode::OK, Json(ListEnvelope::of(data))).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "list_disputes failed");
            err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct ForceCancelBody {
    #[serde(default)]
    pub reason: Option<String>,
}

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
                Json(ForceCancelResponse {
                    ok: true,
                    target_hash: hash,
                    cancellation_hash: sig,
                    already_cancelled: Some(true),
                }),
            )
                .into_response();
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!(error = %e, "force_cancel existing check failed");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "database error");
        }
    }

    let now = now_secs();

    let mut h = Sha256::new();
    h.update(b"operator-force-cancel:");
    h.update(kind.as_bytes());
    h.update(b":");
    h.update(hash.as_bytes());
    h.update(b":");
    h.update(now.to_le_bytes());
    let cancellation_hash = format!("operator:{}", hex::encode(h.finalize()));
    let operator_signer = format!("operator:{actor}");
    let payload = to_detail_value(
        OperatorCancelPayload {
            operator_force_cancel: true,
            actor: &actor,
            reason: &reason,
            target_kind: &kind,
        },
        "force_cancel.payload",
    );

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
        ForceCancelDetail {
            reason: &reason,
            cancellation_hash: &cancellation_hash,
        },
    )
    .await;
    (
        StatusCode::OK,
        Json(ForceCancelResponse {
            ok: true,
            target_hash: hash,
            cancellation_hash,
            already_cancelled: None,
        }),
    )
        .into_response()
}

#[derive(Debug, Default, Deserialize)]
pub struct AuditQuery {
    #[serde(default)]
    pub target_hash: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
}

pub async fn list_audit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<AuditQuery>,
) -> AdminResponse {
    if let Err(e) = require_admin(&state, &headers) {
        return e;
    }
    let limit = q.limit.unwrap_or(200).clamp(1, 1000);
    let rows: Result<Vec<AuditRow>, _> = sqlx::query_as(
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
            let data: Vec<AuditEntry> = rows
                .into_iter()
                .map(
                    |(id, actor, action, target_kind, target_hash, detail, created_at)| {
                        AuditEntry {
                            id,
                            actor,
                            action,
                            target_kind,
                            target_hash,
                            detail,
                            created_at,
                        }
                    },
                )
                .collect();
            (StatusCode::OK, Json(ListEnvelope::of(data))).into_response()
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
    use serde_json::json;

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

    #[test]
    fn wire_identity_error_envelope() {
        let dto = AdminError {
            ok: false,
            message: "admin bearer token required".to_string(),
        };
        assert_eq!(
            serde_json::to_value(&dto).unwrap(),
            json!({ "ok": false, "message": "admin bearer token required" })
        );

        let dto = AdminError {
            ok: false,
            message: "admin controls disabled (CATALYRST_MARKET_ADMIN_TOKEN unset)".to_string(),
        };
        assert_eq!(
            serde_json::to_value(&dto).unwrap(),
            json!({
                "ok": false,
                "message": "admin controls disabled (CATALYRST_MARKET_ADMIN_TOKEN unset)"
            })
        );
    }

    #[test]
    fn wire_identity_set_flag_ok() {
        let dto = SetFlagResponse {
            ok: true,
            target_kind: "bid".to_string(),
            target_hash: "0xabc".to_string(),
            severity: "hide".to_string(),
        };
        assert_eq!(
            serde_json::to_value(&dto).unwrap(),
            json!({ "ok": true, "target_kind": "bid", "target_hash": "0xabc", "severity": "hide" })
        );
    }

    #[test]
    fn wire_identity_clear_flag_ok() {
        let removed = ClearFlagResponse {
            ok: true,
            target_hash: "0xabc".to_string(),
            removed: true,
        };
        assert_eq!(
            serde_json::to_value(&removed).unwrap(),
            json!({ "ok": true, "target_hash": "0xabc", "removed": true })
        );

        let noop = ClearFlagResponse {
            ok: true,
            target_hash: "0xdef".to_string(),
            removed: false,
        };
        assert_eq!(
            serde_json::to_value(&noop).unwrap(),
            json!({ "ok": true, "target_hash": "0xdef", "removed": false })
        );
    }

    #[test]
    fn wire_identity_list_flags() {
        let entry = FlagEntry {
            target_hash: "0xabc".to_string(),
            target_kind: "order".to_string(),
            severity: "review".to_string(),
            reason: "spam".to_string(),
            flagged_by: "admin-token".to_string(),
            flagged_at: 1_700_000_000,
        };
        let dto = ListEnvelope::of(vec![entry]);
        assert_eq!(
            serde_json::to_value(&dto).unwrap(),
            json!({
                "data": [{
                    "target_hash": "0xabc",
                    "target_kind": "order",
                    "severity": "review",
                    "reason": "spam",
                    "flagged_by": "admin-token",
                    "flagged_at": 1_700_000_000_i64,
                }],
                "total": 1
            })
        );

        let empty: ListEnvelope<FlagEntry> = ListEnvelope::of(vec![]);
        assert_eq!(
            serde_json::to_value(&empty).unwrap(),
            json!({ "data": [], "total": 0 })
        );
    }

    #[test]
    fn wire_identity_dispute_action() {
        let opened = DisputeActionResponse {
            ok: true,
            trade_hash: "0xtrade".to_string(),
            status: "open".to_string(),
        };
        assert_eq!(
            serde_json::to_value(&opened).unwrap(),
            json!({ "ok": true, "trade_hash": "0xtrade", "status": "open" })
        );

        for status in ["resolved", "rejected"] {
            let dto = DisputeActionResponse {
                ok: true,
                trade_hash: "0xtrade".to_string(),
                status: status.to_string(),
            };
            assert_eq!(
                serde_json::to_value(&dto).unwrap(),
                json!({ "ok": true, "trade_hash": "0xtrade", "status": status })
            );
        }
    }

    #[test]
    fn wire_identity_list_disputes() {
        let open = DisputeEntry {
            trade_hash: "0xtrade".to_string(),
            status: "open".to_string(),
            reason: "fraud".to_string(),
            resolution: String::new(),
            opened_by: "admin-token".to_string(),
            opened_at: 1_700_000_000,
            resolved_by: None,
            resolved_at: None,
        };
        let v = serde_json::to_value(&open).unwrap();
        assert_eq!(
            v,
            json!({
                "trade_hash": "0xtrade",
                "status": "open",
                "reason": "fraud",
                "resolution": "",
                "opened_by": "admin-token",
                "opened_at": 1_700_000_000_i64,
                "resolved_by": null,
                "resolved_at": null,
            })
        );
        let obj = v.as_object().unwrap();
        assert!(obj.contains_key("resolved_by"));
        assert!(obj.contains_key("resolved_at"));

        let resolved = DisputeEntry {
            trade_hash: "0xtrade".to_string(),
            status: "resolved".to_string(),
            reason: "fraud".to_string(),
            resolution: "refunded".to_string(),
            opened_by: "admin-token".to_string(),
            opened_at: 1_700_000_000,
            resolved_by: Some("admin-token".to_string()),
            resolved_at: Some(1_700_000_100),
        };
        let dto = ListEnvelope::of(vec![resolved]);
        assert_eq!(
            serde_json::to_value(&dto).unwrap(),
            json!({
                "data": [{
                    "trade_hash": "0xtrade",
                    "status": "resolved",
                    "reason": "fraud",
                    "resolution": "refunded",
                    "opened_by": "admin-token",
                    "opened_at": 1_700_000_000_i64,
                    "resolved_by": "admin-token",
                    "resolved_at": 1_700_000_100_i64,
                }],
                "total": 1
            })
        );

        let empty: ListEnvelope<DisputeEntry> = ListEnvelope::of(vec![]);
        assert_eq!(
            serde_json::to_value(&empty).unwrap(),
            json!({ "data": [], "total": 0 })
        );
    }

    #[test]
    fn wire_identity_force_cancel() {
        let fresh = ForceCancelResponse {
            ok: true,
            target_hash: "0xh".to_string(),
            cancellation_hash: "operator:deadbeef".to_string(),
            already_cancelled: None,
        };
        let v = serde_json::to_value(&fresh).unwrap();
        assert_eq!(
            v,
            json!({ "ok": true, "target_hash": "0xh", "cancellation_hash": "operator:deadbeef" })
        );
        assert!(!v.as_object().unwrap().contains_key("already_cancelled"));

        let replay = ForceCancelResponse {
            ok: true,
            target_hash: "0xh".to_string(),
            cancellation_hash: "operator:prior".to_string(),
            already_cancelled: Some(true),
        };
        assert_eq!(
            serde_json::to_value(&replay).unwrap(),
            json!({
                "ok": true,
                "target_hash": "0xh",
                "cancellation_hash": "operator:prior",
                "already_cancelled": true,
            })
        );
    }

    #[test]
    fn wire_identity_list_audit() {
        let entry = AuditEntry {
            id: 42,
            actor: "admin-token".to_string(),
            action: "flag.set".to_string(),
            target_kind: "bid".to_string(),
            target_hash: "0xabc".to_string(),
            detail: json!({ "severity": "hide", "reason": "spam", "legacy_extra": [1, 2] }),
            created_at: 1_700_000_000,
        };
        let dto = ListEnvelope::of(vec![entry]);
        assert_eq!(
            serde_json::to_value(&dto).unwrap(),
            json!({
                "data": [{
                    "id": 42,
                    "actor": "admin-token",
                    "action": "flag.set",
                    "target_kind": "bid",
                    "target_hash": "0xabc",
                    "detail": { "severity": "hide", "reason": "spam", "legacy_extra": [1, 2] },
                    "created_at": 1_700_000_000_i64,
                }],
                "total": 1
            })
        );

        let empty: ListEnvelope<AuditEntry> = ListEnvelope::of(vec![]);
        assert_eq!(
            serde_json::to_value(&empty).unwrap(),
            json!({ "data": [], "total": 0 })
        );
    }

    #[test]
    fn wire_identity_audit_details() {
        assert_eq!(
            to_detail_value(
                FlagSetDetail {
                    severity: "hide",
                    reason: "spam"
                },
                "test"
            ),
            json!({ "severity": "hide", "reason": "spam" })
        );
        assert_eq!(to_detail_value(EmptyDetail {}, "test"), json!({}));
        assert_eq!(
            to_detail_value(ReasonDetail { reason: "fraud" }, "test"),
            json!({ "reason": "fraud" })
        );
        assert_eq!(
            to_detail_value(
                DisputeResolveDetail {
                    status: "resolved",
                    resolution: "refunded"
                },
                "test"
            ),
            json!({ "status": "resolved", "resolution": "refunded" })
        );
        assert_eq!(
            to_detail_value(
                ForceCancelDetail {
                    reason: "rug",
                    cancellation_hash: "operator:deadbeef"
                },
                "test"
            ),
            json!({ "reason": "rug", "cancellation_hash": "operator:deadbeef" })
        );
        assert_eq!(
            to_detail_value(
                OperatorCancelPayload {
                    operator_force_cancel: true,
                    actor: "admin-token",
                    reason: "rug",
                    target_kind: "order",
                },
                "test"
            ),
            json!({
                "operator_force_cancel": true,
                "actor": "admin-token",
                "reason": "rug",
                "target_kind": "order",
            })
        );
    }
}
