use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::rest::fed::authority::Role;
use crate::rest::handlers::permissions::{has_permission, Permission};
use crate::rest::AppState;

use super::{
    auth, err, load_role_uuid, map_db, parse_uuid, require_permission_uuid,
    validate_like_unlike_access,
};

#[derive(Debug, Deserialize)]
pub struct PostBody {
    pub content: String,
}

pub async fn create_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    let uuid = match parse_uuid(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/posts", id);
    let signer = match auth(&headers, "post", &path) {
        Ok(s) => s,
        Err(e) => return e,
    };

    if let Err(e) = require_permission_uuid(
        &state,
        uuid,
        &signer,
        Permission::CreatePosts,
        "create posts in the community",
    )
    .await
    {
        return e;
    }
    let parsed: PostBody = match serde_json::from_slice(&body) {
        Ok(b) => b,
        Err(e) => return err(StatusCode::BAD_REQUEST, format!("invalid body: {}", e)),
    };
    let content = parsed.content.trim().to_string();
    if content.is_empty() {
        return err(StatusCode::BAD_REQUEST, "content is required");
    }
    let post_id = Uuid::new_v4();
    let ins = sqlx::query_as::<_, (Uuid, chrono::NaiveDateTime)>(
        "INSERT INTO community_posts (id, community_id, author_address, content, created_at) \
         VALUES ($1, $2, $3, $4, now()) RETURNING id, created_at",
    )
    .bind(post_id)
    .bind(uuid)
    .bind(&signer)
    .bind(&content)
    .fetch_one(&state.pool)
    .await;
    let (post_id, created_at) = match map_db(ins) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let data = json!({
        "id": post_id,
        "communityId": uuid,
        "authorAddress": signer,
        "content": content,
        "createdAt": created_at,
    });
    (StatusCode::CREATED, Json(json!({ "data": data }))).into_response()
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
) -> Response {
    let uuid = match parse_uuid(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let post_uuid = match Uuid::parse_str(&post_id) {
        Ok(u) => u,
        Err(_) => return err(StatusCode::BAD_REQUEST, "invalid post id"),
    };
    let path = format!("/v1/communities/{}/posts/{}", id, post_id);
    let signer = match auth(&headers, "delete", &path) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let author: Option<String> = match map_db(
        sqlx::query_scalar(
            "SELECT author_address FROM community_posts WHERE id = $1 AND community_id = $2",
        )
        .bind(post_uuid)
        .bind(uuid)
        .fetch_optional(&state.pool)
        .await,
    ) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let Some(author) = author else {
        return err(StatusCode::NOT_FOUND, "Post not found");
    };

    let role = load_role_uuid(&state, uuid, &signer).await;
    let is_author = author.eq_ignore_ascii_case(&signer);
    if !has_permission(role, Permission::DeletePosts) || (role == Role::Mod && !is_author) {
        return err(
            StatusCode::UNAUTHORIZED,
            format!(
                "The user {} doesn't have permission to delete posts from the community",
                signer
            ),
        );
    }
    let del = sqlx::query("DELETE FROM community_posts WHERE id = $1 AND community_id = $2")
        .bind(post_uuid)
        .bind(uuid)
        .execute(&state.pool)
        .await;
    if let Err(e) = map_db(del) {
        return e;
    }
    StatusCode::NO_CONTENT.into_response()
}

pub async fn like_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdPost { id, post_id }): Path<PathIdPost>,
) -> Response {
    let uuid = match parse_uuid(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let post_uuid = match Uuid::parse_str(&post_id) {
        Ok(u) => u,
        Err(_) => return err(StatusCode::BAD_REQUEST, "invalid post id"),
    };
    let path = format!("/v1/communities/{}/posts/{}/like", id, post_id);
    let signer = match auth(&headers, "post", &path) {
        Ok(s) => s,
        Err(e) => return e,
    };
    if let Err(e) = validate_like_unlike_access(&state, uuid, &signer).await {
        return e;
    }
    let exists: Option<bool> = match map_db(
        sqlx::query_scalar("SELECT TRUE FROM community_posts WHERE id = $1 AND community_id = $2")
            .bind(post_uuid)
            .bind(uuid)
            .fetch_optional(&state.pool)
            .await,
    ) {
        Ok(v) => v,
        Err(e) => return e,
    };
    if exists.is_none() {
        return err(StatusCode::NOT_FOUND, "Post not found");
    }
    let ins = sqlx::query(
        "INSERT INTO community_post_likes (post_id, user_address, liked_at) \
         VALUES ($1, $2, now()) ON CONFLICT (post_id, user_address) DO NOTHING",
    )
    .bind(post_uuid)
    .bind(&signer)
    .execute(&state.pool)
    .await;
    if let Err(e) = map_db(ins) {
        return e;
    }
    StatusCode::CREATED.into_response()
}

pub async fn unlike_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdPost { id, post_id }): Path<PathIdPost>,
) -> Response {
    let uuid = match parse_uuid(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let post_uuid = match Uuid::parse_str(&post_id) {
        Ok(u) => u,
        Err(_) => return err(StatusCode::BAD_REQUEST, "invalid post id"),
    };
    let path = format!("/v1/communities/{}/posts/{}/like", id, post_id);
    let signer = match auth(&headers, "delete", &path) {
        Ok(s) => s,
        Err(e) => return e,
    };
    if let Err(e) = validate_like_unlike_access(&state, uuid, &signer).await {
        return e;
    }
    let del =
        sqlx::query("DELETE FROM community_post_likes WHERE post_id = $1 AND user_address = $2")
            .bind(post_uuid)
            .bind(&signer)
            .execute(&state.pool)
            .await;
    if let Err(e) = map_db(del) {
        return e;
    }
    StatusCode::NO_CONTENT.into_response()
}
