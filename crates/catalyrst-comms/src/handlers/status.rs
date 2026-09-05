use axum::Json;

pub async fn status() -> Json<serde_json::Value> {
    let current_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    Json(serde_json::json!({
        "version": crate::VERSION,
        "currentTime": current_time,
        "commitHash": option_env!("GIT_COMMIT").unwrap_or(""),
    }))
}
