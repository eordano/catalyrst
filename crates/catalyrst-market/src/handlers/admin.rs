use axum::extract::{FromRef, FromRequestParts, Path, Query, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use catalyrst_authenticated_admin::{
    AdminAuthRejection, AuthenticatedAdminIdentity, ConfiguredAdminBearerSecret,
};
use catalyrst_authenticated_principal::AuthorityNotEstablished;

use crate::AppState;

const ADMIN_TOKEN_ENV: &str = "CATALYRST_MARKET_ADMIN_TOKEN";

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

#[derive(Clone)]
struct AdminSecretState(ConfiguredAdminBearerSecret);

impl FromRef<AdminSecretState> for ConfiguredAdminBearerSecret {
    fn from_ref(state: &AdminSecretState) -> Self {
        state.0.clone()
    }
}

pub struct RequireAdmin(());

impl RequireAdmin {
    fn actor(&self) -> String {
        "admin-token".to_string()
    }
}

fn to_admin_response(rejection: AdminAuthRejection) -> AdminResponse {
    match rejection.refusal() {
        AuthorityNotEstablished::CredentialNotConfigured { .. } => err(
            StatusCode::FORBIDDEN,
            "admin controls disabled (CATALYRST_MARKET_ADMIN_TOKEN unset)",
        ),
        _ => err(StatusCode::FORBIDDEN, "admin bearer token required"),
    }
}

async fn establish_admin(
    configured: Option<String>,
    parts: &mut Parts,
) -> Result<RequireAdmin, AdminResponse> {
    let carrier = AdminSecretState(ConfiguredAdminBearerSecret {
        environment_variable: ADMIN_TOKEN_ENV,
        configured,
    });
    match AuthenticatedAdminIdentity::from_request_parts(parts, &carrier).await {
        Ok(_) => Ok(RequireAdmin(())),
        Err(rejection) => Err(to_admin_response(rejection)),
    }
}

impl FromRequestParts<AppState> for RequireAdmin {
    type Rejection = AdminResponse;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        establish_admin(state.admin_token.clone(), parts).await
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
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
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

#[utoipa::path(
    post,
    path = "/v1/admin/moderation/{kind}/{hash}/flag",
    tag = "market-admin",
    params(("kind" = String, Path), ("hash" = String, Path)),
    request_body = serde_json::Value,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, body = crate::http::response::MarketErrorBody),
        (status = 403, body = crate::http::response::MarketErrorBody),
        (status = 404, body = crate::http::response::MarketErrorBody),
        (status = 500, body = crate::http::response::MarketErrorBody)
    )
)]
pub async fn set_flag(
    State(state): State<AppState>,
    admin: RequireAdmin,
    Path((kind, hash)): Path<(String, String)>,
    body: Option<Json<FlagBody>>,
) -> AdminResponse {
    let actor = admin.actor();
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

#[utoipa::path(
    delete,
    path = "/v1/admin/moderation/{kind}/{hash}/flag",
    tag = "market-admin",
    params(("kind" = String, Path), ("hash" = String, Path)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, body = crate::http::response::MarketErrorBody),
        (status = 403, body = crate::http::response::MarketErrorBody),
        (status = 500, body = crate::http::response::MarketErrorBody)
    )
)]
pub async fn clear_flag(
    State(state): State<AppState>,
    admin: RequireAdmin,
    Path((kind, hash)): Path<(String, String)>,
) -> AdminResponse {
    let actor = admin.actor();
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

#[utoipa::path(
    get,
    path = "/v1/admin/moderation/flags",
    tag = "market-admin",
    params(("kind" = Option<String>, Query), ("severity" = Option<String>, Query)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, body = crate::http::response::MarketErrorBody),
        (status = 500, body = crate::http::response::MarketErrorBody)
    )
)]
pub async fn list_flags(
    State(state): State<AppState>,
    _admin: RequireAdmin,
    Query(q): Query<ListFlagsQuery>,
) -> AdminResponse {
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

#[utoipa::path(
    post,
    path = "/v1/admin/disputes/{trade_hash}/open",
    tag = "market-admin",
    params(("trade_hash" = String, Path)),
    request_body = serde_json::Value,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, body = crate::http::response::MarketErrorBody),
        (status = 404, body = crate::http::response::MarketErrorBody),
        (status = 500, body = crate::http::response::MarketErrorBody)
    )
)]
pub async fn open_dispute(
    State(state): State<AppState>,
    admin: RequireAdmin,
    Path(trade_hash): Path<String>,
    body: Option<Json<OpenDisputeBody>>,
) -> AdminResponse {
    let actor = admin.actor();
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

#[utoipa::path(
    post,
    path = "/v1/admin/disputes/{trade_hash}/resolve",
    tag = "market-admin",
    params(("trade_hash" = String, Path)),
    request_body = serde_json::Value,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, body = crate::http::response::MarketErrorBody),
        (status = 403, body = crate::http::response::MarketErrorBody),
        (status = 404, body = crate::http::response::MarketErrorBody),
        (status = 500, body = crate::http::response::MarketErrorBody)
    )
)]
pub async fn resolve_dispute(
    State(state): State<AppState>,
    admin: RequireAdmin,
    Path(trade_hash): Path<String>,
    body: Option<Json<ResolveDisputeBody>>,
) -> AdminResponse {
    let actor = admin.actor();
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

#[utoipa::path(
    get,
    path = "/v1/admin/disputes",
    tag = "market-admin",
    params(("status" = Option<String>, Query)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, body = crate::http::response::MarketErrorBody),
        (status = 500, body = crate::http::response::MarketErrorBody)
    )
)]
pub async fn list_disputes(
    State(state): State<AppState>,
    _admin: RequireAdmin,
    Query(q): Query<ListDisputesQuery>,
) -> AdminResponse {
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

#[utoipa::path(
    post,
    path = "/v1/admin/listings/{kind}/{hash}/force-cancel",
    tag = "market-admin",
    params(("kind" = String, Path), ("hash" = String, Path)),
    request_body = serde_json::Value,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, body = crate::http::response::MarketErrorBody),
        (status = 403, body = crate::http::response::MarketErrorBody),
        (status = 404, body = crate::http::response::MarketErrorBody),
        (status = 500, body = crate::http::response::MarketErrorBody)
    )
)]
pub async fn force_cancel(
    State(state): State<AppState>,
    admin: RequireAdmin,
    Path((kind, hash)): Path<(String, String)>,
    body: Option<Json<ForceCancelBody>>,
) -> AdminResponse {
    let actor = admin.actor();
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

#[utoipa::path(
    get,
    path = "/v1/admin/audit",
    tag = "market-admin",
    params(("target_hash" = Option<String>, Query), ("action" = Option<String>, Query), ("limit" = Option<i64>, Query)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, body = crate::http::response::MarketErrorBody),
        (status = 500, body = crate::http::response::MarketErrorBody)
    )
)]
pub async fn list_audit(
    State(state): State<AppState>,
    _admin: RequireAdmin,
    Query(q): Query<AuditQuery>,
) -> AdminResponse {
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
#[path = "admin_tests.rs"]
mod tests;
