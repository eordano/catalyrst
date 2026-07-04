use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};

use crate::http::errors::ApiError;
use crate::AppState;

const DEFAULT_LIMIT: i64 = 500;
const MAX_LIMIT: i64 = 5000;

fn clamp_limit(limit: Option<i64>) -> i64 {
    limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

#[derive(Debug, Deserialize)]
pub struct ChangesQuery {
    #[serde(default)]
    pub since: i64,
    #[serde(default)]
    pub limit: Option<i64>,
}

pub async fn snapshot(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(snapshot_view(state.places.writer_pool()).await?))
}

pub async fn changes(
    State(state): State<AppState>,
    Query(q): Query<ChangesQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        changes_view(state.places.writer_pool(), q.since, clamp_limit(q.limit)).await?,
    ))
}

pub async fn snapshot_view(pool: Option<&PgPool>) -> Result<serde_json::Value, ApiError> {
    let Some(pool) = pool else {
        return Ok(empty_snapshot());
    };

    let latest_seq: i64 =
        sqlx::query_scalar("SELECT COALESCE(MAX(seq), 0) FROM signed_actions_places")
            .fetch_one(pool)
            .await?;

    let by_type: Vec<(String, i64)> = sqlx::query_as(
        "SELECT action_type, count(*)::bigint FROM signed_actions_places GROUP BY action_type ORDER BY action_type ASC",
    )
    .fetch_all(pool)
    .await?;
    let action_count: i64 = by_type.iter().map(|(_, c)| c).sum();
    let actions_by_type: serde_json::Map<String, serde_json::Value> = by_type
        .into_iter()
        .map(|(t, c)| (t, serde_json::json!(c)))
        .collect();

    let sig_hashes: Vec<(String,)> = sqlx::query_as(
        "SELECT signature_hash FROM signed_actions_places ORDER BY signature_hash ASC",
    )
    .fetch_all(pool)
    .await?;
    let mut h = Sha256::new();
    for (s,) in &sig_hashes {
        h.update(s.as_bytes());
        h.update(b"\n");
    }
    let log_hash = hex_encode(&h.finalize());

    let favorites_total: i64 = sqlx::query_scalar("SELECT count(*)::bigint FROM user_favorites")
        .fetch_one(pool)
        .await?;
    let votes_total: i64 = sqlx::query_scalar("SELECT count(*)::bigint FROM user_likes")
        .fetch_one(pool)
        .await?;
    let reports_total: i64 = sqlx::query_scalar("SELECT count(*)::bigint FROM place_reports_local")
        .fetch_one(pool)
        .await?;

    Ok(serde_json::json!({
        "scope": "places",
        "latest_seq": latest_seq,
        "action_count": action_count,
        "actions_by_type": actions_by_type,
        "log_hash": log_hash,
        "current": {
            "favorites": favorites_total,
            "votes": votes_total,
            "reports": reports_total,
        },
    }))
}

pub async fn changes_view(
    pool: Option<&PgPool>,
    since: i64,
    limit: i64,
) -> Result<serde_json::Value, ApiError> {
    let Some(pool) = pool else {
        return Ok(
            serde_json::json!({ "since": since, "limit": limit, "actions": [], "latest_seq": 0 }),
        );
    };

    let rows = sqlx::query(
        "SELECT seq, signature_hash, signer, place_id, action_type, message_payload, signed_at, origin_peer \
         FROM signed_actions_places WHERE seq > $1 ORDER BY seq ASC LIMIT $2",
    )
    .bind(since)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    let mut max_seq = since;
    let actions: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            let seq: i64 = r.get("seq");
            max_seq = max_seq.max(seq);
            serde_json::json!({
                "seq": seq,
                "signature_hash": r.get::<String, _>("signature_hash"),
                "signer": r.get::<String, _>("signer"),
                "place_id": r.get::<String, _>("place_id"),
                "action_type": r.get::<String, _>("action_type"),
                "payload": r.get::<serde_json::Value, _>("message_payload"),
                "signed_at": r.get::<i64, _>("signed_at"),
                "origin_peer": r.get::<Option<String>, _>("origin_peer"),
            })
        })
        .collect();

    Ok(serde_json::json!({
        "since": since,
        "limit": limit,
        "latest_seq": max_seq,
        "actions": actions,
    }))
}

fn empty_snapshot() -> serde_json::Value {
    serde_json::json!({
        "scope": "places",
        "latest_seq": 0,
        "action_count": 0,
        "actions_by_type": {},
        "log_hash": hex_encode(&Sha256::digest([])),
        "current": { "favorites": 0, "votes": 0, "reports": 0 },
    })
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_clamps_to_bounds() {
        assert_eq!(clamp_limit(None), DEFAULT_LIMIT);
        assert_eq!(clamp_limit(Some(0)), 1);
        assert_eq!(clamp_limit(Some(-9)), 1);
        assert_eq!(clamp_limit(Some(42)), 42);
        assert_eq!(clamp_limit(Some(1_000_000)), MAX_LIMIT);
    }

    #[test]
    fn empty_snapshot_is_well_formed_and_deterministic() {
        let a = empty_snapshot();
        let b = empty_snapshot();
        assert_eq!(a, b);
        assert_eq!(a["scope"], "places");
        assert_eq!(a["latest_seq"], 0);
        assert_eq!(a["action_count"], 0);

        assert_eq!(
            a["log_hash"],
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[tokio::test]
    async fn no_writer_yields_empty_but_well_formed_views() {
        let snap = snapshot_view(None).await.unwrap();
        assert_eq!(snap["latest_seq"], 0);
        assert_eq!(snap["current"]["favorites"], 0);

        let page = changes_view(None, 7, 100).await.unwrap();
        assert_eq!(page["since"], 7);
        assert_eq!(page["limit"], 100);
        assert!(page["actions"].as_array().unwrap().is_empty());
    }
}
