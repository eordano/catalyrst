use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::rest::auth_chain::require_signer;
use crate::rest::community_membership_authority::{
    load_standing_from_community_members, CommunityMembershipTier,
};
use crate::rest::fed::apply;
use crate::rest::fed::authority::FederatedCommunityWriteAuthority;
use crate::rest::fed::ids::community_uuid_from_hex;
use crate::rest::fed::messages::CommunityRequestStatusUpdate;
use crate::rest::handlers::permissions::Permission;
use crate::rest::AppState;

use super::{
    emit_gossip, err_json, into_resp, map_apply_err, map_refusal_using_its_own_detail, ok_json,
    preflight, uuid_from_path,
};

#[derive(Debug, Deserialize)]
pub struct CreateRequestBody {
    #[serde(rename = "targetedAddress", default)]
    pub targeted_address: Option<String>,
    #[serde(rename = "type")]
    pub kind: String,
}

#[utoipa::path(
    post,
    path = "/v1/communities/{id}/requests",
    tag = "requests",
    params(("id" = String, Path)),
    request_body(content = serde_json::Value, description = "{ type, targetedAddress? }"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn create_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    let community_uuid = match uuid_from_path(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/requests", id);
    let signer = match require_signer(&headers, "post", &path).await {
        Ok(s) => s.as_str().to_string(),
        Err(e) => return crate::rest::handlers::error::signed_fetch_gate_json(e),
    };
    let req: CreateRequestBody = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => return err_json(StatusCode::BAD_REQUEST, format!("invalid body: {}", e)),
    };

    let kind = match req.kind.as_str() {
        "invite" => "invite",
        "request_to_join" => "request_to_join",
        other => {
            return err_json(
                StatusCode::BAD_REQUEST,
                format!("invalid request type: {}", other),
            )
        }
    };

    let member_address = req
        .targeted_address
        .as_deref()
        .map(|s| s.to_lowercase())
        .unwrap_or_else(|| signer.clone());

    let community: Option<(bool, bool)> =
        sqlx::query_as("SELECT active, private FROM communities WHERE id = $1")
            .bind(community_uuid)
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten();
    let (active, private) = match community {
        Some((active, private)) => (active, private),
        None => {
            return err_json(
                StatusCode::NOT_FOUND,
                format!("Community not found: {}", id),
            )
        }
    };
    if !active {
        return err_json(StatusCode::BAD_REQUEST, "Community is not active");
    }

    let banned: Option<bool> = sqlx::query_scalar(
        "SELECT active FROM community_bans WHERE community_id = $1 AND banned_address = $2",
    )
    .bind(community_uuid)
    .bind(&member_address)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();
    if banned.unwrap_or(false) {
        return err_json(
            StatusCode::UNAUTHORIZED,
            format!(
                "The user {} is banned from the community {}",
                member_address, id
            ),
        );
    }

    if !private && kind == "request_to_join" {
        return err_json(
            StatusCode::BAD_REQUEST,
            "Public communities do not accept requests to join",
        );
    }

    let subject_standing =
        match load_standing_from_community_members(&state.pool, community_uuid, &member_address)
            .await
        {
            Ok(s) => s,
            Err(refusal) => {
                return map_refusal_using_its_own_detail(&refusal, StatusCode::FORBIDDEN)
            }
        };
    if subject_standing.stored_role_text_defaulting_to_none_when_no_row_exists() != "none" {
        return err_json(
            StatusCode::BAD_REQUEST,
            "User cannot join since it is already a member of the community",
        );
    }

    if kind == "invite" {
        let caller_standing = match load_standing_from_community_members(
            &state.pool,
            community_uuid,
            &signer,
        )
        .await
        {
            Ok(s) => s,
            Err(refusal) => {
                return map_refusal_using_its_own_detail(&refusal, StatusCode::FORBIDDEN)
            }
        };
        if !caller_standing.holds_capability_within_this_community(Permission::InviteUsers) {
            return err_json(
                StatusCode::UNAUTHORIZED,
                format!(
                    "The user {} doesn't have permission to invite users",
                    signer
                ),
            );
        }
    } else if member_address != signer {
        return err_json(
            StatusCode::BAD_REQUEST,
            "User trying to impersonate another user",
        );
    }

    let pending: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, type FROM community_requests \
         WHERE community_id = $1 AND member_address = $2 AND status = 'pending' \
         ORDER BY created_at ASC LIMIT 2",
    )
    .bind(community_uuid)
    .bind(&member_address)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    if let Some((rid, _)) = pending.iter().find(|(_, t)| t == kind) {
        return (
            StatusCode::OK,
            Json(json!({
                "data": {
                    "id": rid,
                    "communityId": community_uuid,
                    "memberAddress": member_address,
                    "type": kind,
                    "status": "pending",
                }
            })),
        );
    }

    if let Some((opp_id, _)) = pending.iter().find(|(_, t)| t != kind) {
        let mut tx = match state.pool.begin().await {
            Ok(t) => t,
            Err(e) => {
                tracing::error!(error = %e, "failed to open tx for request auto-accept");
                return err_json(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "failed to create request",
                );
            }
        };
        let join_ok = sqlx::query(
            "INSERT INTO community_members (community_id, member_address, role, joined_at) \
             VALUES ($1, $2, 'member', now()) ON CONFLICT (community_id, member_address) DO NOTHING",
        )
        .bind(community_uuid)
        .bind(&member_address)
        .execute(&mut *tx)
        .await;
        let del_ok = sqlx::query(
            "DELETE FROM community_requests WHERE community_id = $1 AND member_address = $2",
        )
        .bind(community_uuid)
        .bind(&member_address)
        .execute(&mut *tx)
        .await;
        if join_ok.is_err() || del_ok.is_err() || tx.commit().await.is_err() {
            return err_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to create request",
            );
        }
        return (
            StatusCode::OK,
            Json(json!({
                "data": {
                    "id": opp_id,
                    "communityId": community_uuid,
                    "memberAddress": member_address,
                    "type": kind,
                    "status": "accepted",
                }
            })),
        );
    }

    let request_id = Uuid::new_v4();
    let inserted = sqlx::query_as::<_, (Uuid, Uuid, String, String, String)>(
        "INSERT INTO community_requests (id, community_id, member_address, status, type) \
         VALUES ($1, $2, $3, 'pending', $4) \
         RETURNING id, community_id, member_address, status, type",
    )
    .bind(request_id)
    .bind(community_uuid)
    .bind(&member_address)
    .bind(kind)
    .fetch_one(&state.pool)
    .await;

    match inserted {
        Ok((id, community_id, member_address, status, kind)) => (
            StatusCode::OK,
            Json(json!({
                "data": {
                    "id": id,
                    "communityId": community_id,
                    "memberAddress": member_address,
                    "type": kind,
                    "status": status,
                }
            })),
        ),
        Err(e) => {
            tracing::error!(error = %e, "failed to create community request");
            err_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to create request",
            )
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct PathIdReq {
    pub id: String,
    #[serde(rename = "requestId")]
    pub request_id: String,
}

#[utoipa::path(
    patch,
    path = "/v1/communities/{id}/requests/{requestId}",
    tag = "requests",
    params(("id" = String, Path), ("requestId" = String, Path)),
    request_body(content = serde_json::Value, description = "{ intention } for client, or EIP-712 federation envelope"),
    responses(
        (status = 204),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 403, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 409, body = catalyrst_types::ApiErrorBody),
        (status = 429, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn update_request_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdReq { id, request_id }): Path<PathIdReq>,
    body: Bytes,
) -> axum::response::Response {
    if !crate::rest::handlers::client::is_federation_envelope(&body) {
        return crate::rest::handlers::client::update_request_status(
            State(state),
            headers,
            Path(crate::rest::handlers::client::PathIdReq { id, request_id }),
            body,
        )
        .await;
    }
    into_resp(
        fed_update_request_status(
            State(state),
            headers,
            Path(PathIdReq { id, request_id }),
            body,
        )
        .await,
    )
}

async fn fed_update_request_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdReq { id, request_id }): Path<PathIdReq>,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    let uuid = match uuid_from_path(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/requests/{}", id, request_id);
    let (signed, signer) =
        match preflight::<CommunityRequestStatusUpdate>(&state, &headers, "patch", &path, &body)
            .await
        {
            Ok(x) => x,
            Err(e) => return e,
        };
    if community_uuid_from_hex(&signed.message.community_id) != uuid {
        return err_json(StatusCode::BAD_REQUEST, "community_id mismatch");
    }
    if signed.message.request_id != request_id {
        return err_json(StatusCode::BAD_REQUEST, "request_id mismatch");
    }
    if let Err(refusal) = FederatedCommunityWriteAuthority::resolve_requiring_at_least(
        &state.pool,
        &signed.message.community_id,
        signer.as_str(),
        CommunityMembershipTier::ModeratorOfThisCommunity,
    )
    .await
    {
        return map_refusal_using_its_own_detail(&refusal, StatusCode::FORBIDDEN);
    }
    match apply::apply_request_status(&state.pool, &signed, signer.as_str()).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, signer.as_str()).await;
            ok_json(sig)
        }
        Err(e) => map_apply_err(e),
    }
}
