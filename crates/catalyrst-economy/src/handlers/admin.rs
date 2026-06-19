//! Bearer-gated admin controls for the relayer (docs/admin-console.md §4).
//!
//! Three routes, all gated by `CATALYRST_ECONOMY_ADMIN_TOKEN`:
//!   - `GET  /{api}/admin/relayer`        → current runtime + provisioning status
//!   - `POST /{api}/admin/relayer/toggle` → set the broadcast master switch
//!   - `POST /{api}/admin/relayer/signer` → switch the preferred provider
//!
//! The gate fails closed: if the token env is unset, every route returns 403
//! (the control surface is invisible / locked by default). The bearer compare is
//! timing-safe, mirroring `catalyrst-comms` `authorize_moderator`.

use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::admin::SignerPreference;
use crate::http::errors::ApiError;
use crate::AppState;

/// Constant-time string equality (mirrors catalyrst-comms `timing_safe_eq`).
fn timing_safe_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

/// Trim/cap a candidate admin-identity label; `None` if blank after trimming
/// (mirrors catalyrst-telemetry `clean_actor`).
fn clean_actor(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.chars().take(100).collect())
    }
}

/// Resolve the audit actor: the admin console sets the trusted `X-Catalyrst-Admin`
/// header server-side (not reachable from a spoofable browser fetch). When it is
/// absent we attribute the action to `"console"` — these mutations are already
/// bearer-gated, so an unlabelled call is a direct token-holding operator.
fn audit_actor(headers: &HeaderMap) -> String {
    headers
        .get("x-catalyrst-admin")
        .and_then(|v| v.to_str().ok())
        .and_then(clean_actor)
        .unwrap_or_else(|| "console".to_string())
}

/// Fail-closed admin gate: 403 unless `CATALYRST_ECONOMY_ADMIN_TOKEN` is set AND
/// the request carries a matching `Authorization: Bearer <token>`.
fn require_admin(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    check_bearer(state.config.admin_token.as_deref(), bearer_token(headers).as_deref())
}

/// Pure gate logic (testable without a full `AppState`): fail closed when the
/// configured token is `None`, and require a constant-time match otherwise.
fn check_bearer(expected: Option<&str>, presented: Option<&str>) -> Result<(), ApiError> {
    let Some(expected) = expected else {
        return Err(ApiError::Forbidden(
            "Admin controls are disabled (CATALYRST_ECONOMY_ADMIN_TOKEN is unset).".into(),
        ));
    };
    match presented {
        Some(token) if timing_safe_eq(token, expected) => Ok(()),
        _ => Err(ApiError::Forbidden(
            "Invalid or missing admin bearer token.".into(),
        )),
    }
}

fn status_json(state: &AppState) -> Value {
    let pref = state.runtime.signer_preference();
    json!({
        "ok": true,
        "relayerEnabled": state.runtime.relayer_enabled(),
        "signerPreference": pref,
        "provisioned": {
            "oz": state.transaction.has_oz_relayer(),
            "direct": state.transaction.has_direct_signer(),
        },
    })
}

/// `GET /{api}/admin/relayer` — report the live runtime controls + what is
/// provisioned, so the console can render the toggle/switch with current state.
pub async fn relayer_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    require_admin(&state, &headers)?;
    Ok(Json(status_json(&state)))
}

#[derive(Debug, Deserialize)]
pub struct ToggleBody {
    /// `true` = broadcasting on, `false` = paused (validation still runs; broadcast 503s).
    pub enabled: bool,
}

/// `POST /{api}/admin/relayer/toggle` — body `{ "enabled": bool }`. Flip the
/// broadcast master switch at runtime.
pub async fn relayer_toggle(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Json<ToggleBody>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&state, &headers)?;
    let actor = audit_actor(&headers);
    let Json(body) = body.map_err(|e| ApiError::MalformedBody(e.body_text()))?;
    let previous = state.runtime.relayer_enabled();
    state.runtime.set_relayer_enabled(body.enabled);
    // Append-only audit trail. State itself is process-local by design (no DB),
    // so the audit record lives in the structured log with `audit = true` for
    // log-pipeline filtering; it captures the admin identity + before/after.
    tracing::info!(
        audit = true,
        actor = %actor,
        action = "relayer.toggle",
        previous = previous,
        enabled = body.enabled,
        "admin: relayer broadcast toggle changed"
    );
    Ok(Json(status_json(&state)))
}

#[derive(Debug, Deserialize)]
pub struct SignerBody {
    /// One of `auto` | `oz` | `direct`.
    pub preference: SignerPreference,
}

/// `POST /{api}/admin/relayer/signer` — body `{ "preference": "auto"|"oz"|"direct" }`.
/// Switch which provisioned broadcast provider is preferred at runtime.
pub async fn relayer_signer(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Json<SignerBody>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&state, &headers)?;
    let actor = audit_actor(&headers);
    let Json(body) = body.map_err(|e| ApiError::MalformedBody(e.body_text()))?;
    let previous = state.runtime.signer_preference();
    state.runtime.set_signer_preference(body.preference);
    // Append-only audit trail (process-local state, log-borne audit record).
    tracing::info!(
        audit = true,
        actor = %actor,
        action = "relayer.signer",
        previous = ?previous,
        preference = ?body.preference,
        "admin: relayer signer preference changed"
    );
    Ok(Json(status_json(&state)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forbidden(r: Result<(), ApiError>) -> bool {
        matches!(r, Err(ApiError::Forbidden(_)))
    }

    #[test]
    fn unset_token_fails_closed() {
        // No configured token ⇒ 403 even when a bearer is presented.
        assert!(forbidden(check_bearer(None, None)));
        assert!(forbidden(check_bearer(None, Some("anything"))));
    }

    #[test]
    fn missing_or_wrong_bearer_is_forbidden() {
        assert!(forbidden(check_bearer(Some("secret"), None)));
        assert!(forbidden(check_bearer(Some("secret"), Some("wrong"))));
        // length-mismatch path
        assert!(forbidden(check_bearer(Some("secret"), Some("secretsecret"))));
    }

    #[test]
    fn matching_bearer_is_allowed() {
        assert!(check_bearer(Some("secret"), Some("secret")).is_ok());
    }

    #[test]
    fn bearer_token_parses_scheme() {
        let mut h = HeaderMap::new();
        h.insert("authorization", "Bearer tok123".parse().unwrap());
        assert_eq!(bearer_token(&h).as_deref(), Some("tok123"));

        let mut h2 = HeaderMap::new();
        h2.insert("authorization", "Basic tok123".parse().unwrap());
        assert_eq!(bearer_token(&h2), None);
    }

    #[test]
    fn clean_actor_trims_and_caps() {
        assert_eq!(clean_actor("  alice  ").as_deref(), Some("alice"));
        assert_eq!(clean_actor("   "), None);
        assert_eq!(clean_actor(""), None);
        let long: String = "x".repeat(250);
        assert_eq!(clean_actor(&long).map(|s| s.len()), Some(100));
    }

    #[test]
    fn audit_actor_prefers_header_then_falls_back_to_console() {
        // Absent header ⇒ "console" fallback.
        assert_eq!(audit_actor(&HeaderMap::new()), "console");

        // Trusted header ⇒ its (cleaned) value.
        let mut h = HeaderMap::new();
        h.insert("x-catalyrst-admin", "0xAdmin".parse().unwrap());
        assert_eq!(audit_actor(&h), "0xAdmin");

        // Blank header ⇒ "console" fallback.
        let mut h2 = HeaderMap::new();
        h2.insert("x-catalyrst-admin", "   ".parse().unwrap());
        assert_eq!(audit_actor(&h2), "console");
    }
}
