use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::http::ApiError;
use crate::AppState;

type RoleLogRow = (
    String,
    String,
    String,
    String,
    String,
    i64,
    serde_json::Value,
    i64,
);

#[derive(Debug, Deserialize)]
pub struct ChangesQuery {
    #[serde(default)]
    pub since: i64,
    #[serde(default)]
    pub limit: Option<i64>,
}

pub async fn snapshot(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    let latest_role: Option<(i64,)> =
        sqlx::query_as("SELECT COALESCE(MAX(seq), 0) FROM community_role_log")
            .fetch_optional(&state.pool)
            .await
            .map_err(ApiError::from)?;
    let latest_posts: Option<(i64,)> =
        sqlx::query_as("SELECT COALESCE(MAX(seq), 0) FROM community_posts_log")
            .fetch_optional(&state.pool)
            .await
            .map_err(ApiError::from)?;
    let latest_places: Option<(i64,)> =
        sqlx::query_as("SELECT COALESCE(MAX(seq), 0) FROM community_places_log")
            .fetch_optional(&state.pool)
            .await
            .map_err(ApiError::from)?;

    let communities: Vec<(String, String, String, i64)> = sqlx::query_as(
        "SELECT community_id, creator, name, signed_at FROM communities_local ORDER BY seq ASC LIMIT 1000",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let role_hashes: Vec<(String,)> =
        sqlx::query_as("SELECT signature_hash FROM community_role_log ORDER BY signature_hash ASC")
            .fetch_all(&state.pool)
            .await
            .map_err(ApiError::from)?;

    let mut h = Sha256::new();
    for (s,) in &role_hashes {
        h.update(s.as_bytes());
    }
    let role_log_hash = hex::encode(h.finalize());

    let comms_json: Vec<serde_json::Value> = communities
        .into_iter()
        .map(|(community_id, creator, name, signed_at)| {
            serde_json::json!({
                "community_id": community_id,
                "creator": creator,
                "name": name,
                "signed_at": signed_at,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "latest_role_seq": latest_role.map(|(s,)| s).unwrap_or(0),
        "latest_posts_seq": latest_posts.map(|(s,)| s).unwrap_or(0),
        "latest_places_seq": latest_places.map(|(s,)| s).unwrap_or(0),
        "communities": comms_json,
        "role_log_hash": role_log_hash,
    })))
}

pub async fn changes(
    State(state): State<AppState>,
    Query(q): Query<ChangesQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let limit = q.limit.unwrap_or(500).clamp(1, 5000);

    let role: Vec<RoleLogRow> =
        sqlx::query_as(
            "SELECT signature_hash, community_id, signer, target, role, signed_at, message_payload, seq \
             FROM community_role_log WHERE seq > $1 ORDER BY seq ASC LIMIT $2",
        )
        .bind(q.since)
        .bind(limit)
        .fetch_all(&state.pool)
        .await
        .map_err(ApiError::from)?;

    let posts: Vec<(String, String, String, String, i64, i64)> = sqlx::query_as(
        "SELECT signature_hash, community_id, author, content_hash, signed_at, seq \
         FROM community_posts_log WHERE seq > $1 ORDER BY seq ASC LIMIT $2",
    )
    .bind(q.since)
    .bind(limit)
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let places: Vec<(String, String, String, String, String, i64, i64)> = sqlx::query_as(
        "SELECT signature_hash, community_id, place_id, action, signer, signed_at, seq \
         FROM community_places_log WHERE seq > $1 ORDER BY seq ASC LIMIT $2",
    )
    .bind(q.since)
    .bind(limit)
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let role_json: Vec<serde_json::Value> = role
        .into_iter()
        .map(
            |(sig, cid, signer, target, role, signed_at, payload, seq)| {
                serde_json::json!({
                    "kind": "role",
                    "signature_hash": sig,
                    "community_id": cid,
                    "signer": signer,
                    "target": target,
                    "role": role,
                    "signed_at": signed_at,
                    "payload": payload,
                    "seq": seq,
                })
            },
        )
        .collect();

    let posts_json: Vec<serde_json::Value> = posts
        .into_iter()
        .map(|(sig, cid, author, content_hash, signed_at, seq)| {
            serde_json::json!({
                "kind": "post",
                "signature_hash": sig,
                "community_id": cid,
                "author": author,
                "content_hash": content_hash,
                "signed_at": signed_at,
                "seq": seq,
            })
        })
        .collect();

    let places_json: Vec<serde_json::Value> = places
        .into_iter()
        .map(|(sig, cid, pid, action, signer, signed_at, seq)| {
            serde_json::json!({
                "kind": "place",
                "signature_hash": sig,
                "community_id": cid,
                "place_id": pid,
                "action": action,
                "signer": signer,
                "signed_at": signed_at,
                "seq": seq,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "role": role_json,
        "posts": posts_json,
        "places": places_json,
    })))
}
