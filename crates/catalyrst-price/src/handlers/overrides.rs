//! Admin price-override config store (docs/admin-console.md §4 "Price override").
//!
//! - `GET    /admin/api/price/overrides`             — list all overrides (public read).
//! - `PUT    /admin/api/price/overrides/{token}/{vs}` — set/upsert one (bearer-gated).
//! - `DELETE /admin/api/price/overrides/{token}/{vs}` — clear one (bearer-gated).
//!
//! Mutations are gated by a constant-time bearer compare against
//! `CATALYRST_PRICE_ADMIN_TOKEN`; the gate fails closed (403) when that env is
//! unset. This is additive: the public `/api/v3/simple/price` route is
//! unchanged.
//!
//! The override `value` is an exact NUMERIC carried as a decimal *string* —
//! never a JSON number / f64 (mirrors the credits crate's never-f64 stance).
//! Every successful set/clear writes an audit row attributed to the
//! console-set `X-Catalyrst-Admin` identity.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::auth::is_admin;
use crate::ports::overrides::PriceOverride;
use crate::AppState;

fn forbidden() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({ "error": "Forbidden" })),
    )
        .into_response()
}

fn bad_request(msg: &str) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))).into_response()
}

fn internal(err: sqlx::Error) -> Response {
    tracing::error!(%err, "price-override store query failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": "Internal Server Error" })),
    )
        .into_response()
}

fn override_json(o: &PriceOverride) -> serde_json::Value {
    json!({
        "token_id": o.token_id,
        "vs_currency": o.vs_currency,
        // exact NUMERIC, emitted as a decimal string (never a JSON number).
        "value": o.value,
        "note": o.note,
        "updated_by": o.updated_by,
        "updated_at": o.updated_at.to_rfc3339(),
    })
}

fn normalize(s: &str) -> String {
    s.trim().to_ascii_lowercase()
}

/// Validate a decimal override value given as a string. We require a string
/// (never a JSON number) so the value keeps full precision and never passes
/// through f64. Accepts an optional leading sign and a single decimal point;
/// rejects anything non-numeric. Returns the trimmed canonical string.
fn validate_decimal(raw: &str) -> Result<String, Response> {
    let s = raw.trim();
    if s.is_empty() || s.len() > 78 {
        return Err(bad_request("value must be a decimal number"));
    }
    let mut body = s;
    if let Some(rest) = body.strip_prefix(['+', '-']) {
        body = rest;
    }
    let mut seen_dot = false;
    let mut any_digit = false;
    for c in body.chars() {
        match c {
            '0'..='9' => any_digit = true,
            '.' if !seen_dot => seen_dot = true,
            _ => return Err(bad_request("value must be a decimal number")),
        }
    }
    if !any_digit {
        return Err(bad_request("value must be a decimal number"));
    }
    Ok(s.to_string())
}

/// Console-attributed admin identity for the audit trail. The trustworthy
/// source is the `X-Catalyrst-Admin` request header, set server-side by the
/// admin console (not the spoofable client body). Falls back to "console" when
/// the header is absent — these mutations are already bearer/AdminSession gated,
/// so an unattributed call is a direct console operator.
fn admin_identity(headers: &HeaderMap) -> String {
    headers
        .get("x-catalyrst-admin")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.chars().take(100).collect())
        .unwrap_or_else(|| "console".to_string())
}

/// `GET /admin/api/price/overrides` — list all overrides. Read-only, public.
pub async fn list_overrides(State(state): State<AppState>) -> Response {
    match state.overrides.list().await {
        Ok(rows) => {
            let items: Vec<_> = rows.iter().map(override_json).collect();
            Json(json!({ "overrides": items })).into_response()
        }
        Err(err) => internal(err),
    }
}

#[derive(Deserialize)]
pub struct SetOverrideBody {
    /// Exact decimal override value as a string (never a JSON number / f64).
    pub value: String,
    #[serde(default)]
    pub note: Option<String>,
    // NB: an `updated_by` body field is intentionally *not* honored — the admin
    // identity is taken from the console-set `X-Catalyrst-Admin` header so a
    // client cannot forge the audit attribution. Any such field is ignored.
}

/// `PUT /admin/api/price/overrides/{token}/{vs}` — set/upsert an override.
pub async fn set_override(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((token, vs)): Path<(String, String)>,
    Json(body): Json<SetOverrideBody>,
) -> Response {
    if !is_admin(&state, &headers) {
        return forbidden();
    }
    let value = match validate_decimal(&body.value) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let token = normalize(&token);
    let vs = normalize(&vs);
    if token.is_empty() || vs.is_empty() {
        return bad_request("token and vs_currency are required");
    }
    let admin = admin_identity(&headers);
    match state
        .overrides
        .set(&token, &vs, &value, body.note.as_deref(), &admin)
        .await
    {
        Ok(o) => (StatusCode::OK, Json(override_json(&o))).into_response(),
        Err(err) => internal(err),
    }
}

/// `DELETE /admin/api/price/overrides/{token}/{vs}` — clear an override.
pub async fn clear_override(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((token, vs)): Path<(String, String)>,
) -> Response {
    if !is_admin(&state, &headers) {
        return forbidden();
    }
    let token = normalize(&token);
    let vs = normalize(&vs);
    let admin = admin_identity(&headers);
    match state.overrides.clear(&token, &vs, &admin).await {
        Ok(removed) => (StatusCode::OK, Json(json!({ "removed": removed }))).into_response(),
        Err(err) => internal(err),
    }
}
