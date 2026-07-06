use axum::extract::State;
use axum::Json;
use chrono::SecondsFormat;
use serde_json::{json, Value};

use crate::AppState;

pub async fn live_data(State(state): State<AppState>) -> Json<Value> {
    let counts = state.presence.world_counts();
    let total: i64 = counts.iter().map(|(_, c)| c).sum();
    let per_world: Vec<Value> = counts
        .into_iter()
        .map(|(world_name, users)| json!({ "worldName": world_name, "users": users }))
        .collect();
    Json(json!({
        "data": { "totalUsers": total, "perWorld": per_world },
        "lastUpdated": chrono::Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
    }))
}
