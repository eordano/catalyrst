use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::OnceLock;
use uuid::Uuid;

use catalyrst_crypto::signed_fetch::{self, SignedFetchPolicy};
use catalyrst_crypto::{reject_if_signer, Signer, SignerGate, Wallet};
use catalyrst_types::AuthChain;

use super::validation::validate_identity_chain;
use super::{
    bad_request, client_ip, ips_match, is_valid_uuid, normalize_ip, ErrorBody, EvictedBody,
};
use crate::AppState;

const IDENTITY_TTL_SECONDS: i64 = 3600;
// Upstream keeps the status tombstone for two weeks (auth-server storage,
// TWO_WEEKS_IN_SECONDS) so a late GET can still say why the identity is gone.
const IDENTITY_STATUS_TTL_SECONDS: i64 = 14 * 24 * 3600;
const MAX_PENDING_IDENTITIES: usize = 10_000;
const MAX_IDENTITY_TOMBSTONES: usize = 50_000;
// The /auth mount prefix is not part of the wire contract: upstream serves
// this route at /identities and the sites page signs the public pathname it
// fetches, so verification falls back to the unprefixed route and every vhost
// that mounts the crate under another prefix forwards the public path in
// x-original-path (01-catalyst.conf, 11-gateway.conf).
const IDENTITY_SIGNED_PATH: &str = "/identities";
const SIGNED_FETCH_TOLERANCE_SECS: i64 = 5 * 60;
const SCENE_SIGNER: &str = "decentraland-kernel-scene";

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
pub struct IdentityRecord {
    pub identity_id: String,
    pub identity: Value,
    pub ip_address: String,
    pub is_mobile: bool,
    pub created_at: DateTime<Utc>,
    pub expiration: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct IdentityRequestBody {
    pub identity: Value,
    #[serde(rename = "isMobile", default)]
    pub is_mobile: Option<bool>,
}

// The identity is stored and served verbatim; this typed view only runs
// upstream's create checks (auth-server createIdentityHandler) before the blob
// is accepted. `expiration` and `publicKey` are required by upstream's schema
// and never read by it either.
#[derive(Debug, Deserialize)]
struct StoredIdentity {
    #[allow(dead_code)]
    expiration: DateTime<Utc>,
    #[serde(rename = "ephemeralIdentity")]
    ephemeral_identity: EphemeralIdentity,
    #[serde(rename = "authChain")]
    auth_chain: AuthChain,
}

#[derive(Debug, Deserialize)]
struct EphemeralIdentity {
    address: String,
    #[serde(rename = "privateKey")]
    private_key: String,
    #[allow(dead_code)]
    #[serde(rename = "publicKey")]
    public_key: String,
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

pub(super) async fn create_identity(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<IdentityRequestBody>,
) -> Response {
    let request_signer = match verify_identity_post(&headers).await {
        Ok(signer) => signer,
        Err(refusal) => return refusal,
    };
    if body.identity.is_null() {
        return bad_request("AuthIdentity is required in request body");
    }
    let parsed: StoredIdentity = match serde_json::from_value(body.identity.clone()) {
        Ok(parsed) => parsed,
        Err(err) => return bad_request(&format!("Invalid AuthIdentity: {err}")),
    };
    let (owner, final_authority) = match validate_identity_chain(&parsed.auth_chain) {
        Ok(authorities) => authorities,
        Err(msg) => return bad_request(&msg),
    };
    if !parsed
        .ephemeral_identity
        .address
        .eq_ignore_ascii_case(&final_authority)
    {
        return forbidden("Ephemeral wallet address does not match auth chain final authority");
    }
    if !request_signer.as_str().eq_ignore_ascii_case(&owner) {
        return forbidden("Request sender does not match identity owner");
    }
    let derives_address =
        Wallet::from_hex(&parsed.ephemeral_identity.private_key).is_ok_and(|wallet| {
            wallet
                .address()
                .eq_ignore_ascii_case(&parsed.ephemeral_identity.address)
        });
    if !derives_address {
        return forbidden("Ephemeral private key does not match the provided address");
    }

    let signer = owner.to_lowercase();
    let identity_id = Uuid::new_v4().to_string();
    let now = Utc::now();
    sweep_expired_identities(&state, now);
    if state.auth_api.identities.len() >= MAX_PENDING_IDENTITIES {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorBody {
                error: "Too many pending identities, retry shortly".into(),
            }),
        )
            .into_response();
    }
    let expiration = now + Duration::seconds(IDENTITY_TTL_SECONDS);
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
    if state.auth_api.identity_status.len() < MAX_IDENTITY_TOMBSTONES {
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
    }
    (
        StatusCode::CREATED,
        Json(IdentityResponse {
            identity_id,
            expiration,
        }),
    )
        .into_response()
}

fn scene_signer_gate() -> &'static SignerGate {
    static GATE: OnceLock<SignerGate> = OnceLock::new();
    GATE.get_or_init(|| {
        reject_if_signer(&[SCENE_SIGNER]).expect("SCENE_SIGNER is canonical by construction")
    })
}

// Upstream's `createSignedFetchMiddleware()` on this route: current payload
// shape only, scenes turned away, and every refusal answered as 401 with the
// ADR-44 pointer.
async fn verify_identity_post(headers: &HeaderMap) -> Result<Signer, Response> {
    signed_fetch::verify_signed_fetch_meta_with_policy(
        headers,
        "post",
        IDENTITY_SIGNED_PATH,
        SIGNED_FETCH_TOLERANCE_SECS,
        SignedFetchPolicy::new(&[], Some(scene_signer_gate())),
        signed_fetch::default_eip1654_validator().map(|v| &**v),
    )
    .await
    .map(|(signer, _)| signer)
    .map_err(|err| {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": err.http_message(),
                "message": "This endpoint requires a signed fetch request. See ADR-44.",
            })),
        )
            .into_response()
    })
}

// Upstream leans on Redis TTLs (the identity for its hour, the tombstone for
// two weeks); these maps have no reaper, so each new identity pays for the
// sweep instead of a background task, and the caps keep a flood of
// throwaway-wallet posts from growing memory without bound: a full identities
// map refuses the login (the record is what the client comes back for), a full
// tombstone map only drops the diagnostic 404 texts.
fn sweep_expired_identities(state: &AppState, now: DateTime<Utc>) {
    state
        .auth_api
        .identities
        .retain(|_, record| record.expiration >= now);
    let horizon = now - Duration::seconds(IDENTITY_STATUS_TTL_SECONDS);
    state
        .auth_api
        .identity_status
        .retain(|_, status| status.created_at >= horizon);
}

pub(super) async fn get_identity(
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

fn forbidden(msg: &str) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(ErrorBody {
            error: msg.to_string(),
        }),
    )
        .into_response()
}
