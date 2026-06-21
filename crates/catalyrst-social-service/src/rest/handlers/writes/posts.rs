use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::rest::fed::apply;
use crate::rest::fed::authority::load_role;
use crate::rest::fed::ids::community_uuid_from_hex;
use crate::rest::fed::messages::{
    CommunityPost, CommunityPostDelete, CommunityPostLike, CommunityPostUnlike,
};
use crate::rest::handlers::permissions::{can_delete_post, Permission};
use crate::rest::AppState;

use super::{
    emit_gossip, err_json, into_resp, map_apply_err, ok_json, ok_json_with, preflight,
    require_like_permission, require_permission, uuid_from_path,
};

pub async fn create_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> axum::response::Response {
    if !crate::rest::handlers::client::is_federation_envelope(&body) {
        return crate::rest::handlers::client::create_post(State(state), headers, Path(id), body)
            .await;
    }
    into_resp(fed_create_post(State(state), headers, Path(id), body).await)
}

async fn fed_create_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    let uuid = match uuid_from_path(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/posts", id);
    let (signed, signer) =
        match preflight::<CommunityPost>(&state, &headers, "post", &path, &body).await {
            Ok(x) => x,
            Err(e) => return e,
        };
    if community_uuid_from_hex(&signed.message.community_id) != uuid {
        return err_json(StatusCode::BAD_REQUEST, "community_id mismatch");
    }

    if let Err(e) = require_permission(
        &state,
        &signed.message.community_id,
        &signer,
        Permission::CreatePosts,
        "create posts in the community",
    )
    .await
    {
        return e;
    }
    let content_hash_lc = signed.message.content_hash.to_ascii_lowercase();
    let body_present = crate::rest::content_store::is_valid_hash(&content_hash_lc)
        && state.content_store.exists(&content_hash_lc);
    if !body_present {
        tracing::debug!(
            community_id = %signed.message.community_id,
            content_hash = %signed.message.content_hash,
            signer = %signer,
            "CommunityPost accepted before content body is locally present; federation pull will fetch it"
        );
    }
    match apply::apply_post(&state.pool, &signed, &signer).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, &signer).await;
            ok_json_with(sig, json!({ "content_body_local": body_present }))
        }
        Err(e) => map_apply_err(e),
    }
}

#[derive(Debug, Deserialize)]
pub struct PathIdPost {
    pub id: String,
    #[serde(rename = "postId")]
    pub post_id: String,
}

pub async fn delete_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdPost { id, post_id }): Path<PathIdPost>,
    body: Bytes,
) -> axum::response::Response {
    if !crate::rest::handlers::client::is_federation_envelope(&body) {
        return crate::rest::handlers::client::delete_post(
            State(state),
            headers,
            Path(crate::rest::handlers::client::PathIdPost { id, post_id }),
        )
        .await;
    }
    into_resp(
        fed_delete_post(
            State(state),
            headers,
            Path(PathIdPost { id, post_id }),
            body,
        )
        .await,
    )
}

async fn fed_delete_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdPost { id, post_id }): Path<PathIdPost>,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    let _ = match uuid_from_path(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/posts/{}", id, post_id);
    let (signed, signer) =
        match preflight::<CommunityPostDelete>(&state, &headers, "delete", &path, &body).await {
            Ok(x) => x,
            Err(e) => return e,
        };
    let post_meta = post_meta(&state.pool, &signed.message.post_id).await;
    let community_id_for_post = post_meta
        .as_ref()
        .map(|(c, _)| c.clone())
        .unwrap_or_default();
    let author = post_meta.map(|(_, a)| a);

    if signed.message.community_id != community_id_for_post || signed.message.post_id != post_id {
        return err_json(StatusCode::BAD_REQUEST, "post_id / community_id mismatch");
    }
    let is_author = author
        .as_deref()
        .map(|a| a.eq_ignore_ascii_case(&signer))
        .unwrap_or(false);

    let role = match load_role(&state.pool, &signed.message.community_id, &signer).await {
        Ok(r) => r,
        Err(e) => return map_apply_err(e),
    };
    if !can_delete_post(role, is_author) {
        return err_json(
            StatusCode::UNAUTHORIZED,
            format!(
                "The user {} doesn't have permission to delete posts from the community",
                signer
            ),
        );
    }
    match apply::apply_post_delete(&state.pool, &signed).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, &signer).await;
            ok_json(sig)
        }
        Err(e) => map_apply_err(e),
    }
}

async fn post_meta(pool: &sqlx::PgPool, post_sig_hash: &str) -> Option<(String, String)> {
    sqlx::query_as::<_, (String, String)>(
        "SELECT community_id, author FROM community_posts_log WHERE signature_hash = $1",
    )
    .bind(post_sig_hash)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
}

pub async fn like_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdPost { id, post_id }): Path<PathIdPost>,
    body: Bytes,
) -> axum::response::Response {
    if !crate::rest::handlers::client::is_federation_envelope(&body) {
        return crate::rest::handlers::client::like_post(
            State(state),
            headers,
            Path(crate::rest::handlers::client::PathIdPost { id, post_id }),
        )
        .await;
    }
    into_resp(
        fed_like_post(
            State(state),
            headers,
            Path(PathIdPost { id, post_id }),
            body,
        )
        .await,
    )
}

async fn fed_like_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdPost { id, post_id }): Path<PathIdPost>,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    let _ = match uuid_from_path(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/posts/{}/like", id, post_id);
    let (signed, signer) =
        match preflight::<CommunityPostLike>(&state, &headers, "post", &path, &body).await {
            Ok(x) => x,
            Err(e) => return e,
        };
    if signed.message.post_id != post_id {
        return err_json(StatusCode::BAD_REQUEST, "post_id mismatch");
    }
    if let Err(e) = require_like_permission(&state, &signed.message.community_id, &signer).await {
        return e;
    }
    match apply::apply_post_like(&state.pool, &signed, &signer).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, &signer).await;
            ok_json(sig)
        }
        Err(e) => map_apply_err(e),
    }
}

pub async fn unlike_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdPost { id, post_id }): Path<PathIdPost>,
    body: Bytes,
) -> axum::response::Response {
    if !crate::rest::handlers::client::is_federation_envelope(&body) {
        return crate::rest::handlers::client::unlike_post(
            State(state),
            headers,
            Path(crate::rest::handlers::client::PathIdPost { id, post_id }),
        )
        .await;
    }
    into_resp(
        fed_unlike_post(
            State(state),
            headers,
            Path(PathIdPost { id, post_id }),
            body,
        )
        .await,
    )
}

async fn fed_unlike_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdPost { id, post_id }): Path<PathIdPost>,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    let _ = match uuid_from_path(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/posts/{}/like", id, post_id);
    let (signed, signer) =
        match preflight::<CommunityPostUnlike>(&state, &headers, "delete", &path, &body).await {
            Ok(x) => x,
            Err(e) => return e,
        };
    if signed.message.post_id != post_id {
        return err_json(StatusCode::BAD_REQUEST, "post_id mismatch");
    }
    if let Err(e) = require_like_permission(&state, &signed.message.community_id, &signer).await {
        return e;
    }
    match apply::apply_post_unlike(&state.pool, &signed, &signer).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, &signer).await;
            ok_json(sig)
        }
        Err(e) => map_apply_err(e),
    }
}
