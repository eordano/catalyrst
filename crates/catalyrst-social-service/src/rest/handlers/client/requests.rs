use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use uuid::Uuid;

use crate::rest::handlers::permissions::{has_permission, Permission};
use crate::rest::AppState;

use super::{auth, err, load_role_uuid, map_db, parse_uuid};

#[derive(Debug, Deserialize)]
pub struct PathIdReq {
    pub id: String,
    #[serde(rename = "requestId")]
    pub request_id: String,
}

#[derive(Debug, Deserialize)]
pub struct RequestStatusBody {
    pub intention: String,
}

pub async fn update_request_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdReq { id, request_id }): Path<PathIdReq>,
    body: Bytes,
) -> Response {
    let uuid = match parse_uuid(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let req_uuid = match Uuid::parse_str(&request_id) {
        Ok(u) => u,
        Err(_) => return err(StatusCode::BAD_REQUEST, "invalid request id"),
    };
    let path = format!("/v1/communities/{}/requests/{}", id, request_id);
    let signer = match auth(&headers, "patch", &path) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let parsed: RequestStatusBody = match serde_json::from_slice(&body) {
        Ok(b) => b,
        Err(e) => return err(StatusCode::BAD_REQUEST, format!("invalid body: {}", e)),
    };
    let status = match parsed.intention.to_lowercase().as_str() {
        "accepted" => "accepted",
        "rejected" => "rejected",
        "cancelled" => "cancelled",
        other => {
            return err(
                StatusCode::BAD_REQUEST,
                format!("invalid intention: {}", other),
            )
        }
    };

    let row: Option<(String, String, String)> = match map_db(
        sqlx::query_as(
            "SELECT member_address, type, status FROM community_requests WHERE id = $1 AND community_id = $2",
        )
        .bind(req_uuid)
        .bind(uuid)
        .fetch_optional(&state.pool)
        .await,
    ) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let Some((member_address, kind, _cur)) = row else {
        return err(StatusCode::NOT_FOUND, "Request not found");
    };

    let self_caller = member_address.eq_ignore_ascii_case(&signer);
    let manager_check = || async {
        let role = load_role_uuid(&state, uuid, &signer).await;
        if has_permission(role, Permission::AcceptRequests)
            && has_permission(role, Permission::RejectRequests)
        {
            Ok(())
        } else {
            Err(err(
                StatusCode::UNAUTHORIZED,
                format!(
                    "The user {} doesn't have permission to accept and reject requests",
                    signer
                ),
            ))
        }
    };
    let auth_err: Option<Response> = match (kind.as_str(), status) {
        ("invite", "cancelled") => {
            if self_caller {
                Some(err(
                    StatusCode::UNAUTHORIZED,
                    "Invited user cannot cancel their invite",
                ))
            } else {
                manager_check().await.err()
            }
        }
        ("invite", _) => {
            if self_caller {
                None
            } else {
                Some(err(
                    StatusCode::UNAUTHORIZED,
                    "Only invited user can accept or reject invites",
                ))
            }
        }
        ("request_to_join", "cancelled") => {
            if self_caller {
                None
            } else {
                Some(err(
                    StatusCode::UNAUTHORIZED,
                    "Only requesting user can cancel their request",
                ))
            }
        }
        ("request_to_join", _) => {
            if self_caller {
                Some(err(
                    StatusCode::UNAUTHORIZED,
                    "Requesting user cannot accept or reject their own request",
                ))
            } else {
                manager_check().await.err()
            }
        }
        _ => None,
    };
    if let Some(e) = auth_err {
        return e;
    }

    if status == "accepted" {
        let banned: Option<bool> = sqlx::query_scalar(
            "SELECT active FROM community_bans WHERE community_id = $1 AND banned_address = $2",
        )
        .bind(uuid)
        .bind(&member_address)
        .fetch_optional(&state.pool)
        .await
        .ok()
        .flatten();
        if banned.unwrap_or(false) {
            return err(
                StatusCode::UNAUTHORIZED,
                format!(
                    "The user {} is banned from the community {}",
                    member_address, uuid
                ),
            );
        }
    }

    let upd =
        sqlx::query("UPDATE community_requests SET status = $2, updated_at = now() WHERE id = $1")
            .bind(req_uuid)
            .bind(status)
            .execute(&state.pool)
            .await;
    if let Err(e) = map_db(upd) {
        return e;
    }

    if status == "accepted" {
        let _ = sqlx::query(
            "INSERT INTO community_members (community_id, member_address, role, joined_at) \
             VALUES ($1, $2, 'member', now()) ON CONFLICT (community_id, member_address) DO NOTHING",
        )
        .bind(uuid)
        .bind(&member_address)
        .execute(&state.pool)
        .await;
    }
    StatusCode::NO_CONTENT.into_response()
}
