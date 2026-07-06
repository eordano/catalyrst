use axum::extract::{Path, Query};
use axum::http::HeaderMap;
use axum::Json;
use catalyrst_fed::{Scope, Signed, TypedMessage};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::fed::replay;
use crate::http::response::ApiError;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct FeedQuery {
    pub since: Option<i64>,
    pub limit: Option<i64>,
}

pub async fn get_feed(Query(_q): Query<FeedQuery>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({"events": [], "partial": false})))
}

pub async fn get_attendance(
    Path(event_id): Path<String>,
    Query(_q): Query<FeedQuery>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(
        json!({"event_id": event_id, "attendances": [], "partial": false}),
    ))
}

pub fn is_federation_envelope(body: &Value) -> bool {
    body.as_object()
        .map(|o| {
            o.contains_key("domain") && o.contains_key("message") && o.contains_key("signature")
        })
        .unwrap_or(false)
}

pub async fn preflight<T: TypedMessage + DeserializeOwned>(
    state: &AppState,
    headers: &HeaderMap,
    body: Value,
) -> Result<(Signed<T>, String), ApiError> {
    let signed: Signed<T> = serde_json::from_value(body).map_err(|e| {
        ApiError::bad_request(format!("invalid Signed<{}>: {}", T::PRIMARY_TYPE, e))
    })?;

    let signer = signed
        .signer()
        .map_err(|e| ApiError::unauthorized(format!("signature verify: {}", e)))?;
    if let Some(addr) = crate::auth_chain::try_extract(headers).map(|c| c.signer) {
        if !addr.eq_ignore_ascii_case(&signer) {
            return Err(ApiError::unauthorized(
                "auth-chain signer != envelope signer",
            ));
        }
    }
    let now = chrono::Utc::now().timestamp();
    signed
        .verify(&signer, now)
        .map_err(|e| ApiError::unauthorized(format!("signature verify: {}", e)))?;
    if !signed.domain.name.eq_ignore_ascii_case(&state.domain.name) {
        return Err(ApiError::bad_request(format!(
            "domain mismatch: expected {}",
            state.domain.name
        )));
    }
    replay::check_and_record(&state.pool, &signer, &signed.nonce, signed.signed_at)
        .await
        .map_err(|e| ApiError::bad_request(format!("replay: {}", e)))?;
    Ok((signed, signer))
}

pub async fn emit_gossip<T>(state: &AppState, signed: &Signed<T>, sig_hash: &str, signer: &str)
where
    T: TypedMessage + serde::Serialize,
{
    match catalyrst_fed::GossipEnvelope::local(
        Scope::Events,
        signed,
        sig_hash.to_string(),
        signer.to_ascii_lowercase(),
    ) {
        Ok(env) => {
            if let Err(e) = state.gossip.publish(&env).await {
                tracing::warn!(error = %e, signature_hash = %sig_hash, "events gossip publish failed (action durable; peers reconcile via snapshot)");
            }
        }
        Err(e) => tracing::warn!(error = %e, "failed to build events gossip envelope"),
    }
}

#[cfg(test)]
mod tests {
    use super::is_federation_envelope;
    use serde_json::json;

    #[test]
    fn detects_signed_envelope() {
        let env = json!({
            "domain": {}, "message": {}, "signature": "0x", "nonce": [], "signed_at": 1
        });
        assert!(is_federation_envelope(&env));
    }

    #[test]
    fn rejects_legacy_or_partial_bodies() {
        assert!(!is_federation_envelope(&json!({ "email": "a@b.c" })));
        assert!(!is_federation_envelope(
            &json!({ "domain": {}, "message": {} })
        ));
        assert!(!is_federation_envelope(&json!("not an object")));
    }
}
