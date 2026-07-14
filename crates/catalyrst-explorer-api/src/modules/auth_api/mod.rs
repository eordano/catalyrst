use crate::modules::admin_auth::require_admin;
use crate::AppState;
use axum::body::Body;
use axum::extract::{Path, Request, State};
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Duration, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use catalyrst_types::AuthChain;

mod validation;

use validation::{validate_auth_chain, validate_request_message};

const REQUEST_TTL_SECONDS: i64 = 300;

pub struct AuthApiState {
    pub requests: DashMap<String, ChallengeRecord>,
    pub identities: DashMap<String, IdentityRecord>,
    pub identity_status: DashMap<String, IdentityStatus>,
}

impl Default for AuthApiState {
    fn default() -> Self {
        Self {
            requests: DashMap::new(),
            identities: DashMap::new(),
            identity_status: DashMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct IdentityStatus {
    pub expiration: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub consumed: bool,
    pub signer: String,
    pub deletion_reason: Option<DeletionReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeletionReason {
    Consumed,
    Expired,
    IpMismatch,
}

#[derive(Debug, Clone)]
pub struct ChallengeRecord {
    pub request_id: String,
    pub method: String,
    pub params: Vec<Value>,
    pub sender: Option<String>,
    pub code: u32,
    pub challenge: String,
    pub created_at: DateTime<Utc>,
    pub expiration: DateTime<Utc>,
    pub requires_validation: bool,
    pub status: ChallengeStatus,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum ChallengeStatus {
    Pending,
    Signed { outcome: OutcomeResponseMessage },
    Fulfilled,
}

#[derive(Debug, Clone)]
pub struct IdentityRecord {
    pub identity_id: String,
    pub identity: Value,
    pub ip_address: String,
    pub is_mobile: bool,
    pub created_at: DateTime<Utc>,
    pub expiration: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateRequestBody {
    pub method: String,
    #[serde(default)]
    pub params: Option<Vec<Value>>,
    #[serde(rename = "authChain", default)]
    pub auth_chain: Option<AuthChain>,
    #[serde(flatten)]
    pub unknown_fields: Map<String, Value>,
}

#[derive(Debug, Serialize)]
pub struct CreateRequestResponse {
    #[serde(rename = "requestId")]
    pub request_id: String,
    pub expiration: DateTime<Utc>,
    pub code: u32,
    pub challenge: String,
}

#[derive(Debug, Serialize)]
pub struct RecoverResponse {
    pub expiration: DateTime<Utc>,
    pub code: u32,
    pub method: String,
    pub params: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender: Option<String>,
    pub challenge: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OutcomeError {
    pub code: i32,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HttpOutcomeMessage {
    pub sender: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<OutcomeError>,
    #[serde(rename = "authChain", default, skip_serializing_if = "Option::is_none")]
    pub auth_chain: Option<AuthChain>,
}

#[derive(Debug, Serialize, Clone)]
pub struct OutcomeResponseMessage {
    #[serde(rename = "requestId")]
    pub request_id: String,
    pub sender: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<OutcomeError>,
    #[serde(rename = "authChain", skip_serializing_if = "Option::is_none")]
    pub auth_chain: Option<AuthChain>,
}

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub error: String,
}

#[derive(Debug, Serialize)]
pub struct EvictedBody {
    pub error: String,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ValidationStatusResponse {
    #[serde(rename = "requiresValidation")]
    pub requires_validation: bool,
}

#[derive(Debug, Deserialize)]
pub struct IdentityRequestBody {
    pub identity: Value,
    #[serde(rename = "isMobile", default)]
    pub is_mobile: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct IdentityResponse {
    #[serde(rename = "identityId")]
    pub identity_id: String,
    pub expiration: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct IdentityIdValidationResponse {
    pub identity: Value,
}

#[derive(Debug, Serialize)]
pub struct LiveResponse {
    pub timestamp: i64,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/health/live", get(health_live))
        .route("/auth/requests", post(create_request))
        .route("/auth/requests/{id}", get(get_request_outcome))
        .route("/auth/requests/{id}", post(post_request_outcome_legacy))
        .route("/auth/v2/requests/{id}", get(get_request_v2))
        .route("/auth/v2/requests/{id}/outcome", post(post_request_outcome))
        .route(
            "/auth/v2/requests/{id}/validation",
            post(post_request_validation),
        )
        .route(
            "/auth/v2/requests/{id}/validation",
            get(get_request_validation),
        )
        .route("/auth/identities", post(create_identity))
        .route("/auth/identities/{id}", get(get_identity))
        .route("/admin/auth/challenges", get(admin_list_challenges))
        .route("/admin/auth/challenges/{id}", get(admin_get_challenge))
        .route(
            "/admin/auth/challenges/{id}/revoke",
            post(admin_revoke_challenge),
        )
        .route("/admin/auth/identities", get(admin_list_identities))
        .route(
            "/admin/auth/identities/{id}/revoke",
            post(admin_revoke_identity),
        )
        .layer(middleware::from_fn(cors_preflight))
}

// Preflight for the browser SPA: answer OPTIONS with the allowed methods and
// headers. Access-Control-Allow-Origin is added by nginx on the auth-api
// vhost; setting it here as well would duplicate the header and break CORS.
async fn cors_preflight(req: Request, next: Next) -> Response {
    if req.method() == Method::OPTIONS {
        return Response::builder()
            .status(StatusCode::NO_CONTENT)
            .header(header::ACCESS_CONTROL_ALLOW_METHODS, "GET, POST, OPTIONS")
            .header(header::ACCESS_CONTROL_ALLOW_HEADERS, "content-type")
            .header(header::ACCESS_CONTROL_MAX_AGE, "3600")
            .body(Body::empty())
            .expect("static preflight response");
    }
    next.run(req).await
}

fn challenge_view(record: &ChallengeRecord) -> Value {
    let status = match &record.status {
        ChallengeStatus::Pending => "pending",
        ChallengeStatus::Signed { .. } => "signed",
        ChallengeStatus::Fulfilled => "fulfilled",
    };
    json!({
        "requestId": record.request_id,
        "method": record.method,
        "sender": record.sender,
        "code": record.code,
        "createdAt": record.created_at,
        "expiration": record.expiration,
        "requiresValidation": record.requires_validation,
        "status": status,
    })
}

async fn admin_list_challenges(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(resp) = require_admin(&headers) {
        return resp;
    }
    let items: Vec<Value> = state
        .auth_api
        .requests
        .iter()
        .map(|e| challenge_view(e.value()))
        .collect();
    (
        StatusCode::OK,
        Json(json!({ "count": items.len(), "challenges": items })),
    )
        .into_response()
}

async fn admin_get_challenge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if let Err(resp) = require_admin(&headers) {
        return resp;
    }
    match state.auth_api.requests.get(&id) {
        Some(entry) => (StatusCode::OK, Json(challenge_view(entry.value()))).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "challenge_not_found", "requestId": id })),
        )
            .into_response(),
    }
}

async fn admin_revoke_challenge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if let Err(resp) = require_admin(&headers) {
        return resp;
    }
    let removed = state.auth_api.requests.remove(&id).is_some();
    let status = if removed {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    };
    (
        status,
        Json(json!({ "ok": removed, "requestId": id, "revoked": removed })),
    )
        .into_response()
}

async fn admin_list_identities(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(resp) = require_admin(&headers) {
        return resp;
    }
    let items: Vec<Value> = state
        .auth_api
        .identities
        .iter()
        .map(|e| {
            let r = e.value();
            json!({
                "identityId": r.identity_id,
                "ipAddress": r.ip_address,
                "isMobile": r.is_mobile,
                "createdAt": r.created_at,
                "expiration": r.expiration,
            })
        })
        .collect();
    (
        StatusCode::OK,
        Json(json!({ "count": items.len(), "identities": items })),
    )
        .into_response()
}

async fn admin_revoke_identity(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if let Err(resp) = require_admin(&headers) {
        return resp;
    }
    let removed = state.auth_api.identities.remove(&id).is_some();
    let status = if removed {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    };
    (
        status,
        Json(json!({ "ok": removed, "identityId": id, "revoked": removed })),
    )
        .into_response()
}

async fn health_live() -> impl IntoResponse {
    Json(LiveResponse {
        timestamp: Utc::now().timestamp_millis(),
    })
}

async fn create_request(
    State(state): State<AppState>,
    Json(body): Json<CreateRequestBody>,
) -> Response {
    let validated = match validate_request_message(&body) {
        Ok(validated) => validated,
        Err(msg) => return bad_request(&msg),
    };
    let sender = match validate_auth_chain(validated.auth_chain) {
        Ok(sender) => Some(sender.to_lowercase()),
        Err(msg) => return bad_request(&msg),
    };

    let request_id = Uuid::new_v4().to_string();
    let now = Utc::now();
    let expiration = now + Duration::seconds(REQUEST_TTL_SECONDS);
    let code = random_pairing_code();
    let challenge = build_challenge(&request_id, code, &now, &body.method, validated.params);

    let record = ChallengeRecord {
        request_id: request_id.clone(),
        method: body.method.clone(),
        params: validated.params.to_vec(),
        sender,
        code,
        challenge: challenge.clone(),
        created_at: now,
        expiration,
        requires_validation: false,
        status: ChallengeStatus::Pending,
    };

    state.auth_api.requests.insert(request_id.clone(), record);

    (
        StatusCode::CREATED,
        Json(CreateRequestResponse {
            request_id,
            expiration,
            code,
            challenge,
        }),
    )
        .into_response()
}

async fn get_request_v2(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let now = Utc::now();
    if let Some(reason) = evict_if_expired(&state, &id, now) {
        return reason;
    }
    let Some(entry) = state.auth_api.requests.get(&id) else {
        return not_found(&id);
    };
    let record = entry.value();
    if matches!(record.status, ChallengeStatus::Fulfilled) {
        return gone_already_fulfilled(&id);
    }
    let body = RecoverResponse {
        expiration: record.expiration,
        code: record.code,
        method: record.method.clone(),
        params: record.params.clone(),
        sender: record.sender.clone(),
        challenge: record.challenge.clone(),
    };
    (StatusCode::OK, Json(body)).into_response()
}

async fn get_request_outcome(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let now = Utc::now();
    if let Some(reason) = evict_if_expired(&state, &id, now) {
        return reason;
    }

    let outcome_opt = {
        let Some(entry) = state.auth_api.requests.get(&id) else {
            return not_found(&id);
        };
        let record = entry.value();
        match &record.status {
            ChallengeStatus::Fulfilled => return gone_already_fulfilled(&id),
            ChallengeStatus::Pending => None,
            ChallengeStatus::Signed { outcome } => Some(outcome.clone()),
        }
    };

    match outcome_opt {
        None => StatusCode::NO_CONTENT.into_response(),
        Some(outcome) => {
            if let Some(mut entry) = state.auth_api.requests.get_mut(&id) {
                entry.status = ChallengeStatus::Fulfilled;
            }
            (StatusCode::OK, Json(outcome)).into_response()
        }
    }
}

async fn post_request_outcome(
    state: State<AppState>,
    path: Path<String>,
    body: Json<HttpOutcomeMessage>,
) -> Response {
    submit_outcome(state, path, body).await
}

async fn post_request_outcome_legacy(
    state: State<AppState>,
    path: Path<String>,
    body: Json<HttpOutcomeMessage>,
) -> Response {
    submit_outcome(state, path, body).await
}

async fn submit_outcome(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<HttpOutcomeMessage>,
) -> Response {
    if let Err(msg) = validate_outcome_body(&body) {
        return bad_request(&msg);
    }

    if let Some(chain) = body.auth_chain.as_ref() {
        if let Err(msg) = validate_auth_chain(chain) {
            return bad_request(&msg);
        }
    }

    let now = Utc::now();
    if let Some(reason) = evict_if_expired(&state, &id, now) {
        return reason;
    }

    let Some(mut entry) = state.auth_api.requests.get_mut(&id) else {
        return not_found(&id);
    };
    let record = entry.value_mut();

    match record.status {
        ChallengeStatus::Fulfilled => return gone_already_fulfilled(&id),
        ChallengeStatus::Signed { .. } => {
            return bad_request(&format!(
                "Request with id \"{}\" already has a response",
                id
            ));
        }
        ChallengeStatus::Pending => {}
    }

    let outcome = OutcomeResponseMessage {
        request_id: id.clone(),
        sender: body.sender.to_lowercase(),
        result: body.result,
        error: body.error,
        auth_chain: body.auth_chain,
    };
    record.status = ChallengeStatus::Signed { outcome };

    StatusCode::OK.into_response()
}

async fn post_request_validation(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    let now = Utc::now();
    if let Some(reason) = evict_if_expired(&state, &id, now) {
        return reason;
    }
    let Some(mut entry) = state.auth_api.requests.get_mut(&id) else {
        return not_found(&id);
    };
    let record = entry.value_mut();
    if matches!(record.status, ChallengeStatus::Fulfilled) {
        return gone_already_fulfilled(&id);
    }
    record.requires_validation = true;
    StatusCode::NO_CONTENT.into_response()
}

async fn get_request_validation(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let now = Utc::now();
    if let Some(reason) = evict_if_expired(&state, &id, now) {
        return reason;
    }
    let Some(entry) = state.auth_api.requests.get(&id) else {
        return not_found(&id);
    };
    let record = entry.value();
    if matches!(record.status, ChallengeStatus::Fulfilled) {
        return gone_already_fulfilled(&id);
    }
    (
        StatusCode::OK,
        Json(ValidationStatusResponse {
            requires_validation: record.requires_validation,
        }),
    )
        .into_response()
}

async fn create_identity(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<IdentityRequestBody>,
) -> Response {
    if body.identity.is_null() {
        return bad_request("AuthIdentity is required in request body");
    }
    let signer = identity_owner(&body.identity).unwrap_or_default();
    let identity_id = Uuid::new_v4().to_string();
    let now = Utc::now();
    let expiration = now + Duration::seconds(3600);
    let record = IdentityRecord {
        identity_id: identity_id.clone(),
        identity: body.identity,
        ip_address: client_ip(&headers),
        is_mobile: body.is_mobile.unwrap_or(false),
        created_at: now,
        expiration,
    };
    state
        .auth_api
        .identities
        .insert(identity_id.clone(), record);
    state.auth_api.identity_status.insert(
        identity_id.clone(),
        IdentityStatus {
            expiration,
            created_at: now,
            consumed: false,
            signer,
            deletion_reason: None,
        },
    );
    (
        StatusCode::CREATED,
        Json(IdentityResponse {
            identity_id,
            expiration,
        }),
    )
        .into_response()
}

async fn get_identity(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if !is_valid_uuid(&id) {
        return bad_request("Invalid identity format");
    }
    let now = Utc::now();

    let Some((_, record)) = state.auth_api.identities.remove(&id) else {
        return identity_status_response(&state, &id);
    };

    let signer = identity_owner(&record.identity).unwrap_or_default();

    if record.expiration < now {
        update_identity_status(
            &state,
            &id,
            false,
            Some(DeletionReason::Expired),
            &signer,
            now,
        );
        return (
            StatusCode::GONE,
            Json(ErrorBody {
                error: "Identity has expired".into(),
            }),
        )
            .into_response();
    }

    let request_ip = client_ip(&headers);
    if !record.is_mobile
        && !record.ip_address.is_empty()
        && !request_ip.is_empty()
        && !ips_match(&record.ip_address, &request_ip)
    {
        update_identity_status(
            &state,
            &id,
            false,
            Some(DeletionReason::IpMismatch),
            &signer,
            now,
        );
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorBody {
                error: "IP address mismatch".into(),
            }),
        )
            .into_response();
    }
    if !record.is_mobile
        && !record.ip_address.is_empty()
        && !request_ip.is_empty()
        && normalize_ip(&record.ip_address) != normalize_ip(&request_ip)
    {
        tracing::warn!(
            stored = %record.ip_address,
            request = %request_ip,
            %signer,
            "identity IP matched only by /24 prefix"
        );
    }

    update_identity_status(
        &state,
        &id,
        true,
        Some(DeletionReason::Consumed),
        &signer,
        now,
    );
    (
        StatusCode::OK,
        Json(IdentityIdValidationResponse {
            identity: record.identity,
        }),
    )
        .into_response()
}

fn identity_status_response(state: &AppState, id: &str) -> Response {
    let Some(status) = state
        .auth_api
        .identity_status
        .get(id)
        .map(|e| e.value().clone())
    else {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorBody {
                error: "Identity not found".into(),
            }),
        )
            .into_response();
    };
    match status.deletion_reason {
        Some(DeletionReason::Consumed) => (
            StatusCode::NOT_FOUND,
            Json(ErrorBody {
                error: "Identity was already consumed".into(),
            }),
        )
            .into_response(),
        Some(DeletionReason::Expired) => (
            StatusCode::NOT_FOUND,
            Json(ErrorBody {
                error: "Identity has expired".into(),
            }),
        )
            .into_response(),
        Some(DeletionReason::IpMismatch) => (
            StatusCode::NOT_FOUND,
            Json(ErrorBody {
                error: "Identity was deleted due to IP mismatch".into(),
            }),
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(EvictedBody {
                error: "Identity was evicted".into(),
                created_at: status.created_at,
            }),
        )
            .into_response(),
    }
}

fn update_identity_status(
    state: &AppState,
    id: &str,
    consumed: bool,
    reason: Option<DeletionReason>,
    signer: &str,
    now: DateTime<Utc>,
) {
    if let Some(mut entry) = state.auth_api.identity_status.get_mut(id) {
        entry.consumed = consumed;
        entry.deletion_reason = reason;
    } else {
        state.auth_api.identity_status.insert(
            id.to_string(),
            IdentityStatus {
                expiration: now,
                created_at: now,
                consumed,
                signer: signer.to_string(),
                deletion_reason: reason,
            },
        );
    }
}

fn identity_owner(identity: &Value) -> Option<String> {
    identity
        .get("authChain")
        .and_then(|c| c.as_array())
        .and_then(|links| links.first())
        .and_then(|link| link.get("payload"))
        .and_then(|p| p.as_str())
        .map(|s| s.to_lowercase())
}

fn random_pairing_code() -> u32 {
    loop {
        let bytes = *Uuid::new_v4().as_bytes();
        for (i, byte) in bytes.into_iter().enumerate() {
            if i == 6 || i == 8 {
                continue;
            }
            if byte < 200 {
                return u32::from(byte % 100);
            }
        }
    }
}

fn build_challenge(
    request_id: &str,
    code: u32,
    now: &DateTime<Utc>,
    method: &str,
    params: &[Value],
) -> String {
    let params_blob = serde_json::to_string(params).unwrap_or_else(|_| "[]".into());
    format!(
        "Decentraland Login\nRequest: {}\nCode: {}\nMethod: {}\nParams: {}\nIssued: {}",
        request_id,
        code,
        method,
        params_blob,
        now.to_rfc3339()
    )
}

fn evict_if_expired(state: &AppState, id: &str, now: DateTime<Utc>) -> Option<Response> {
    let expired = state
        .auth_api
        .requests
        .get(id)
        .map(|e| e.value().expiration < now)
        .unwrap_or(false);
    if expired {
        state.auth_api.requests.remove(id);
        Some(
            (
                StatusCode::GONE,
                Json(ErrorBody {
                    error: format!("Request with id \"{}\" has expired", id),
                }),
            )
                .into_response(),
        )
    } else {
        None
    }
}

fn not_found(id: &str) -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorBody {
            error: format!("Request with id \"{}\" not found", id),
        }),
    )
        .into_response()
}

fn gone_already_fulfilled(id: &str) -> Response {
    (
        StatusCode::GONE,
        Json(ErrorBody {
            error: format!("Request with id \"{}\" has already been fulfilled", id),
        }),
    )
        .into_response()
}

fn bad_request(msg: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorBody {
            error: msg.to_string(),
        }),
    )
        .into_response()
}

fn is_eth_address(value: &str) -> bool {
    value
        .strip_prefix("0x")
        .is_some_and(|hex| hex.len() == 40 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn validate_outcome_body(body: &HttpOutcomeMessage) -> Result<(), String> {
    if !is_eth_address(&body.sender) {
        return Err("sender must be a valid address".into());
    }
    match (body.result.is_some(), body.error.is_some()) {
        (true, false) | (false, true) => Ok(()),
        (false, false) => Err("Outcome must include either a result or an error".into()),
        (true, true) => Err("Outcome must include either a result or an error, not both".into()),
    }
}

fn is_valid_uuid(s: &str) -> bool {
    // Upstream auth-server accepts only the hyphenated UUID v4 form
    // (RFC 4122 variant); Uuid::parse_str alone also admits simple, braced,
    // and urn forms plus other versions, so those must be rejected here.
    s.len() == 36
        && Uuid::parse_str(s).is_ok_and(|u| {
            u.get_version_num() == 4 && matches!(u.get_variant(), uuid::Variant::RFC4122)
        })
}

fn client_ip(headers: &HeaderMap) -> String {
    if let Some(value) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
        let ip = value.trim();
        if !ip.is_empty() {
            return normalize_ip(ip);
        }
    }
    if let Some(value) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(last) = value.rsplit(',').next() {
            let ip = last.trim();
            if !ip.is_empty() {
                return normalize_ip(ip);
            }
        }
    }
    String::new()
}

fn normalize_ip(ip: &str) -> String {
    ip.strip_prefix("::ffff:").unwrap_or(ip).trim().to_string()
}

fn ips_match(stored: &str, request: &str) -> bool {
    if stored.is_empty() || request.is_empty() {
        return false;
    }
    let a = normalize_ip(stored);
    let b = normalize_ip(request);
    if a == b {
        return true;
    }
    let octets_a: Vec<&str> = a.split('.').collect();
    let octets_b: Vec<&str> = b.split('.').collect();
    if octets_a.len() == 4
        && octets_b.len() == 4
        && octets_a.iter().all(|o| o.parse::<u8>().is_ok())
        && octets_b.iter().all(|o| o.parse::<u8>().is_ok())
    {
        return octets_a[..3] == octets_b[..3];
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderName;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(
                HeaderName::from_bytes(name.as_bytes()).unwrap(),
                value.parse().unwrap(),
            );
        }
        map
    }

    #[test]
    fn client_ip_ignores_client_settable_spoof_headers() {
        let map = headers(&[
            ("true-client-ip", "6.6.6.6"),
            ("cf-connecting-ip", "7.7.7.7"),
            ("x-real-ip", "203.0.113.9"),
        ]);
        assert_eq!(client_ip(&map), "203.0.113.9");
    }

    #[test]
    fn client_ip_takes_proxy_appended_forwarded_entry() {
        let map = headers(&[
            ("true-client-ip", "6.6.6.6"),
            ("x-forwarded-for", "6.6.6.6, 203.0.113.9"),
        ]);
        assert_eq!(client_ip(&map), "203.0.113.9");
    }

    #[test]
    fn client_ip_empty_when_only_spoofable_headers_present() {
        let map = headers(&[
            ("true-client-ip", "6.6.6.6"),
            ("cf-connecting-ip", "7.7.7.7"),
        ]);
        assert_eq!(client_ip(&map), "");
    }

    #[test]
    fn pairing_code_within_range() {
        for _ in 0..1000 {
            assert!(random_pairing_code() < 100);
        }
    }

    #[test]
    fn pairing_code_independent_of_challenge_timestamp() {
        let now = Utc::now();
        let stamp_code = now.timestamp_subsec_nanos() % 100;
        let codes: std::collections::HashSet<u32> =
            (0..128).map(|_| random_pairing_code()).collect();
        assert!(codes.len() > 1);
        assert!(codes.iter().any(|c| *c != stamp_code));
        let challenge = build_challenge("req-1", 42, &now, "personal_sign", &[]);
        assert!(challenge.contains("Code: 42"));
    }

    #[test]
    fn request_ttl_matches_the_upstream_default() {
        assert_eq!(REQUEST_TTL_SECONDS, 300);
    }

    fn outcome(
        sender: &str,
        result: Option<Value>,
        error: Option<OutcomeError>,
    ) -> HttpOutcomeMessage {
        HttpOutcomeMessage {
            sender: sender.into(),
            result,
            error,
            auth_chain: None,
        }
    }

    #[test]
    fn outcome_sender_must_be_an_eth_address() {
        let sig = Some(Value::from("0xdeadbeef"));
        assert!(validate_outcome_body(&outcome(
            "0x1234567890123456789012345678901234567890",
            sig.clone(),
            None
        ))
        .is_ok());
        for bad in [
            "",
            "not-an-address",
            "0x123",
            "1234567890123456789012345678901234567890",
            "0x123456789012345678901234567890123456789z",
        ] {
            assert_eq!(
                validate_outcome_body(&outcome(bad, sig.clone(), None)).unwrap_err(),
                "sender must be a valid address"
            );
        }
    }

    #[test]
    fn uuid_accepts_hyphenated_v4_in_either_case() {
        assert!(is_valid_uuid("8c545b52-c08e-4c39-a854-46e2b5b03c48"));
        assert!(is_valid_uuid("8C545B52-C08E-4C39-A854-46E2B5B03C48"));
    }

    #[test]
    fn uuid_rejects_non_v4_versions_and_nil() {
        // v1 time-based
        assert!(!is_valid_uuid("c232ab00-9414-11ec-b909-0242ac120002"));
        // nil UUID (version 0)
        assert!(!is_valid_uuid("00000000-0000-0000-0000-000000000000"));
    }

    #[test]
    fn uuid_rejects_non_hyphenated_encodings() {
        // simple (32-char) form
        assert!(!is_valid_uuid("8c545b52c08e4c39a85446e2b5b03c48"));
        // braced form
        assert!(!is_valid_uuid("{8c545b52-c08e-4c39-a854-46e2b5b03c48}"));
        // urn form
        assert!(!is_valid_uuid(
            "urn:uuid:8c545b52-c08e-4c39-a854-46e2b5b03c48"
        ));
    }

    #[test]
    fn uuid_rejects_non_rfc4122_variant_nibbles() {
        // variant nibble 'c' = Microsoft reserved
        assert!(!is_valid_uuid("8c545b52-c08e-4c39-c854-46e2b5b03c48"));
        // variant nibble '7' = NCS
        assert!(!is_valid_uuid("8c545b52-c08e-4c39-7854-46e2b5b03c48"));
    }

    #[test]
    fn outcome_requires_exactly_one_of_result_or_error() {
        let addr = "0x1234567890123456789012345678901234567890";
        let err = OutcomeError {
            code: 4001,
            message: "User rejected the request".into(),
            data: None,
        };
        assert!(validate_outcome_body(&outcome(addr, Some(Value::from("0xsig")), None)).is_ok());
        assert!(validate_outcome_body(&outcome(addr, None, Some(err.clone()))).is_ok());
        assert_eq!(
            validate_outcome_body(&outcome(addr, None, None)).unwrap_err(),
            "Outcome must include either a result or an error"
        );
        assert_eq!(
            validate_outcome_body(&outcome(addr, Some(Value::from("0xsig")), Some(err)))
                .unwrap_err(),
            "Outcome must include either a result or an error, not both"
        );
    }
}
